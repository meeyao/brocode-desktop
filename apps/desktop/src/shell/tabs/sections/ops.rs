//! What a person does to sections and the selected tabs: gather them, move them (tabs anywhere in
//! the list, sections anywhere among its tabs), fold, rename, remove a section (Undo puts it
//! back), and close, archive or delete several tabs at once.

use std::time::Duration;

use opencode_i18n::t;
use opencode_kit::servers::ServerKey;
use rune::{Drag, Phase, PointerButton, Tx};

use super::model::{self, Doc, Drop, Item, Section};
use super::{FIELD, LIVE, SCROLL_KEY, arrange_tabs, drag, listed_uids, naming, select, shown, state, state_mut, update, visible_uids};
use crate::App;
use crate::tabs::Tab;

pub fn clear_selection(s: &mut App) {
    if !state(s).selection.is_empty() {
        state_mut(s).selection.step(select::Event::Clear);
    }
}

/// `uids` in the order the sidebar lists them.
fn in_list_order(s: &App, uids: &[String]) -> Vec<String> {
    listed_uids(s).into_iter().filter(|u| uids.contains(u)).collect()
}

pub(crate) fn shown_items(s: &App) -> Vec<Item> {
    let order: Vec<String> = shown(s).into_iter().map(|(_, u)| u).collect();
    model::items(&state(s).doc, &order)
}

pub fn naming_step(s: &mut App, event: naming::Event<'_>, tx: &mut Tx) {
    match state_mut(s).naming.step(event) {
        naming::Effect::None => {}
        naming::Effect::Focus => tx.focus(FIELD, false),
        naming::Effect::Save { section, name } => {
            tx.animated();
            update(s, |d| {
                let name = d.unique_name(name.trim(), &section);
                if let Some(sec) = d.get_mut(&section) {
                    sec.name = name;
                }
            });
        }
    }
}

/// The name the titles of `uids` plainly share, if any.
fn shared_name(s: &App, uids: &[String]) -> Option<String> {
    let titles: Vec<String> = uids.iter().filter_map(|u| s.tab(u)).map(|t| crate::view::tab_strip::face(s, t).title.clone()).collect();
    naming::suggest(&titles.iter().map(String::as_str).collect::<Vec<_>>())
}

/// A new section of `uids` in place where the first of them stands, its name field open on the
/// name their titles share, or empty (an empty ⏎ keeps "New section").
pub fn create(s: &mut App, uids: &[String], tx: &mut Tx) {
    let uids = in_list_order(s, uids);
    if uids.is_empty() {
        return;
    }
    let guess = shared_name(s, &uids);
    let id = crate::windows::store::new_id();
    let name = state(s).doc.unique_name(guess.as_deref().unwrap_or(t!("rune.sections.new")), &id);
    let section = Section { id: id.clone(), name: name.clone(), collapsed: false, members: Vec::new() };
    rearrange(s, |items| model::gather(items, &uids, section));
    clear_selection(s);
    let text = if guess.is_some() { name.as_str() } else { "" };
    naming_step(s, naming::Event::Start { section: &id, text, before: &name, created: true }, tx);
}

/// Rename section `id` in place.
pub fn start_rename(s: &mut App, id: &str, tx: &mut Tx) {
    let Some(sec) = state(s).doc.get(id).cloned() else { return };
    naming_step(s, naming::Event::Start { section: id, text: &sec.name, before: &sec.name, created: false }, tx);
}

/// `uids` to the end of section `id`.
pub fn move_to(s: &mut App, uids: &[String], id: &str) {
    let uids = in_list_order(s, uids);
    rearrange(s, |items| model::land(items, &uids, &Drop::Into { id: id.to_owned(), prepend: false }));
    clear_selection(s);
}

/// `uids` out of their sections, each just below the one it left.
pub fn ungroup(s: &mut App, uids: &[String]) {
    rearrange(s, |items| model::ungroup(items, uids));
    clear_selection(s);
}

/// The list's top level changed by `f`, then saved: the sections, and the tabs in its order
/// (keeping any pinned tabs ahead of `shown` in their relative order).
fn rearrange(s: &mut App, f: impl FnOnce(&mut Vec<Item>)) {
    let mut items = shown_items(s);
    f(&mut items);
    let (doc, shown_order) = model::split(items);
    let mut full: Vec<String> = s.tabs.iter().filter(|t| crate::tabs::pins::holds(s, t)).map(Tab::uid).collect();
    full.extend(shown_order);
    update(s, |d| *d = doc);
    arrange_tabs(s, &full, true);
}

pub fn toggle(s: &mut App, id: &str, tx: &mut Tx) {
    tx.animated();
    update(s, |d| {
        if let Some(sec) = d.get_mut(id) {
            sec.collapsed = !sec.collapsed;
        }
    });
    if let Some(shown) = visible_uids(s) {
        state_mut(s).selection.step(select::Event::Retain(&shown));
    }
}

/// Section `id` goes, its tabs left where they are, none closed: only its header fades. The
/// toast's Undo puts it back.
pub fn remove_section(s: &mut App, id: &str, tx: &mut Tx) {
    let before = state(s).doc.clone();
    if before.get(id).is_none() {
        return;
    }
    tx.animated();
    naming_step(s, naming::Event::Gone(id), tx);
    rearrange(s, |items| model::dissolve(items, id));
    crate::undo::offer(s, t!("rune.sections.removed"), crate::undo::Undoable::Sections(Box::new(before)));
}

/// A press on section `id`'s header (its click): it folds, unless it ended a drag.
pub fn header_press(s: &mut App, id: &str, tx: &mut Tx) {
    if drag::step(&mut state_mut(s).drag, drag::Event::Press) == drag::Effect::Click {
        toggle(s, id, tx);
    }
}

/// A drag on section `id`'s header: past 4 points it carries the section to another place.
pub fn header_drag(s: &mut App, id: &str, d: &mut Drag, tx: &mut Tx) {
    if tx.button() != PointerButton::Primary {
        return;
    }
    let event = match d.phase {
        Phase::Down if in_end_slot(d) => drag::Event::Ignore,
        Phase::Down => drag::Event::DownSection {
            id: id.to_owned(),
            uids: state(s).doc.get(id).map(|sec| sec.members.clone()).unwrap_or_default(),
            items: shown_items(s),
            start_scroll: scroll_offset(s),
            touch: session_view::style::touch_controls(),
        },
        Phase::Move => {
            if matches!(state(s).drag, drag::DragState::DraggingSection { .. }) {
                auto_scroll(s, d, tx);
            }
            drag::Event::MoveSection { dy: d.dy, scroll_y: scroll_offset(s) }
        }
        Phase::Up if d.cancelled() => drag::Event::Cancel,
        Phase::Up => drag::Event::Up { on_self: on_element(s, d) },
    };
    let was_active = state(s).drag.is_active();
    match drag::step(&mut state_mut(s).drag, event) {
        drag::Effect::None | drag::Effect::Click | drag::Effect::Commit { .. } => {}
        drag::Effect::Redraw => {
            if !was_active && state(s).drag.is_active() {
                auto_scroll(s, d, tx);
            }
            tx.animated();
        }
        drag::Effect::Reorder { id, to } => {
            tx.animated();
            reorder_section(s, &id, to);
        }
    }
}

/// Whether a header's press is on its end slot (Remove, under the pointer).
fn in_end_slot(d: &Drag) -> bool {
    let from_end = if opencode_i18n::direction() == opencode_i18n::Direction::Rtl { d.start_x } else { d.w - d.start_x };
    from_end <= 4.0 + 20.0
}

fn on_element(s: &App, d: &Drag) -> bool {
    let local = (0.0..=d.w).contains(&d.x) && (0.0..=d.h).contains(&d.y);
    let [sx, sy, sw, sh] = s.kernel.slots.untracked(&LIVE).scroll_rect.get();
    let wp = d.window_point();
    let in_scroll = sh <= 0.0 || ((sx..=sx + sw).contains(&wp[0]) && (sy..=sy + sh).contains(&wp[1]));
    local && in_scroll
}

fn scroll_offset(s: &App) -> f32 {
    s.kernel.slots.untracked(&LIVE).scroll_pos.get().offset[1]
}

/// Scrolls `sidebar-sections-scroll` smoothly when a dragged tab or section approaches the top or
/// bottom edge of the viewport, scheduling 60 Hz drag repeats while held there.
fn auto_scroll(s: &App, d: &Drag, tx: &mut Tx) {
    const ACCEL: f32 = 9.0;
    const TOLERANCE: f32 = 24.0;
    const FRAME: Duration = Duration::from_micros(16_667);

    let live = s.kernel.slots.untracked(&LIVE);
    let (at, [x, y, w, h]) = (live.scroll_pos.get(), live.scroll_rect.get());
    if h <= 0.0 || at.max[1] <= 0.0 {
        return;
    }
    let p = d.window_point();
    if !(x - TOLERANCE..=x + w + TOLERANCE).contains(&p[0]) {
        return;
    }
    let band = (h * 0.12).clamp(24.0, 44.0);
    let pos = p[1] - y;
    let speed = if pos <= band && at.offset[1] > 0.0 {
        -ACCEL * ((band - pos) / band).clamp(0.0, 1.5)
    } else if pos >= h - band && at.offset[1] < at.max[1] {
        ACCEL * ((pos - (h - band)) / band).clamp(0.0, 1.5)
    } else {
        0.0
    };
    if speed == 0.0 {
        return;
    }
    let to = (at.offset[1] + speed).clamp(0.0, at.max[1]);
    if (to - at.offset[1]).abs() > 0.01 {
        let mut moved = at;
        moved.offset[1] = to;
        live.scroll_pos.set(moved);
        tx.scroll_to_instant(SCROLL_KEY, 0.0, to);
        tx.drag_repeat_after(FRAME);
    }
}

/// Section `id` moved to index `to` among the list's top-level entries (`0..items.len()`).
pub fn reorder_section(s: &mut App, id: &str, to: usize) {
    rearrange(s, |items| model::place_section(items, id, to));
}

/// Undo on a removed section's toast: the sections as they were, less tabs closed since.
pub fn put_back(s: &mut App, mut doc: Doc) {
    let open: Vec<String> = s.tabs.iter().map(Tab::uid).collect();
    doc.retain_open(&open);
    update(s, |d| *d = doc);
    rearrange(s, |_| {});
}

pub fn close_tabs(s: &mut App, uids: &[String], tx: &mut Tx) {
    clear_selection(s);
    for uid in uids {
        s.close_tab(uid, tx);
    }
}

/// The sessions behind `uids` (drafts have none): server and id.
fn sessions_of(s: &App, uids: &[String]) -> Vec<(ServerKey, String)> {
    uids.iter().filter_map(|u| s.tab(u)).filter(|t| !t.is_draft()).map(|t| (t.server.clone(), t.id.clone())).collect()
}

pub fn session_count(s: &App, uids: &[String]) -> usize {
    sessions_of(s, uids).len()
}

/// Archive each session (each tab closes; the last archive's toast offers its session back).
pub fn archive(s: &mut App, uids: &[String], tx: &mut Tx) {
    clear_selection(s);
    for (server, id) in sessions_of(s, uids) {
        crate::session_menu::SessionMenuHost::archive_session(s, &server, &id, tx);
    }
}

/// Delete each session, once confirmed: they and their subagents leave the list at once and one
/// toast offers them all back; the deletes go only when it does (`session_menu::delete`).
pub fn delete(s: &mut App, uids: &[String]) {
    clear_selection(s);
    let mut gone = Vec::new();
    for (server, id) in sessions_of(s, uids) {
        let hidden = crate::servers::runtime::with_server(s, &server, |s| {
            let mut tree = vec![id.clone()];
            let mut at = 0;
            while at < tree.len() {
                let parent = tree[at].clone();
                tree.extend(s.sessions.iter().filter(|x| x.parent.as_deref() == Some(parent.as_str())).map(|x| x.id.clone()));
                at += 1;
            }
            let hidden: Vec<_> = s.sessions.iter().filter(|x| tree.contains(&x.id)).cloned().collect();
            s.sessions.retain(|x| !tree.contains(&x.id));
            hidden
        });
        gone.push((server, id, hidden.unwrap_or_default()));
    }
    if gone.is_empty() {
        return;
    }
    let n = gone.len();
    crate::undo::offer(s, &t!("rune.undo.sessionsDeleted", count = n), crate::undo::Undoable::DeletedMany(gone));
}

/// A press on tab `uid` (its click): ⌘ toggles it in the selection, ⇧ takes the run to it, a
/// plain click opens it and drops the selection.
pub fn press(s: &mut App, uid: &str, tx: &mut Tx) {
    if drag::step(&mut state_mut(s).drag, drag::Event::Press) != drag::Effect::Click {
        return;
    }
    let live = s.kernel.slots.untracked(&LIVE);
    let (cmd, shift) = (live.cmd.get(), live.shift.get());
    if shift {
        let order = visible_uids(s).unwrap_or_default();
        let from = s.current_tab();
        state_mut(s).selection.step(select::Event::Range { to: uid, order: &order, from: from.as_deref() });
        return;
    }
    if cmd {
        state_mut(s).selection.step(select::Event::Toggle(uid));
        return;
    }
    tx.instant();
    clear_selection(s);
    if s.current_tab().as_deref() != Some(uid) {
        s.select_tab(uid, tx);
    }
}

/// A drag on tab `uid` (at `index` in the window's tabs).
pub fn tab_drag(s: &mut App, uid: &str, index: usize, d: &mut Drag, tx: &mut Tx) {
    if tx.button() != PointerButton::Primary {
        return;
    }
    let live = s.kernel.slots.untracked(&LIVE);
    if d.phase == Phase::Down && (live.cmd.get() || live.shift.get()) {
        // A ⌘- or ⇧-click picks on release; ignore drag moves and tear-out until up.
        drag::step(&mut state_mut(s).drag, drag::Event::Ignore);
        return;
    }
    if matches!(state(s).drag, drag::DragState::Ignoring) {
        drag::step(&mut state_mut(s).drag, if d.phase == Phase::Up { drag::Event::Up { on_self: false } } else { drag::Event::Move { dy: 0.0, scroll_y: 0.0 } });
        return;
    }
    if d.phase == Phase::Down {
        let picked = state(s).selection.contains(uid);
        if !picked {
            clear_selection(s);
            if !session_view::style::touch_controls() && s.current_tab().as_deref() != Some(uid) {
                s.select_tab(uid, tx);
            }
        }
    }
    // Single-tab horizontal tear-out to a new window: park in-sidebar preview while torn out, and
    // resume seamlessly if dragged back over the sidebar before release.
    if !state(s).drag.multi_dragging() && crate::tabs::groups::handle_tab_tear(s, uid, index, d, tx) {
        if d.phase == Phase::Up {
            state_mut(s).drag = if !d.cancelled() && on_element(s, d) {
                drag::DragState::Dropped { uids: Vec::new() }
            } else {
                drag::DragState::Idle
            };
        } else if crate::tabs::groups::torn_out(s) && drag::step(&mut state_mut(s).drag, drag::Event::Park) == drag::Effect::Redraw {
            tx.animated();
        }
        return;
    }
    let event = match d.phase {
        Phase::Down => {
            let sel = state(s).selection.uids().to_vec();
            let picked = sel.iter().any(|u| u == uid);
            let uids = if picked && sel.len() > 1 { in_list_order(s, &sel) } else { vec![uid.to_owned()] };
            let items = shown_items(s);
            let grab_y = model::measure(&items).tab(uid).map_or(d.start_y, |r| r.pill[0] + d.start_y);
            drag::Event::Down {
                lead: uid.to_owned(),
                uids,
                items,
                grab_y,
                start_scroll: scroll_offset(s),
                touch: session_view::style::touch_controls(),
            }
        }
        Phase::Move => {
            if matches!(state(s).drag, drag::DragState::Dragging { .. }) {
                auto_scroll(s, d, tx);
            }
            drag::Event::Move { dy: d.dy, scroll_y: scroll_offset(s) }
        }
        Phase::Up if d.cancelled() => drag::Event::Cancel,
        Phase::Up => drag::Event::Up { on_self: on_element(s, d) },
    };
    let was_active = state(s).drag.is_active();
    match drag::step(&mut state_mut(s).drag, event) {
        drag::Effect::None | drag::Effect::Click | drag::Effect::Reorder { .. } => {}
        drag::Effect::Redraw => {
            if !was_active && state(s).drag.is_active() {
                auto_scroll(s, d, tx);
            }
            tx.animated();
        }
        drag::Effect::Commit { uids, drop } => {
            tx.animated();
            land(s, &uids, &drop);
        }
    }
}

/// `uids` land at `drop`: in a section, or anywhere in the list out of one.
pub fn land(s: &mut App, uids: &[String], drop: &Drop) {
    rearrange(s, |items| model::land(items, uids, drop));
    if let Some(shown) = visible_uids(s) {
        state_mut(s).selection.step(select::Event::Retain(&shown));
    }
}
