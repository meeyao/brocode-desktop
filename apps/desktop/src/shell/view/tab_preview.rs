//! Desktop's tab hover preview (`tab-popover.tsx`, `tab-popover-machine.ts`, `tab-popover.css`):
//! resting on a session tab for 750 ms shows a card with its project, its whole title, its
//! folder, and (when multiple servers are listed and the tab isn't on the built-in server) its
//! server name; once one has shown, the next tab's card shows at once while the pointer hops
//! across tabs within 500 ms. It comes and goes without animating, snapped under a top tab
//! (`bottom-start`, 6 away) or beside a side tab (`right-start` / `left-start` in RTL, 6 away,
//! `#63074`). A drag, a press, a rename or the tab's menu keeps it away.
//!
//! Two machines, as in Desktop: [`Card`] is Kobalte's `HoverCard` on the hovered tab with the
//! tab's `popoverOpen` (`tab-nav.tsx:182-185, 535-550`), and [`State`] is the preview shared by
//! every strip (`TabPreviewState`), which hears the card open and close.

use rune::machine::Attempt;

use super::*;

/// The hover before a card first shows (`TAB_PREVIEW_OPEN_DELAY_MS`).
const OPEN_DELAY: f64 = 0.75;
/// After a card closes, the next tab's opens at once within this (`TAB_PREVIEW_SKIP_WINDOW_MS`).
const SKIP_WINDOW: f64 = 0.5;
/// The card (`width: 256px; padding: 12px; gap: 4px; border-radius: 6px`).
const W: f32 = 256.0;
// The card's `font-variant-numeric: tabular-nums` reaches every line.
const PROJECT: TextStyle = TextStyle::new(13.0, 440.0).lh(19.5).tracking(-0.04).features("\"tnum\"");
const TITLE: TextStyle = TextStyle::new(13.0, 530.0).lh(19.5).tracking(-0.04).features("\"tnum\"");
/// The server's line (`[data-slot="server"]`: 13 at 440 on `--line-height-compact`, 16).
const SERVER: TextStyle = TextStyle::new(13.0, 440.0).lh(16.0).tracking(-0.04).features("\"tnum\"");

/// `TabPreviewState`: `cold` waits the full open delay; `open` and `warm` (just closed) open the
/// next preview at once. `instant` marks a preview opened during such a streak (Desktop's card
/// has no entrance either way, `tab-popover.css:28-45`).
#[derive(Clone, Debug, PartialEq)]
pub enum State {
    Cold { attempt: Attempt },
    Open { attempt: Attempt, tab: String, anchor: String, instant: bool },
    /// Closed a moment ago, until `until` on the app's clock (`warm.start`).
    Warm { attempt: Attempt, until: f64, instant: bool },
}

impl Default for State {
    fn default() -> State {
        State::Cold { attempt: Attempt(0) }
    }
}

/// `TabPreviewEvent`. `Closed` is also `owner.disposed`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event<'a> {
    Opened { tab: &'a str, anchor: &'a str },
    Closed(&'a str),
    WarmExpired { attempt: Attempt },
}

/// `transitionTabPreview`. The warm timer is the `until` the state carries, so `warm.start`
/// and `warm.clear` have nothing left to do.
pub fn step(state: &mut State, event: Event<'_>, now: f64) {
    match event {
        Event::Opened { tab, anchor } => {
            if matches!(state, State::Open { tab: t, anchor: a, .. } if t == tab && a == anchor) {
                return;
            }
            let attempt = attempt(state);
            let instant = !matches!(state, State::Cold { .. });
            // Another tab's preview takes over at once and closes the one that was up.
            *state = State::Open { attempt, tab: tab.to_owned(), anchor: anchor.to_owned(), instant };
        }
        Event::Closed(tab) => {
            let State::Open { attempt, tab: t, instant, .. } = state else { return };
            if t != tab {
                return;
            }
            *state = State::Warm { attempt: Attempt(attempt.0 + 1), until: now + SKIP_WINDOW, instant: *instant };
        }
        Event::WarmExpired { attempt: due } => {
            if let State::Warm { attempt, .. } = state
                && *attempt == due
            {
                *state = State::Cold { attempt: due };
            }
        }
    }
}

fn attempt(state: &State) -> Attempt {
    match state {
        State::Cold { attempt } | State::Open { attempt, .. } | State::Warm { attempt, .. } => *attempt,
    }
}

/// `selectTabPreviewOpenDelay`: Kobalte reads it as the pointer enters a tab.
fn open_delay(state: &State) -> f64 {
    if matches!(state, State::Cold { .. }) { OPEN_DELAY } else { 0.0 }
}

/// Kobalte's `HoverCard` on the hovered tab, with that tab's `popoverOpen`.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Card {
    /// The pointer is on no tab.
    #[default]
    Away,
    /// The pointer rests on `tab` and its card is closed, refused or shut by its menu: it asks
    /// again only when the pointer comes back (Kobalte opens on `pointerenter`), however often
    /// the tab says the pointer is on it.
    Resting { tab: String, anchor: String },
    /// The pointer entered `tab`: its card asks to open at `at` (`openWithDelay`), unless the
    /// pointer leaves first (`cancelOpening`).
    Waiting { tab: String, anchor: String, at: f64 },
    /// `tab` took the card (`popoverOpen`): it shows while nothing blocks it.
    Asked { tab: String, anchor: String },
}

/// The card, and the preview it feeds.
#[derive(Default)]
pub struct Preview {
    card: Card,
    state: State,
}

impl App {
    /// The pointer came onto (`inside`) or left a `TabStrip` tab `id` (anchored at `t-{id}`).
    pub(crate) fn preview_hover(&mut self, id: &str, inside: bool, tx: &mut Tx) {
        let anchor = format!("t-{id}");
        self.preview_hover_at(id, &anchor, inside, tx);
    }

    /// The pointer came onto (`inside`) or left tab `id`, whose row element is keyed `anchor`.
    pub(crate) fn preview_hover_at(&mut self, id: &str, anchor: &str, inside: bool, tx: &mut Tx) {
        let now = tx.clock();
        self.expire_preview(now);
        let shown = self.previewed().map(|(t, a)| (t.to_owned(), a.to_owned()));
        // A draft has no session, so it never takes the card (`!props.session`).
        let draft = self.tab(id).is_some_and(crate::tabs::Tab::is_draft);
        let p = &mut *self.gestures.tab_preview;
        if !inside {
            // Leaving the tab cancels its opening, and the pointer off the tab and its card
            // closes it (`onHoverOutside`, `closeDelay` 0).
            if matches!(&p.card, Card::Waiting { tab, .. } | Card::Asked { tab, .. } | Card::Resting { tab, .. } if tab == id) {
                p.card = Card::Away;
            }
        } else if !matches!(&p.card, Card::Waiting { tab, anchor: a, .. } | Card::Asked { tab, anchor: a } | Card::Resting { tab, anchor: a } if tab == id && a == anchor) {
            p.card = if draft {
                Card::Resting { tab: id.to_owned(), anchor: anchor.to_owned() }
            } else {
                Card::Waiting { tab: id.to_owned(), anchor: anchor.to_owned(), at: now + open_delay(&p.state) }
            };
        }
        self.settle_preview(now, tx);
        // The strip draws its hover by hover groups, so only the card opening or closing changes
        // what the view draws; a hover that doesn't runs no view.
        let now_shown = self.previewed().map(|(t, a)| (t.to_owned(), a.to_owned()));
        if now_shown != shown {
            tx.instant();
        } else {
            tx.live_only();
        }
    }

    /// Dismisses any pending or open tab hover card (e.g. on window blur).
    pub(crate) fn clear_preview(&mut self, tx: &mut Tx) {
        if matches!(self.gestures.tab_preview.card, Card::Away) && !matches!(self.gestures.tab_preview.state, State::Open { .. }) {
            return;
        }
        let now = tx.clock();
        let shown = self.previewed().is_some();
        self.gestures.tab_preview.card = Card::Away;
        self.settle_preview(now, tx);
        if shown {
            tx.instant();
        }
    }

    /// What keeps tab `id`'s card away (`previewBlocked`): a drag or press in the strip or tab
    /// groups, its rename, its menu.
    fn preview_blocked(&self, id: &str) -> bool {
        self.gestures.tab_drag.phase() != rune_kit::shell::TabDragPhase::Idle
            || !matches!(crate::tabs::groups::state(self).drag, crate::tabs::groups::drag::State::Idle)
            || crate::tabs::sections::state(self).drag.is_gesture()
            || crate::tabs::sections::state(self).naming.editing().is_some()
            || !crate::tabs::sections::state(self).selection.is_empty()
            || self.kernel.slots.get(&crate::session_menu::MENUS).renaming().is_some_and(|t| self.names(&t, id))
            || self.kernel.slots.get(&crate::session_menu::MENUS).on.as_ref().is_some_and(|t| self.names(t, id))
    }

    /// Whether a menu or rename target is tab `id` (`Tab::uid`).
    fn names(&self, target: &crate::session_menu::Target, id: &str) -> bool {
        target.tab(self).is_some_and(|t| t.uid == id)
    }

    /// Whether tab `id`'s card shows once it has it (`open() && !previewBlocked()`): a narrow
    /// window's tabs show none (`tab-popover.tsx:61`).
    fn preview_open(&self, id: &str) -> bool {
        !self.preview_blocked(id) && !self.narrow()
    }

    /// The tab uid whose hover card is currently open (kept built in virtualized strips).
    pub(crate) fn previewed_tab(&self) -> Option<&str> {
        self.previewed().map(|(tab, _)| tab)
    }

    /// The tab and its row anchor key whose card shows now.
    fn previewed(&self) -> Option<(&str, &str)> {
        let State::Open { tab, anchor, .. } = &self.gestures.tab_preview.state else { return None };
        (self.preview_open(tab) && self.tab_layout() != crate::layout_state::TabLayout::Inbox)
            .then_some((tab.as_str(), anchor.as_str()))
    }

    fn expire_preview(&mut self, now: f64) {
        if let State::Warm { attempt, until, .. } = self.gestures.tab_preview.state
            && now + 1e-9 >= until
        {
            step(&mut self.gestures.tab_preview.state, Event::WarmExpired { attempt }, now);
        }
    }

    /// The card's opening comes due, a menu takes the tab, and the preview hears whether the
    /// card is open (`createComputed` over `open() && !previewBlocked()`).
    fn settle_preview(&mut self, now: f64, tx: &mut Tx) {
        if let Card::Waiting { tab, anchor, at } = &self.gestures.tab_preview.card {
            if now + 1e-3 < *at {
                tx.wake_at(*at);
            } else {
                // The card asks to open; a blocked tab refuses, and the card doesn't ask again
                // until the pointer comes back (`if (value && previewBlocked()) return`).
                let (tab, anchor) = (tab.clone(), anchor.clone());
                self.gestures.tab_preview.card = if self.preview_blocked(&tab) {
                    Card::Resting { tab, anchor }
                } else {
                    Card::Asked { tab, anchor }
                };
            }
        }
        // Its menu opening closes the tab's card (`if (open) setPopoverOpen(false)`).
        if let Card::Asked { tab, anchor } = &self.gestures.tab_preview.card
            && self.kernel.slots.get(&crate::session_menu::MENUS).on.as_ref().is_some_and(|t| self.names(t, tab))
        {
            self.gestures.tab_preview.card = Card::Resting { tab: tab.clone(), anchor: anchor.clone() };
        }
        let open = match &self.gestures.tab_preview.card {
            Card::Asked { tab, anchor } if self.preview_open(tab) => Some((tab.clone(), anchor.clone())),
            _ => None,
        };
        // Every poll comes here: the preview is written only when the card opens or closes.
        match (&self.gestures.tab_preview.state, open) {
            (State::Open { tab, .. }, None) => {
                let tab = tab.clone();
                step(&mut self.gestures.tab_preview.state, Event::Closed(&tab), now);
            }
            (State::Open { tab: shown, anchor: shown_a, .. }, Some((tab, anchor))) if *shown == tab && *shown_a == anchor => {}
            (_, Some((tab, anchor))) => step(&mut self.gestures.tab_preview.state, Event::Opened { tab: &tab, anchor: &anchor }, now),
            _ => {}
        }
    }
}

/// `poll`: the warm streak ends, a card's opening comes due, and blocks come and go.
pub(crate) fn poll(s: &mut App, now: f64, tx: &mut Tx) -> bool {
    s.expire_preview(now);
    let shown = s.previewed().map(|(t, a)| (t.to_owned(), a.to_owned()));
    s.settle_preview(now, tx);
    let changed = s.previewed().map(|(t, a)| (t.to_owned(), a.to_owned())) != shown;
    if changed {
        tx.instant();
    }
    changed
}

/// The card, at the window's root (`tab-popover.tsx`, `tab-popover.css`).
pub(crate) fn view(s: &App, p: &Pal) -> Option<El> {
    let (id, anchor) = s.previewed()?;
    let tab = s.tab(id)?;
    // Its session, on its own server (`props.session`): none, no card.
    let face = super::tab_strip::face(s, tab);
    let title = face.session_title?;
    let dir = if face.dir.is_empty() { tab.directory.as_str() } else { face.dir };
    let vertical = s.tab_layout().has_sidebar();
    let side = if vertical { Side::Right } else { Side::Bottom };
    let project = project_or_dir(face.project, dir);
    // Paths break after their separators, not inside a folder's name (`<wbr>`).
    let detail = breakable(&s.tilde(&tab.server, dir));
    // The server, only while several are listed in the registry and it isn't the app's own (`servers.tag`, `tab-nav.tsx:202-207`).
    let server = crate::servers::registry::tag_of(s, &tab.server);
    let max_w = rune::memo::layout_with("zoom", |r| (r[2] - 24.0).max(0.0)).unwrap_or(W);
    // Beside a side tab, its top is the row's as drawn (`right-start` of the 28 tall trigger),
    // not the top of the slot that reaches half the gap above it.
    let anchor = if vertical && anchor == format!("t-{id}") { format!("{anchor}-pill") } else { anchor.to_owned() };
    let card = col()
        .key("tab-preview")
        .a11y_hidden()
        .anchored(anchor, side, Align::Start, 6.0)
        .anchored_round()
        .w(W)
        .max_w(max_w)
        .padding(PAD)
        .gap(GAP)
        .radius(6.0)
        .fill(p.bg_base)
        .elevate(p.floating)
        .transition(rune::Transition::none())
        .motion(Spring::NONE)
        .child(
            col().key("header").w_full().gap(GAP)
                .children((!project.is_empty()).then(|| text(project).key("project-name").font(PROJECT).color(p.muted)))
                .children((!title.is_empty()).then(|| text(title.to_owned()).key("title").font(TITLE).color(p.text).wrap())),
        )
        .children((!detail.is_empty()).then(|| {
            row().key("row").w_full().min_w(0.0).child(text(detail).key("detail").font(PROJECT).color(p.muted).wrap().grow(1.0).shrink().min_w(0.0))
        }))
        .children(server.map(|name| text(name).key("server").font(SERVER).color(p.muted).w_full().truncate()));
    Some(card)
}

/// The card's padding and the gap between its lines.
const PAD: f32 = 12.0;
const GAP: f32 = 4.0;

/// `path` with a break allowed after each separator.
fn breakable(path: &str) -> String {
    path.chars().flat_map(|c| if c == '/' || c == '\\' { vec![c, '\u{200B}'] } else { vec![c] }).collect()
}
