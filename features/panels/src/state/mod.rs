//! Side region per-session state, resolved strip projection, and query helpers.

mod actions;
pub mod flight;
mod resolve;
pub mod width_hold;

pub use width_hold::{MouseClose, StripScroll, StripWidthHold, WidthHoldEvent, release_pane_width_hold, release_strip_width_holds};

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Duration;

use crate::closing;
use crate::drag::PaneDrag;
use crate::host::PanelsHost;
use crate::resize::Resize;
use crate::tabs::{ClosedSideTab, SessionTabState, SessionTabs, session_pane_tab, session_tab_pane};
use crate::{Launch, PanelLauncherDesc, PanelRegistration, PanelTabDesc, SessionPane};

pub use actions::{
    adopt_child_strip, begin_keeping_keyboard, close_side_tab, dismiss_side_new_tab, end_keeping_keyboard, finish_show_pane_tab,
    hide_side_tab, keeping_keyboard, move_side_tab, open_side_new_tab, open_side_tab, open_side_tab_for_session,
    promote_preview_tab, reopen_closed_side_tab, routed_unlisted, select_side_tab, show_pane_tab, show_side_tab, sync_empty_side,
    toggle_bottom_region, toggle_side_region,
};
pub use resolve::resolve_side_region;

/// `PANE_HEIGHT_MIN` (`side-region.tsx`): each pane of a split side region keeps this much.
pub const SPLIT_MIN_H: f32 = 120.0;
/// The side pane's share of a split side region until the divider is dragged.
pub const SPLIT_DEFAULT_RATIO: f32 = 0.5;
/// `DEFAULT_DOCK_HEIGHT` (`shell/state/layout.tsx`): the bottom pane's height until dragged.
pub const BOTTOM_DEFAULT_H: f32 = 280.0;
/// `BOTTOM_PANE_HEIGHT_MIN` (`session/screen.tsx`).
pub const BOTTOM_MIN_H: f32 = 120.0;

/// One session's side region, as Desktop keeps it per shell tab (`TabRegions`): whether its
/// panes show, its strip, and the sizes the user dragged (none until they do).
#[derive(Clone, Debug, PartialEq)]
pub struct SessionSideState {
    pub visibility: Visibility,
    pub bottom_open: bool,
    pub tab_state: SessionTabState,
    pub closed: Vec<ClosedSideTab>,
    /// The chat panel's width beside the side region (`sessionWidth`).
    pub session_width: Option<f32>,
    /// The side pane's share of the side region's height while a pane splits below it.
    pub split_ratio: Option<f32>,
    /// The bottom pane's height.
    pub bottom_height: Option<f32>,
}

impl Default for SessionSideState {
    fn default() -> Self {
        Self {
            visibility: Visibility::Unseen,
            bottom_open: true,
            tab_state: SessionTabState::default(),
            closed: Vec::new(),
            session_width: None,
            split_ratio: None,
            bottom_height: None,
        }
    }
}

/// A shell tab's `contentMounted` latch (`side-region.tsx`). Another session opening a pane
/// cannot make this one lay out a hidden plugin tree. Only the open preference is persisted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visibility {
    Unseen,
    Shown,
    Shut,
}

impl Visibility {
    pub fn open(self) -> bool {
        self == Self::Shown
    }

    pub fn mounted(self) -> bool {
        self != Self::Unseen
    }

    pub fn step(&mut self, open: bool) {
        *self = match (*self, open) {
            (_, true) => Self::Shown,
            (Self::Unseen, false) => Self::Unseen,
            (_, false) => Self::Shut,
        };
    }
}

/// Explicit autofocus request for a side-panel New tab view (`new-tab-machine.ts`, opencode#66818).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NewTabFocus {
    #[default]
    Idle,
    Requested { pane: SessionPane, token: u64 },
}

#[derive(Clone, Debug, PartialEq)]
pub struct NewTabEntry {
    pub id: String,
    pub pane: SessionPane,
    pub session_id: String,
    pub over: Option<String>,
}

/// Whether the routed session's side region is open with an empty strip (`new-tab-machine.ts`
/// `openedEmpty`). Entering such a span opens one New tab; leaving it lets the next one open
/// another.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum EmptySpan {
    #[default]
    Outside,
    Within { session: String },
}

impl EmptySpan {
    pub fn within(&self, session: &str) -> bool {
        matches!(self, Self::Within { session: s } if s == session)
    }

    /// Whether step would change the span: polls write it only then.
    pub fn moves(&self, session: &str, open_empty: bool) -> bool {
        if open_empty { !self.within(session) } else { !matches!(self, Self::Outside) }
    }

    /// Steps on the routed `session`'s region as it is now; true when it enters a span.
    pub fn step(&mut self, session: &str, open_empty: bool) -> bool {
        if !open_empty {
            *self = Self::Outside;
            return false;
        }
        if self.within(session) {
            return false;
        }
        *self = Self::Within { session: session.to_owned() };
        true
    }
}

/// Context menu open over the side/bottom tab strip (`tab_menu` and `add_menu` are mutually exclusive).
#[derive(Clone, Debug, Default, PartialEq)]
pub enum SideMenu {
    #[default]
    Closed,
    /// Right-click context menu on a side tab.
    Tab { key: String, x: f32, y: f32 },
    /// `+` launcher dropdown menu anchor.
    Add { pane: SessionPane, x: f32, y: f32 },
}

impl SideMenu {
    pub fn is_open(&self) -> bool {
        !matches!(self, Self::Closed)
    }

    pub fn close(&mut self) {
        *self = Self::Closed;
    }

    /// Closes only the tab right-click menu, leaving an open `+` launcher dropdown intact
    /// (`side-region.tsx:502-571`, `deferred-menu-machine.ts:90-97`).
    pub fn close_tab_menu(&mut self) {
        if matches!(self, Self::Tab { .. }) {
            *self = Self::Closed;
        }
    }
}

/// Keyboard focus mode of the side/bottom panel region.
///
/// `Quiet` cannot exist while unfocused: Desktop tracks side focus in one `last` signal updated on
/// every `focusin`/`pointerdown`/`keydown` (`packages/app/src/runtime/plugin/closed-tabs.ts:146-153,186-193`),
/// with no muted bit surviving blur. Re-focusing through `toggle()` calls `root.focus()`
/// (`packages/app/src/runtime/plugin/panels.tsx:461-468`), setting `data-focused`
/// (`packages/app/src/runtime/plugin/side-region.tsx:108`) and lighting the selected tab
/// (`packages/ui/src/navigation/tabs/tabs.css:561-582`); `#bottom-panel` is never muted there either.
/// A shortcut pressed in the composer moves the keyboard here in `Quiet` (Open file, New tab,
/// Reopen closed tab), so the selected tab's label rests muted until the next key or press inside;
/// a terminal is the exception because its focus lands later.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FocusMode {
    #[default]
    Unfocused,
    Active,
    Quiet,
}

impl FocusMode {
    pub fn is_focused(self) -> bool {
        match self {
            Self::Unfocused => false,
            Self::Active | Self::Quiet => true,
        }
    }

    pub fn is_quiet(self) -> bool {
        match self {
            Self::Quiet => true,
            Self::Unfocused | Self::Active => false,
        }
    }
}

/// Whether the composer held keyboard focus at the last poll without a pointer press landing in
/// the side region since: the keyboard leaving the composer for here with no press is a
/// shortcut's doing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
enum ComposerHandoff {
    #[default]
    Idle,
    Armed,
}

/// Events driving [`SideFocus`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SideFocusEvent {
    /// Focus moved into the side region programmatically (tab open, select, or toggle).
    Focus,
    /// A pointer press landed inside the side region (focuses, wakes from quiet, disarms shortcut handoff).
    Press,
    /// Focus left the side region.
    Blur,
    /// A key or edit inside the side region wakes a quiet tab label to active.
    Wake,
    /// Restores a previously saved [`FocusMode`].
    Restore(FocusMode),
    /// Per-frame composer focus sync (`tick.rs`).
    SyncComposer { composing: bool, in_terminal: bool },
}

/// Explicit state machine for side/bottom region keyboard focus.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct SideFocus {
    mode: FocusMode,
    handoff: ComposerHandoff,
}

impl SideFocus {
    pub fn mode(self) -> FocusMode {
        self.mode
    }

    pub fn is_focused(self) -> bool {
        self.mode.is_focused()
    }

    pub fn is_quiet(self) -> bool {
        self.mode.is_quiet()
    }

    /// Whether the composer was focused at the last poll without an intervening press inside the region.
    pub fn is_armed(self) -> bool {
        matches!(self.handoff, ComposerHandoff::Armed)
    }

    /// Pure transition: computes the next state for `event` without mutating `self`.
    pub fn next(self, event: SideFocusEvent) -> Self {
        match event {
            SideFocusEvent::Focus => Self {
                mode: match self.mode {
                    FocusMode::Unfocused => FocusMode::Active,
                    FocusMode::Active => FocusMode::Active,
                    FocusMode::Quiet => FocusMode::Quiet,
                },
                handoff: self.handoff,
            },
            SideFocusEvent::Press => Self {
                mode: FocusMode::Active,
                handoff: ComposerHandoff::Idle,
            },
            SideFocusEvent::Blur => Self {
                mode: FocusMode::Unfocused,
                handoff: self.handoff,
            },
            SideFocusEvent::Wake => Self {
                mode: match self.mode {
                    FocusMode::Unfocused => FocusMode::Unfocused,
                    FocusMode::Active | FocusMode::Quiet => FocusMode::Active,
                },
                handoff: self.handoff,
            },
            SideFocusEvent::Restore(mode) => Self { mode, handoff: self.handoff },
            SideFocusEvent::SyncComposer { composing, in_terminal } => {
                let mode = match (self.handoff, composing, self.mode, in_terminal) {
                    (ComposerHandoff::Armed, false, FocusMode::Active, false) => FocusMode::Quiet,
                    _ => self.mode,
                };
                let handoff = if composing { ComposerHandoff::Armed } else { ComposerHandoff::Idle };
                Self { mode, handoff }
            }
        }
    }

    /// Whether `event` would change the state (lets polls avoid dirtying `Tracked<SideState>`).
    pub fn would_change(self, event: SideFocusEvent) -> bool {
        self.next(event) != self
    }

    /// Steps the state machine; returns `true` when the state changed.
    pub fn step(&mut self, event: SideFocusEvent) -> bool {
        let next = self.next(event);
        if next == *self {
            return false;
        }
        *self = next;
        true
    }

    pub fn focus(&mut self) {
        self.step(SideFocusEvent::Focus);
    }

    pub fn press(&mut self) {
        self.step(SideFocusEvent::Press);
    }

    pub fn blur(&mut self) {
        self.step(SideFocusEvent::Blur);
    }

    pub fn wake(&mut self) {
        self.step(SideFocusEvent::Wake);
    }

    pub fn restore(&mut self, mode: FocusMode) {
        self.step(SideFocusEvent::Restore(mode));
    }

    pub fn sync_composer(&mut self, composing: bool, in_terminal: bool) -> bool {
        self.step(SideFocusEvent::SyncComposer { composing, in_terminal })
    }

    pub fn wake_on_key(&mut self) -> bool {
        self.step(SideFocusEvent::Wake)
    }
}

/// Whether the left sidebar's flight is moving the side region's edge, so its contents glide
/// with the edge rather than land at once (`App::toggle_sidebar`, `panels::view::region_body`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum EdgeFlight {
    #[default]
    Idle,
    /// Gliding with the sidebar's edge until `until` (`PanelsHost::clock`).
    Flying { until: f64 },
}

/// Events driving [`EdgeFlight`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EdgeFlightEvent {
    /// The left sidebar toggled while the side region was open at `now`.
    Started { now: f64 },
    /// Per-frame poll at `now`: lands the flight once `now >= until`.
    Tick { now: f64 },
}

/// Effects returned by [`EdgeFlight::step`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EdgeFlightEffect {
    /// Wake after the flight finishes so `Flying` transitions back to `Idle`.
    WakeAfter(Duration),
    /// The flight finished (`Flying -> Idle`): redraw the side region without gliding.
    Redraw,
}

impl EdgeFlight {
    pub fn is_flying(self) -> bool {
        matches!(self, Self::Flying { .. })
    }

    /// Whether `Tick { now }` would transition `Flying -> Idle` (lets polls avoid dirtying
    /// `Tracked<SideState>` while idle or mid-flight).
    pub fn due(self, now: f64) -> bool {
        matches!(self, Self::Flying { until } if now >= until)
    }

    pub fn step(&mut self, event: EdgeFlightEvent) -> Option<EdgeFlightEffect> {
        match event {
            EdgeFlightEvent::Started { now } => {
                *self = Self::Flying { until: now + crate::view::SIDE_FLIGHT };
                Some(EdgeFlightEffect::WakeAfter(Duration::from_secs_f64(crate::view::SIDE_FLIGHT + 0.01)))
            }
            EdgeFlightEvent::Tick { now } => match *self {
                Self::Flying { until } if now >= until => {
                    *self = Self::Idle;
                    Some(EdgeFlightEffect::Redraw)
                }
                Self::Idle | Self::Flying { .. } => None,
            },
        }
    }
}

/// Guards against a held or repeated `⌘W` closing the session tab right after emptying the side
/// region (`packages/app/src/session/commands/machine.ts` `CloseTabGuardState`,
/// `packages/app/src/session/commands/use-session-commands.tsx:161-213,464-482`).
///
/// Opening a side/bottom tab or reopening the side region disarms it, matching Desktop's
/// `layout.synced` (`machine.ts:220-225`) and `file.close` `when` guard (`use-session-commands.tsx:470`),
/// so a tab opened by pointer or menu is closed normally by the next `⌘W`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CloseTabGuard {
    #[default]
    Disarmed,
    /// Armed after `⌘W` closed the side region's last tab; swallows repeated `⌘W` until another
    /// key is pressed or a foreground user action opens/focuses a panel tab or region.
    Armed,
}

/// Events driving [`CloseTabGuard`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseTabGuardEvent {
    /// Closing a tab via `⌘W` emptied or shut the side region.
    Emptied,
    /// A key event reached the panels key handler (`is_close`: matches `Bind::TabClose`).
    KeyDown { is_close: bool },
    /// A foreground action opened/selected a panel tab or reopened/focused the side region.
    RegionOpened,
}

/// Effects returned by [`CloseTabGuard::step`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseTabGuardEffect {
    /// Swallow this `⌘W` so it does not fall through and close the session tab.
    Swallow,
    /// Let the key continue normally.
    Pass,
}

impl CloseTabGuard {
    pub fn is_armed(self) -> bool {
        matches!(self, Self::Armed)
    }

    /// Pure transition: computes `(next_state, effect)` without mutating `self`.
    pub fn next(self, event: CloseTabGuardEvent) -> (Self, CloseTabGuardEffect) {
        match (self, event) {
            (_, CloseTabGuardEvent::Emptied) => (Self::Armed, CloseTabGuardEffect::Pass),
            (Self::Armed, CloseTabGuardEvent::KeyDown { is_close: true }) => (Self::Armed, CloseTabGuardEffect::Swallow),
            (Self::Armed, CloseTabGuardEvent::KeyDown { is_close: false } | CloseTabGuardEvent::RegionOpened) => {
                (Self::Disarmed, CloseTabGuardEffect::Pass)
            }
            (Self::Disarmed, CloseTabGuardEvent::KeyDown { .. } | CloseTabGuardEvent::RegionOpened) => {
                (Self::Disarmed, CloseTabGuardEffect::Pass)
            }
        }
    }

    pub fn step(&mut self, event: CloseTabGuardEvent) -> CloseTabGuardEffect {
        let (next, effect) = self.next(event);
        *self = next;
        effect
    }
}

/// Saved keyboard focus snapshot held while a background tab open runs ([`keeping_keyboard`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SavedKeyboardFocus {
    pub composer_focused: bool,
    pub side_focus: FocusMode,
    pub active_pane_focus: SessionPane,
}

/// Whether a tab is opening on someone else's behalf ([`keeping_keyboard`]), carrying the
/// saved keyboard focus snapshot only while active.
///
/// Not re-entrant: every caller (`surfaces/browser/mod.rs`, `terminal-surface/src/ops.rs`)
/// runs a synchronous non-re-entrant tab open inside [`keeping_keyboard`], and Luau `onFocus`
/// hooks only queue deferred work for the next poll.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BackgroundOpen {
    #[default]
    Idle,
    Active(SavedKeyboardFocus),
}

/// Events driving [`BackgroundOpen`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackgroundOpenEvent {
    Begin(SavedKeyboardFocus),
    End,
}

impl BackgroundOpen {
    pub fn is_active(self) -> bool {
        matches!(self, Self::Active(_))
    }

    /// Steps the background-open state machine; `End` returns the saved focus snapshot to restore.
    pub fn step(&mut self, event: BackgroundOpenEvent) -> Option<SavedKeyboardFocus> {
        match event {
            BackgroundOpenEvent::Begin(saved) => {
                debug_assert!(matches!(*self, Self::Idle), "keeping_keyboard is not re-entrant");
                *self = Self::Active(saved);
                None
            }
            BackgroundOpenEvent::End => match std::mem::replace(self, Self::Idle) {
                Self::Active(saved) => Some(saved),
                Self::Idle => None,
            },
        }
    }
}

pub(crate) type ResolvedCache = (u64, rune::Reads, Option<Rc<ResolvedRegion>>);

pub struct SideState {
    /// The chat/side divider's gesture, over the chat panel's width.
    pub chat_resize: Resize,
    /// The split divider's gesture, over the lower pane's height.
    pub split_resize: Resize,
    /// The bottom dock's top edge's gesture, over its height.
    pub bottom_resize: Resize,
    /// The open, empty side region that already opened its New tab.
    pub empty_span: EmptySpan,
    /// The window's width at the last layout, for sizes estimated outside the view.
    pub laid_w: Cell<f32>,
    /// Whether the left sidebar's flight is moving the side region's edge, so its contents glide
    /// with the edge rather than land at once.
    pub edge_flight: EdgeFlight,
    /// Whether a pane opened during this frame's poll, which flies past the poll's later systems.
    pub pane_flight: Cell<flight::PaneFlight>,
    /// A tab dragged between panes.
    pub pane_drag: PaneDrag,
    /// Side/bottom region keyboard focus and composer-shortcut quiet state machine.
    pub focus: SideFocus,
    pub active_pane_focus: SessionPane,
    /// A tab is opening for someone other than the user ([`keeping_keyboard`]), carrying the
    /// saved keyboard focus snapshot until completion.
    pub background_open: BackgroundOpen,
    /// Blocks an extra `⌘W` right after closing the side region's last tab from closing the session.
    pub close_guard: CloseTabGuard,
    /// The side region's last tab, still drawn while closing it slides the region shut.
    pub closing: closing::State,
    /// Active strip context menu (tab right-click menu or `+` launcher dropdown).
    pub menu: SideMenu,
    /// Each pane's tab strip scroll (by `SessionPane as usize`): what its handlers read, which a
    /// build never does.
    pub strips: Rc<[StripScroll; 3]>,
    /// Chrome-style mouse-close tab width holds per pane (`tab-strip-widths.ts`, `opencode#66887`).
    pub width_holds: [StripWidthHold; 3],
    /// One side panel open state shared across sessions while `sharedSidePanel` is on (#64950).
    pub shared_open: Option<bool>,
    /// Sessions that mounted the side region while `shared_open` was true (transient, for smooth close).
    pub shared_mounted: HashSet<String>,
    pub sessions: HashMap<String, SessionSideState>,
    pub adding: Vec<NewTabEntry>,
    pub chosen_new_tab: HashMap<SessionPane, String>,
    pub new_tab_focus: NewTabFocus,
    pub(crate) next_new_serial: u64,
    /// The last [`resolve_side_region`]: the plugins' surface versions and the `Tracked`
    /// state it read, and what it resolved.
    pub(crate) resolved: RefCell<Option<ResolvedCache>>,
    /// Incremented whenever [`resolve_side_region`] recomputes with a changed entry key list,
    /// so polls detect view-time tab list changes without churning on no-op cache rebuilds.
    pub(crate) resolved_gen: Cell<u64>,
    /// Stamp of stored-tab rekey/prune inputs as of the last [`crate::poll_panels`] check.
    pub(crate) tabs_stamp: Cell<Option<u64>>,
}

impl Default for SideState {
    fn default() -> Self {
        Self {
            chat_resize: Default::default(),
            split_resize: Default::default(),
            bottom_resize: Default::default(),
            empty_span: EmptySpan::Outside,
            laid_w: Cell::new(1280.0),
            edge_flight: EdgeFlight::Idle,
            pane_flight: Cell::new(flight::PaneFlight::Idle),
            pane_drag: Default::default(),
            focus: SideFocus::default(),
            active_pane_focus: SessionPane::Side,
            background_open: BackgroundOpen::Idle,
            close_guard: CloseTabGuard::Disarmed,
            closing: Default::default(),
            menu: SideMenu::Closed,
            strips: Rc::default(),
            width_holds: Default::default(),
            resolved: Default::default(),
            resolved_gen: Cell::new(0),
            tabs_stamp: Cell::new(None),
            shared_open: None,
            shared_mounted: HashSet::new(),
            sessions: HashMap::new(),
            adding: Vec::new(),
            chosen_new_tab: HashMap::new(),
            new_tab_focus: NewTabFocus::Idle,
            next_new_serial: 1,
        }
    }
}


#[derive(Clone)]
pub struct ResolvedEntry {
    pub key: String,
    pub plugin: &'static str,
    pub tab: PanelTabDesc,
    pub provider: PanelRegistration,
    pub pane: SessionPane,
}

pub struct ResolvedRegion {
    /// The session the strip is kept under (`PanelsHost::panel_session_id`).
    pub session_id: String,
    /// The routed session, which the panels list and draw for: a subagent on its root's strip.
    pub view_session: String,
    pub directory: String,
    pub project_id: String,
    pub entries: Vec<ResolvedEntry>,
    pub strip: Vec<ResolvedEntry>,
    pub active: Option<String>,
    pub active_pane: SessionPane,
    pub wide: bool,
    pub launchers: Vec<(&'static str, PanelLauncherDesc, Launch)>,
}

impl ResolvedRegion {
    pub fn pane_entries(&self, pane: SessionPane) -> Vec<&ResolvedEntry> {
        self.strip.iter().filter(|e| e.pane == pane && !e.tab.hidden).collect()
    }

    pub fn pane_active(&self, pane: SessionPane, stored: &SessionTabs) -> Option<String> {
        let members: Vec<&ResolvedEntry> = self.strip.iter().filter(|e| e.pane == pane).collect();
        if let Some(act) = self.active.as_deref()
            && members.iter().any(|e| e.key == act)
        {
            return Some(act.to_owned());
        }
        if let Some(shown) = stored.selected.get(&pane)
            && members.iter().any(|e| e.key == *shown)
        {
            return Some(shown.clone());
        }
        if pane != SessionPane::Side {
            return members.first().map(|e| e.key.clone());
        }
        let eligible: Vec<&&ResolvedEntry> = members.iter().filter(|e| e.tab.fallback).collect();
        eligible
            .iter()
            .find(|e| !e.tab.pinned && !e.tab.first)
            .or_else(|| eligible.iter().find(|e| !e.tab.pinned && e.tab.first))
            .or_else(|| eligible.iter().find(|e| e.tab.pinned))
            .map(|e| e.key.clone())
            .or_else(|| members.first().map(|e| e.key.clone()))
    }

    pub fn entry(&self, key: &str) -> Option<&ResolvedEntry> {
        self.entries.iter().find(|e| e.key == key)
    }
}

/// The panel providers: each running plugin that adds panels, sorted by `(order, plugin)`.
pub fn all_panels(host: &dyn PanelsHost) -> Vec<PanelRegistration> {
    let mut out = host.panel_providers();
    out.sort_by_key(|p| (p.order, p.plugin));
    out
}

/// The provider of namespace `plugin`.
pub fn find_panel(host: &dyn PanelsHost, plugin: &str) -> Option<PanelRegistration> {
    all_panels(host).into_iter().find(|p| p.plugin == plugin)
}

/// The side region shows: open for the routed session, in a wide window (a narrow one folds
/// it into its views and keeps it open for when the window widens, as Desktop's `sideVisible`).
pub fn side_open(host: &dyn PanelsHost) -> bool {
    side_stored_open(host) && !host.narrow()
}

/// Whether `session`'s side region is open, respecting `sharedSidePanel` (#64950).
pub fn session_side_open(host: &dyn PanelsHost, session: &str) -> bool {
    if host.shared_side_panel() {
        return host.side().shared_open.unwrap_or(false);
    }
    host.side().sessions.get(host.panel_root(session)).is_some_and(|st| st.visibility.open())
}

/// The routed session's side region is open, shown or folded away by a narrow window.
pub fn side_stored_open(host: &dyn PanelsHost) -> bool {
    host.panel_session_id().is_some_and(|id| session_side_open(host, id))
}

pub fn side_mounted(host: &dyn PanelsHost) -> bool {
    host.panel_session_id().is_some_and(|id| {
        if host.shared_side_panel() {
            let side = host.side();
            side.shared_open.unwrap_or(false) || side.shared_mounted.contains(id)
        } else {
            host.side().sessions.get(id).is_some_and(|st| st.visibility.mounted())
        }
    })
}

/// The bottom pane shows: open for the routed session, in a wide window (`screen.tsx:545`).
pub fn bottom_open(host: &dyn PanelsHost) -> bool {
    bottom_stored_open(host) && !host.narrow()
}

pub fn bottom_stored_open(host: &dyn PanelsHost) -> bool {
    host.panel_session_id()
        .and_then(|id| host.side().sessions.get(id))
        .is_some_and(|st| st.bottom_open)
}

/// Whether the side region and the bottom pane are on screen. The bottom pane can be stored
/// open with no tab in it, which draws nothing: its first tab opens it.
pub fn panes_shown(host: &dyn PanelsHost) -> (bool, bool) {
    let bottom = bottom_stored_open(host)
        && resolve_side_region(host).is_some_and(|r| !r.pane_entries(SessionPane::Bottom).is_empty());
    (side_stored_open(host), bottom)
}

/// A session's side state: a child session's is its root's (`PanelsHost::panel_root`).
pub fn session_side<'a>(host: &'a dyn PanelsHost, session_id: &str) -> Option<&'a SessionSideState> {
    host.side().sessions.get(host.panel_root(session_id))
}

pub fn session_side_mut<'a>(host: &'a mut dyn PanelsHost, session_id: &str) -> &'a mut SessionSideState {
    let session_id = host.panel_root(session_id).to_owned();
    host.side_mut().sessions.entry(session_id).or_default()
}

pub fn shown_new_tab<'a>(
    host: &'a dyn PanelsHost,
    session_id: &str,
    pane: SessionPane,
    pane_active: Option<&str>,
) -> Option<&'a NewTabEntry> {
    let side = host.side();
    let live: Vec<&NewTabEntry> = side
        .adding
        .iter()
        .filter(|n| n.session_id == session_id && n.pane == pane && n.over.as_deref() == pane_active)
        .collect();
    if let Some(chosen) = side.chosen_new_tab.get(&pane)
        && let Some(found) = live.iter().copied().find(|n| n.id == *chosen)
    {
        return Some(found);
    }
    live.last().copied()
}

pub fn live_new_tabs<'a>(
    host: &'a dyn PanelsHost,
    session_id: &str,
    pane: SessionPane,
    pane_active: Option<&str>,
) -> Vec<&'a NewTabEntry> {
    host.side()
        .adding
        .iter()
        .filter(|n| n.session_id == session_id && n.pane == pane && n.over.as_deref() == pane_active)
        .collect()
}

/// Where tab `key` is in `session`: 0 not in the strip, 1 listed, 2 its pane's shown tab
/// with the pane hidden, 3 on screen (Desktop's `layout.state`).
pub fn tab_state(host: &dyn PanelsHost, session: &str, key: &str) -> u64 {
    let Some(st) = host.side().sessions.get(host.panel_root(session)) else { return 0 };
    let tabs = &st.tab_state.tabs;
    if !tabs.all.iter().any(|k| k == key) {
        return 0;
    }
    let pane = session_tab_pane(tabs, key);
    if session_pane_tab(tabs, pane) != Some(key) {
        return 1;
    }
    let shown = match pane {
        SessionPane::Bottom => st.bottom_open,
        SessionPane::Side | SessionPane::Split => session_side_open(host, session),
    };
    if shown { 3 } else { 2 }
}

/// The tab pane `pane` of the routed session shows, if any.
pub fn pane_shown_tab(host: &dyn PanelsHost, pane: SessionPane) -> Option<String> {
    let reg = resolve_side_region(host)?;
    let empty = SessionTabs::default();
    let stored = host.side().sessions.get(&reg.session_id).map_or(&empty, |s| &s.tab_state.tabs);
    reg.pane_active(pane, stored)
}

/// Whether `key` is its pane's shown tab and that pane is on screen (Desktop's `"visible"`).
pub fn side_tab_visible(host: &dyn PanelsHost, key: &str) -> bool {
    let Some(sid) = host.panel_session_id() else { return false };
    let Some(st) = host.side().sessions.get(sid) else { return false };
    let pane = session_tab_pane(&st.tab_state.tabs, key);
    let open = if pane == SessionPane::Bottom { st.bottom_open } else { session_side_open(host, sid) };
    open && pane_shown_tab(host, pane).as_deref() == Some(key)
}
