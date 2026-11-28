//! Dragging in the sections list as one explicit state machine:
//! - Vertical press + 4 pt travel on a tab drags it (or every selected tab in list order when the
//!   pressed tab is part of a multi-selection): siblings part smoothly on `GLIDE` to show the
//!   exact landing slot, section headers light when targeted, and section-end hairlines preview
//!   bare drops below a section.
//! - Vertical press + 4 pt travel on a Whisper header lifts and carries the entire section
//!   (header + member rows together) past bare tabs or other sections without height flip-flop.
//! - Hysteresis stabilizes every boundary against pointer tremor; `Escape`, window blur or pointer
//!   cancel springs everything back cleanly; trailing clicks are swallowed only when released over
//!   the pressed element itself.

use std::collections::HashMap;

use super::model::{self, BlockPos, Drop, Item, Layout, REACH, RowPos};

const MOUSE_SLOP: f32 = 4.0;
const TOUCH_SLOP: f32 = 8.0;
/// Half-width of the deadband (`±HYST`) around every hit-test boundary so crossing never flickers.
const HYST: f32 = 2.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Carried {
    Lead(f32),
    Tucked(f32),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shifts {
    pub tabs: HashMap<String, f32>,
    pub tucked: HashMap<String, f32>,
    pub heads: HashMap<String, f32>,
    pub blocks: HashMap<String, f32>,
    pub lines: HashMap<String, f32>,
    pub hide_lines: Vec<String>,
    pub dissolve_heads: Vec<String>,
    pub preview_lines: Vec<(String, f32)>,
    pub end_line: Option<f32>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum DragState {
    #[default]
    Idle,
    /// Modifier-click (`⌘`/`⇧`) or header end-slot button press: ignore drag phases until release.
    Ignoring,
    /// Pressed on tab `lead`, carrying `uids` once vertical travel reaches slop.
    Pending {
        lead: String,
        uids: Vec<String>,
        items: Vec<Item>,
        grab_y: f32,
        start_scroll: f32,
        touch: bool,
    },
    Dragging {
        lead: String,
        uids: Vec<String>,
        items: Vec<Item>,
        grab_y: f32,
        start_scroll: f32,
        dy: f32,
        raw_zone: Option<Drop>,
        drop: Option<Drop>,
        shifts: Shifts,
    },
    /// Pressed on section `id`'s header.
    PendingSection {
        id: String,
        uids: Vec<String>,
        items: Vec<Item>,
        start_scroll: f32,
        touch: bool,
    },
    /// Carrying section `id` (header + rows as one block) to index `to` among top-level items.
    DraggingSection {
        id: String,
        uids: Vec<String>,
        items: Vec<Item>,
        start_scroll: f32,
        dy: f32,
        raw_to: usize,
        to: Option<usize>,
        shifts: Shifts,
    },
    /// Released over the pressed element after a drag: swallows the trailing `Press` in this frame.
    Dropped { uids: Vec<String> },
    /// Settling after a drag: keeps the moved `uids` raised while their spring finishes.
    Landed { uids: Vec<String> },
}

pub enum Event {
    Ignore,
    Down {
        lead: String,
        uids: Vec<String>,
        items: Vec<Item>,
        grab_y: f32,
        start_scroll: f32,
        touch: bool,
    },
    Move {
        dy: f32,
        scroll_y: f32,
    },
    /// Horizontal tear-out took the pointer outside the sidebar: close in-sidebar preview shifts
    /// while keeping the press ready if the pointer re-enters the sidebar before release.
    Park,
    DownSection {
        id: String,
        uids: Vec<String>,
        items: Vec<Item>,
        start_scroll: f32,
        touch: bool,
    },
    MoveSection {
        dy: f32,
        scroll_y: f32,
    },
    Up {
        on_self: bool,
    },
    Cancel,
    /// The row's or header's `on_press` after `Up`.
    Press,
}

#[derive(Debug, PartialEq)]
pub enum Effect {
    None,
    Redraw,
    Commit { uids: Vec<String>, drop: Drop },
    Reorder { id: String, to: usize },
    Click,
}

fn slop(touch: bool) -> f32 {
    if touch { TOUCH_SLOP } else { MOUSE_SLOP }
}

pub fn step(state: &mut DragState, event: Event) -> Effect {
    match (std::mem::take(state), event) {
        (_, Event::Ignore) => {
            *state = DragState::Ignoring;
            Effect::None
        }
        (DragState::Ignoring, Event::Move { .. } | Event::MoveSection { .. } | Event::Park) => {
            *state = DragState::Ignoring;
            Effect::None
        }
        (DragState::Ignoring, Event::Up { .. } | Event::Cancel) => Effect::None,
        (DragState::Ignoring, Event::Press) => Effect::Click,
        (_, Event::Down { lead, uids, items, grab_y, start_scroll, touch }) => {
            *state = DragState::Pending { lead, uids, items, grab_y, start_scroll, touch };
            Effect::None
        }
        (_, Event::DownSection { id, uids, items, start_scroll, touch }) => {
            *state = DragState::PendingSection { id, uids, items, start_scroll, touch };
            Effect::None
        }
        (DragState::Dragging { lead, uids, items, grab_y, start_scroll, .. }, Event::Park) => {
            *state = DragState::Pending { lead, uids, items, grab_y, start_scroll, touch: false };
            Effect::Redraw
        }
        (s @ DragState::Pending { .. }, Event::Park) => {
            *state = s;
            Effect::None
        }
        (DragState::Pending { lead, uids, items, grab_y, start_scroll, touch }, Event::Move { dy, scroll_y }) => {
            let vdy = dy + (scroll_y - start_scroll);
            if vdy.abs() < slop(touch) {
                *state = DragState::Pending { lead, uids, items, grab_y, start_scroll, touch };
                return Effect::None;
            }
            *state = update_tab_drag(lead, uids, items, grab_y, start_scroll, vdy, None);
            Effect::Redraw
        }
        (DragState::Dragging { lead, uids, items, grab_y, start_scroll, raw_zone, .. }, Event::Move { dy, scroll_y }) => {
            let vdy = dy + (scroll_y - start_scroll);
            *state = update_tab_drag(lead, uids, items, grab_y, start_scroll, vdy, raw_zone);
            Effect::Redraw
        }
        (DragState::PendingSection { id, uids, items, start_scroll, touch }, Event::MoveSection { dy, scroll_y }) => {
            let vdy = dy + (scroll_y - start_scroll);
            if vdy.abs() < slop(touch) {
                *state = DragState::PendingSection { id, uids, items, start_scroll, touch };
                return Effect::None;
            }
            let from = items.iter().position(|it| it.is_section(&id)).unwrap_or(0);
            *state = update_section_drag(id, uids, items, start_scroll, vdy, from);
            Effect::Redraw
        }
        (DragState::DraggingSection { id, uids, items, start_scroll, raw_to, .. }, Event::MoveSection { dy, scroll_y }) => {
            let vdy = dy + (scroll_y - start_scroll);
            *state = update_section_drag(id, uids, items, start_scroll, vdy, raw_to);
            Effect::Redraw
        }
        (DragState::Dragging { uids, drop: Some(drop), .. }, Event::Up { on_self }) => {
            *state = if on_self { DragState::Dropped { uids: uids.clone() } } else { DragState::Landed { uids: uids.clone() } };
            Effect::Commit { uids, drop }
        }
        (DragState::DraggingSection { id, uids, to: Some(to), .. }, Event::Up { on_self }) => {
            *state = if on_self { DragState::Dropped { uids } } else { DragState::Landed { uids } };
            Effect::Reorder { id, to }
        }
        (DragState::Dragging { .. } | DragState::DraggingSection { .. }, Event::Up { on_self }) => {
            *state = if on_self { DragState::Dropped { uids: Vec::new() } } else { DragState::Idle };
            Effect::Redraw
        }
        (DragState::Dragging { .. } | DragState::DraggingSection { .. }, Event::Cancel) => Effect::Redraw,
        (DragState::Dropped { uids }, Event::Press) => {
            *state = if uids.is_empty() { DragState::Idle } else { DragState::Landed { uids } };
            Effect::None
        }
        (DragState::Idle | DragState::Pending { .. } | DragState::PendingSection { .. } | DragState::Landed { .. }, Event::Press) => Effect::Click,
        (DragState::Pending { .. } | DragState::PendingSection { .. }, Event::Up { .. } | Event::Cancel) => Effect::None,
        (s, _) => {
            *state = s;
            Effect::None
        }
    }
}

fn update_tab_drag(
    lead: String,
    uids: Vec<String>,
    items: Vec<Item>,
    grab_y: f32,
    start_scroll: f32,
    raw_dy: f32,
    prev_raw: Option<Drop>,
) -> DragState {
    let before = model::measure(&items);
    let (dy, base_y) = match before.tab(&lead) {
        Some(r) => (
            raw_dy.clamp(-r.pill[0] - 20.0, (before.total_h - r.pill[1] + 28.0).max(16.0)),
            (r.pill[0] + r.pill[1]) * 0.5,
        ),
        None => (raw_dy, grab_y),
    };
    // Measure from the carried pill's vertical centre with a gentle leading-edge advance so a
    // 1-slot drag (`±31 pt`) swaps cleanly regardless of where inside the 28 pt row the press landed.
    let pointer_y = base_y + dy + dy.clamp(-8.0, 8.0);
    let next_raw = zone_at(&before, &uids, pointer_y);
    let raw_zone = match prev_raw {
        p if p != next_raw && (zone_at(&before, &uids, pointer_y - HYST) == p || zone_at(&before, &uids, pointer_y + HYST) == p) => p,
        _ => next_raw,
    };
    let mut after_items = items.clone();
    let drop = raw_zone.clone().filter(|d| {
        model::land(&mut after_items, &uids, d);
        after_items != items
    });
    let mut shifts = match &drop {
        Some(d) => tab_shifts(&before, &model::measure(&after_items), &uids, d),
        None => Shifts::default(),
    };
    if let Some(lr) = before.tab(&lead) {
        let lead_top = lr.slot_top + dy;
        for u in &uids {
            if u != &lead && let Some(ur) = before.tab(u) {
                shifts.tucked.insert(u.clone(), lead_top - ur.slot_top);
            }
        }
    }
    DragState::Dragging { lead, uids, items, grab_y, start_scroll, dy, raw_zone, drop, shifts }
}

fn update_section_drag(
    id: String,
    uids: Vec<String>,
    items: Vec<Item>,
    start_scroll: f32,
    raw_dy: f32,
    prev_raw: usize,
) -> DragState {
    let before = model::measure(&items);
    let Some(from) = items.iter().position(|it| it.is_section(&id)) else {
        return DragState::DraggingSection { id, uids, items, start_scroll, dy: raw_dy, raw_to: 0, to: None, shifts: Shifts::default() };
    };
    let own = before.blocks[from].span();
    let dy = raw_dy.clamp(-own[0] - 8.0, (before.total_h - own[1] + 18.0).max(10.0));
    let next_raw = section_slot(&before, from, own, dy);
    let raw_to = if next_raw != prev_raw
        && (section_slot(&before, from, own, dy - HYST) == prev_raw || section_slot(&before, from, own, dy + HYST) == prev_raw)
    {
        prev_raw
    } else {
        next_raw
    };
    let to = (raw_to != from).then_some(raw_to);
    let shifts = match to {
        Some(target) => {
            let mut after_items = items.clone();
            model::place_section(&mut after_items, &id, target);
            section_shifts(&before, &model::measure(&after_items), &id)
        }
        None => Shifts::default(),
    };
    DragState::DraggingSection { id, uids, items, start_scroll, dy, raw_to, to, shifts }
}

/// Monotone slot `0..blocks.len()` for a section dragged by `dy` from `from`: compares the
/// dragged block's top edge when moving up and its bottom edge when moving down against frozen
/// block midpoints, eliminating height-difference oscillation.
fn section_slot(layout: &Layout, from: usize, own: [f32; 2], dy: f32) -> usize {
    if dy < 0.0 {
        let drag_top = own[0] + dy;
        let up = layout.blocks[..from].iter().filter(|b| {
            let [t, btm] = b.span();
            drag_top < (t + btm) * 0.5
        }).count();
        from - up
    } else if dy > 0.0 {
        let drag_bot = own[1] + dy;
        let down = layout.blocks[from + 1..].iter().filter(|b| {
            let [t, btm] = b.span();
            drag_bot > (t + btm) * 0.5
        }).count();
        from + down
    } else {
        from
    }
}

/// Exhaustive partition of `(-∞, +∞)` into drop candidates for `carried` at content `y`.
fn zone_at(layout: &Layout, carried: &[String], y: f32) -> Option<Drop> {
    let blocks = &layout.blocks;
    if blocks.is_empty() {
        return None;
    }
    for (n, block) in blocks.iter().enumerate() {
        let next = blocks.get(n + 1);
        match block {
            BlockPos::Tab(r) => {
                let end = match next {
                    Some(BlockPos::Tab(nr)) => (r.pill[1] + nr.pill[0]) * 0.5,
                    Some(BlockPos::Section { head, .. }) => head[0] - 4.5,
                    None => f32::INFINITY,
                };
                if y < end {
                    let mid = (r.pill[0] + r.pill[1]) * 0.5;
                    return Some(if y < mid { Drop::Before(r.uid.clone()) } else { Drop::After(r.uid.clone()) });
                }
            }
            BlockPos::Section { id, collapsed, members, block: span, head, rows, line } => {
                let before_sec_end = head[0] + if *collapsed { 4.0 } else { 7.5 };
                if y < before_sec_end {
                    return Some(Drop::BeforeSection(id.clone()));
                }
                if *collapsed {
                    let into_end = match next {
                        Some(BlockPos::Tab(_)) => line.unwrap_or(head[1] + 6.0),
                        Some(BlockPos::Section { .. }) => head[1] + 3.5,
                        None => head[1] + 6.0,
                    };
                    if y < into_end {
                        return Some(Drop::Into { id: id.clone(), prepend: false });
                    }
                    if next.is_none() {
                        return Some(Drop::End);
                    }
                    continue;
                }
                let from_this = carried.iter().all(|u| members.contains(u));
                let stay: Vec<&RowPos> = rows.iter().filter(|r| !carried.contains(&r.uid)).collect();
                let head_end = rows.first().map_or(head[1] + 2.0, |r| r.pill[0]);
                if y < head_end {
                    return if from_this {
                        stay.first().map(|r| Drop::Before(r.uid.clone()))
                    } else {
                        Some(Drop::Into { id: id.clone(), prepend: true })
                    };
                }
                let bot = span[1] - REACH;
                let sec_end = match next {
                    Some(BlockPos::Tab(_)) => line.unwrap_or(bot + 6.0),
                    Some(BlockPos::Section { .. }) => bot + 4.5,
                    None => bot + 9.0,
                };
                if y < sec_end {
                    for (k, r) in rows.iter().enumerate() {
                        let r_end = rows.get(k + 1).map_or(sec_end, |nr| (r.pill[1] + nr.pill[0]) * 0.5);
                        if y < r_end {
                            let mid = (r.pill[0] + r.pill[1]) * 0.5;
                            return Some(if y < mid { Drop::Before(r.uid.clone()) } else { Drop::After(r.uid.clone()) });
                        }
                    }
                    return None;
                }
                if next.is_none() {
                    return Some(Drop::End);
                }
            }
        }
    }
    Some(Drop::End)
}

fn tab_shifts(before: &Layout, after: &Layout, carried: &[String], drop: &Drop) -> Shifts {
    let mut out = Shifts::default();
    let mut any_moved = false;
    for b in &before.blocks {
        match b {
            BlockPos::Tab(r) if !carried.contains(&r.uid) => {
                if let Some(ar) = after.tab(&r.uid) {
                    let d = ar.slot_top - r.slot_top;
                    if d.abs() > 0.01 {
                        out.tabs.insert(r.uid.clone(), d);
                        any_moved = true;
                    }
                }
            }
            BlockPos::Section { id, head, rows, line, .. } => {
                match after.head_top(id) {
                    Some(ah) => {
                        let d = ah - head[0];
                        if d.abs() > 0.01 {
                            out.heads.insert(id.clone(), d);
                            any_moved = true;
                        }
                    }
                    None => out.dissolve_heads.push(id.clone()),
                }
                for r in rows.iter().filter(|r| !carried.contains(&r.uid)) {
                    if let Some(ar) = after.tab(&r.uid) {
                        let d = ar.slot_top - r.slot_top;
                        if d.abs() > 0.01 {
                            out.tabs.insert(r.uid.clone(), d);
                            any_moved = true;
                        }
                    }
                }
                match (line, after.line_y(id)) {
                    (Some(bl), Some(al)) if (al - bl).abs() > 0.01 => _ = out.lines.insert(id.clone(), al - bl),
                    (Some(_), None) => out.hide_lines.push(id.clone()),
                    (None, Some(al)) => out.preview_lines.push((id.clone(), al)),
                    _ => {}
                }
            }
            BlockPos::Tab(_) => {}
        }
    }
    if matches!(drop, Drop::End) && !any_moved && let Some(first) = carried.first() && let Some(ar) = after.tab(first) {
        out.end_line = Some(ar.pill[0] - REACH);
    }
    out
}

fn section_shifts(before: &Layout, after: &Layout, dragged_id: &str) -> Shifts {
    let mut out = Shifts::default();
    for b in &before.blocks {
        match b {
            BlockPos::Tab(r) => {
                if let Some(ar) = after.tab(&r.uid) {
                    let d = ar.slot_top - r.slot_top;
                    if d.abs() > 0.01 {
                        out.tabs.insert(r.uid.clone(), d);
                    }
                }
            }
            BlockPos::Section { id, block, line, .. } if id != dragged_id => {
                if let Some(ab) = after.block_span(id) {
                    let d = ab[0] - block[0];
                    if d.abs() > 0.01 {
                        out.blocks.insert(id.clone(), d);
                    }
                }
                match (line, after.line_y(id)) {
                    (Some(bl), Some(al)) if (al - bl).abs() > 0.01 => _ = out.lines.insert(id.clone(), al - bl),
                    (Some(_), None) => out.hide_lines.push(id.clone()),
                    (None, Some(al)) => out.preview_lines.push((id.clone(), al)),
                    _ => {}
                }
            }
            BlockPos::Section { id, line: Some(_), .. } if id == dragged_id => {
                out.hide_lines.push(id.clone());
            }
            BlockPos::Section { .. } => {}
        }
    }
    out
}

impl DragState {
    pub fn is_idle(&self) -> bool {
        matches!(self, DragState::Idle)
    }

    pub fn is_active(&self) -> bool {
        matches!(self, DragState::Dragging { .. } | DragState::DraggingSection { .. })
    }

    pub fn is_gesture(&self) -> bool {
        matches!(
            self,
            DragState::Pending { .. }
                | DragState::Dragging { .. }
                | DragState::PendingSection { .. }
                | DragState::DraggingSection { .. }
        )
    }

    /// How tab `uid` participates in an active tab drag.
    pub fn carried(&self, uid: &str) -> Option<Carried> {
        match self {
            DragState::Dragging { lead, dy, .. } if lead == uid => Some(Carried::Lead(*dy)),
            DragState::Dragging { uids, shifts, .. } if uids.iter().any(|u| u == uid) => {
                Some(Carried::Tucked(shifts.tucked.get(uid).copied().unwrap_or(0.0)))
            }
            _ => None,
        }
    }

    /// Number of tabs carried when `uid` is the dragged lead of a multi-selection (`>= 2`).
    pub fn carried_count(&self, uid: &str) -> Option<usize> {
        match self {
            DragState::Dragging { lead, uids, .. } if lead == uid && uids.len() > 1 => Some(uids.len()),
            _ => None,
        }
    }

    pub fn multi_dragging(&self) -> bool {
        matches!(self, DragState::Pending { uids, .. } | DragState::Dragging { uids, .. } if uids.len() > 1)
    }

    pub fn drop(&self) -> Option<&Drop> {
        match self {
            DragState::Dragging { drop, .. } => drop.as_ref(),
            _ => None,
        }
    }

    /// Whether the active drop brings at least one external tab into section `id` via a member row.
    pub fn joining_section(&self, members: &[String]) -> bool {
        match self {
            DragState::Dragging { uids, drop: Some(Drop::Before(u) | Drop::After(u)), .. } => {
                members.contains(u) && uids.iter().any(|c| !members.contains(c))
            }
            _ => false,
        }
    }

    /// Whether tab `uid` was just moved by a drag, gliding over neighbours until settled.
    pub fn landing(&self, uid: &str) -> bool {
        matches!(self, DragState::Dropped { uids } | DragState::Landed { uids } if uids.iter().any(|u| u == uid))
    }

    /// How far section `id`'s whole block is carried by the pointer.
    pub fn carried_section(&self, id: &str) -> Option<f32> {
        match self {
            DragState::DraggingSection { id: dragged, dy, .. } if dragged == id => Some(*dy),
            _ => None,
        }
    }

    pub fn shifts(&self) -> Option<&Shifts> {
        match self {
            DragState::Dragging { shifts, .. } | DragState::DraggingSection { shifts, .. } => Some(shifts),
            _ => None,
        }
    }

    pub fn tab_shift(&self, uid: &str) -> f32 {
        self.shifts().and_then(|s| s.tabs.get(uid).copied()).unwrap_or(0.0)
    }

    pub fn head_shift(&self, id: &str) -> f32 {
        self.shifts().and_then(|s| s.heads.get(id).copied()).unwrap_or(0.0)
    }

    pub fn head_dissolving(&self, id: &str) -> bool {
        self.shifts().is_some_and(|s| s.dissolve_heads.iter().any(|x| x == id))
    }

    pub fn block_shift(&self, id: &str) -> f32 {
        self.shifts().and_then(|s| s.blocks.get(id).copied()).unwrap_or(0.0)
    }

    pub fn line_shift(&self, id: &str) -> f32 {
        self.shifts().and_then(|s| s.lines.get(id).copied()).unwrap_or(0.0)
    }

    pub fn line_hidden(&self, id: &str) -> bool {
        self.shifts().is_some_and(|s| s.hide_lines.iter().any(|x| x == id))
    }
}
