//! Experimental › Sidebar sections (desktop host): side tabs a person picks several of with
//! ⌘-click and ⇧-click, acts on together from a right-click, and gathers under quiet named
//! headers (⌘G). Off, nothing here runs: no state is read or saved and no tab moves.
//!
//! The window keeps its sections in `sidebar.sections` beside its tabs, and its tab order is the
//! list's (`model`): a closed tab leaves its section, and a section left empty goes.

pub mod drag;
pub mod model;
pub mod naming;
mod ops;
pub mod select;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use opencode_kernel::{EscapeLayer, Feature, KernelBuilder, OverlayRegistration, Slot};
use rune::{KeyEvent, Live as RuneLive, ScrollPos, TextEdit, Tx};

pub use model::{Doc, Drop, Entry, Laid, Section};
pub use ops::*;

use crate::App;
use crate::tabs::Tab;

/// A window's sections, its selection, the name being edited and the drag.
#[derive(Default)]
pub struct SectionsState {
    pub doc: Doc,
    pub selection: select::Selection,
    pub naming: naming::Naming,
    /// The field a late edit lands in once naming ended.
    pub scratch: TextEdit,
    pub drag: drag::DragState,
    /// The tabs open when last looked (`Tab::uid`): one gone leaves its section.
    seen: Vec<String>,
}

pub static SECTIONS: Slot<SectionsState> = Slot::new();

/// What the rows and scroll viewport report without redrawing: laid-out rects, scroll viewport
/// rect + offset for drag auto-scroll, and live modifiers (`home_modifiers`).
pub struct Live {
    pub rects: RefCell<HashMap<String, [f32; 4]>>,
    pub scroll_rect: RuneLive<[f32; 4]>,
    pub scroll_pos: Rc<Cell<ScrollPos>>,
    pub cmd: Cell<bool>,
    pub shift: Cell<bool>,
}

impl Default for Live {
    fn default() -> Self {
        Self {
            rects: RefCell::new(HashMap::new()),
            scroll_rect: RuneLive::new([0.0; 4]),
            scroll_pos: Rc::new(Cell::new(ScrollPos::default())),
            cmd: Cell::new(false),
            shift: Cell::new(false),
        }
    }
}

pub static LIVE: Slot<Live> = Slot::new();

const ROW: &str = "sidebar.sections";
pub const OVERLAY_ID: &str = "sidebar.sections";
/// The name field's key.
pub const FIELD: &str = "sidebar-section-name";
/// The scroll viewport's key.
pub const SCROLL_KEY: &str = "sidebar-sections-scroll";

pub struct SidebarSectionsFeature;

impl Feature<App> for SidebarSectionsFeature {
    fn id(&self) -> &'static str {
        "feature.sidebar-sections"
    }

    fn register(&self, builder: &mut KernelBuilder<App>) {
        builder.register_slot(&SECTIONS);
        builder.register_slot(&LIVE);
        builder.schedule(crate::tick::stage::REACT_SHELL, "sidebar-sections.poll", |s, _, tx| poll(s, tx));
        // ⌘G gathers the selection before Find reads it as Find next (Find closed); no layer
        // draws here, so it takes the last band (`with_key_in`).
        builder.register_overlay(OverlayRegistration::<App>::new(OVERLAY_ID).with_key_in(crate::overlays::key_band::LOW, key));
        builder.register_escape_layer(EscapeLayer::<App>::new(
            "escape.sidebar_sections_drag",
            crate::overlays::escape_band::CHROME,
            |s| state(s).drag.is_gesture(),
            |s, _, tx| {
                if drag::step(&mut state_mut(s).drag, drag::Event::Cancel) == drag::Effect::Redraw {
                    tx.animated();
                }
            },
        ));
        builder.register_escape_layer(EscapeLayer::<App>::new(
            "escape.sidebar_sections_naming",
            crate::overlays::escape_band::CHROME,
            |s| state(s).naming.editing().is_some(),
            |s, _, tx| {
                naming_step(s, naming::Event::Cancel, tx);
                tx.blur();
            },
        ));
        builder.register_escape_layer(EscapeLayer::<App>::new(
            "escape.sidebar_sections",
            crate::overlays::escape_band::FALLBACK,
            |s| !state(s).selection.is_empty(),
            |s, _, _| clear_selection(s),
        ));
    }
}

pub fn state(s: &App) -> &SectionsState {
    s.kernel.slots.get(&SECTIONS)
}

pub(crate) fn state_mut(s: &mut App) -> &mut SectionsState {
    s.kernel.slots.get_mut(&SECTIONS)
}

/// Whether the sidebar lists its tabs in sections now: the setting is on and side tabs show the
/// plain list (Organize › List, Projects, or Consolidated, with no tab groups of either kind).
pub fn on(s: &App) -> bool {
    use crate::settings::Organize;
    if !s.settings.sidebar_sections() || s.tab_layout() != crate::layout_state::TabLayout::Vertical || s.narrow() {
        return false;
    }
    match s.settings.sidebar_organize.effective() {
        Organize::List => !crate::tabs::groups::on(s),
        Organize::Projects => true,
        Organize::Consolidated => s.settings.navigation() != crate::settings::Navigation::List && crate::tabs::auto_groups::groups(s).is_none(),
        _ => false,
    }
}

/// The tabs the list shows (a pinned one is in the pinned area): index and uid.
pub fn shown(s: &App) -> Vec<(usize, String)> {
    s.tabs.iter().enumerate().filter(|(_, t)| !crate::tabs::pins::holds(s, t)).map(|(i, t)| (i, t.uid())).collect()
}

pub fn laid(s: &App) -> Laid {
    model::lay(&state(s).doc, &shown(s))
}

/// The tabs as listed, a folded section's hidden (⌘1–9 and ⇧-click run through these).
pub fn visible_uids(s: &App) -> Option<Vec<String>> {
    if !on(s) {
        return None;
    }
    Some(laid(s).visible().filter_map(|i| s.tabs.get(i).map(Tab::uid)).collect())
}

/// Next visible tab UID when closing `uid` while sidebar sections are active, keeping collapsed
/// sections closed instead of stepping into a hidden member tab.
pub fn next_visible_after_closing(s: &App, uid: &str) -> Option<String> {
    let visible = visible_uids(s)?;
    let at = visible.iter().position(|u| u == uid)?;
    visible.get(at + 1).or_else(|| at.checked_sub(1).and_then(|p| visible.get(p))).cloned()
}

/// Every listed tab, folded ones too: the order a group of tabs keeps.
pub(crate) fn listed_uids(s: &App) -> Vec<String> {
    laid(s).all().filter_map(|i| s.tabs.get(i).map(Tab::uid)).collect()
}

/// Reads the window's row (from the boot packet at launch). A row saved before the list was one
/// order showed every section above the other tabs: the tabs take that order here, in memory, and
/// keep it once anything in the list changes (`persists()` reads false this early).
pub fn restore(s: &mut App) {
    let ns = opencode_storage::window_namespace(&s.window.id);
    let saved = crate::storage::json(&ns, ROW);
    let mut doc = saved.as_ref().map(Doc::from_json).unwrap_or_default();
    let uids: Vec<String> = s.tabs.iter().map(Tab::uid).collect();
    if !uids.is_empty() {
        doc.retain_open(&uids);
    }
    let legacy = saved.as_ref().is_some_and(|v| v["v"].as_u64() != Some(2));
    if legacy && !doc.sections.is_empty() && s.settings.sidebar_sections() {
        let order = model::legacy_order(&doc, &uids);
        arrange_tabs(s, &order, false);
    }
    let seen = s.tabs.iter().map(Tab::uid).collect();
    let st = state_mut(s);
    st.doc = doc;
    st.seen = seen;
}

/// The window's tabs in `order` (uids); saved when `save` and anything moved.
pub(crate) fn arrange_tabs(s: &mut App, order: &[String], save: bool) {
    let rank: HashMap<&str, usize> = order.iter().enumerate().map(|(i, u)| (u.as_str(), i)).collect();
    let mut sorted: Vec<Tab> = s.tabs.to_vec();
    sorted.sort_by_key(|t| rank.get(t.uid().as_str()).copied().unwrap_or(usize::MAX));
    if sorted != *s.tabs {
        *s.tabs = sorted;
        if save {
            s.save_tabs();
        }
    }
}

fn save(s: &App) {
    if s.persists() {
        crate::windows::store::save(&s.window, ROW, &state(s).doc.to_json());
    }
}

pub(crate) fn update(s: &mut App, f: impl FnOnce(&mut Doc)) {
    f(&mut state_mut(s).doc);
    save(s);
}

/// The live modifiers, from the window's modifier changes: written where no view reads them.
pub fn modifiers(s: &App, mods: rune::Mods) {
    let live = s.kernel.slots.untracked(&LIVE);
    live.cmd.set(mods.cmd);
    live.shift.set(mods.shift);
}

/// Resets held modifier flags and cancels any in-flight drag when the window loses focus.
pub fn window_left(s: &mut App, tx: &mut Tx) {
    let live = s.kernel.slots.untracked(&LIVE);
    live.cmd.set(false);
    live.shift.set(false);
    if s.kernel.slots.untracked(&SECTIONS).drag.is_gesture() {
        state_mut(s).drag = drag::DragState::Idle;
        tx.animated();
    }
}

fn poll(s: &mut App, tx: &mut Tx) -> bool {
    let active = on(s);
    let mut changed = false;
    if !active {
        let (has_sel, has_naming, has_drag) = {
            let u = s.kernel.slots.untracked(&SECTIONS);
            (!u.selection.is_empty(), u.naming.editing().is_some(), !u.drag.is_idle())
        };
        if has_sel {
            clear_selection(s);
            changed = true;
        }
        if has_naming {
            naming_step(s, naming::Event::Cancel, tx);
            changed = true;
        }
        if has_drag {
            state_mut(s).drag = drag::DragState::Idle;
            changed = true;
        }
        if !s.settings.sidebar_sections() {
            return changed;
        }
    }
    if matches!(s.kernel.slots.untracked(&SECTIONS).drag, drag::DragState::Dropped { .. }) {
        drag::step(&mut state_mut(s).drag, drag::Event::Press);
        changed = true;
    }
    let uids: Vec<String> = s.tabs.iter().map(Tab::uid).collect();
    if uids != s.kernel.slots.untracked(&SECTIONS).seen {
        let gone: Vec<String> = s.kernel.slots.untracked(&SECTIONS).seen.iter().filter(|u| !uids.contains(u)).cloned().collect();
        if !gone.is_empty() && state(s).doc.sections.iter().any(|sec| sec.members.iter().any(|m| gone.contains(m))) {
            update(s, |d| d.pull(&gone));
        }
        let st = state_mut(s);
        st.selection.step(select::Event::Retain(&uids));
        st.seen = uids;
        changed = true;
    }
    if active && let Some(focused) = s.kernel.slots.untracked(&SECTIONS).naming.focused() {
        let arming = matches!(s.kernel.slots.untracked(&SECTIONS).naming, naming::Naming::Arming { .. });
        if arming || !focused {
            naming_step(s, naming::Event::Focused(focused), tx);
            changed = true;
        }
    }
    changed
}

/// ⌘G (Ctrl+G off macOS) with tabs selected: a new section of them. Find's own ⌘G (Find next)
/// keeps the key while its bar is open.
fn key(s: &mut App, ev: &KeyEvent, tx: &mut Tx) -> bool {
    if !ev.is_command('g') || ev.mods.shift || ev.mods.alt || state(s).selection.is_empty() || !on(s) || s.settings_open() {
        return false;
    }
    if s.kernel.slots.get(&crate::find::FIND).is_open() {
        return false;
    }
    let uids = state(s).selection.uids().to_vec();
    create(s, &uids, tx);
    true
}
