//! A Whisper header: no box, no border, no colour; a faint label on the tabs' own spine (their
//! avatars' start edge), its disclosure triangle hung in the gutter before it (shown on hover, or
//! while folded), so the label never moves. Folded, it counts its tabs and shows whether one works
//! or waits, in the tabs' own end slot. A press folds it, a drag carries the section anywhere in
//! the list, a double press or Rename section names it in place, and a right-click opens its menu.
//! A tab dragged over it lights it: an accent wash and ring, its label bright.

use super::*;
use crate::tabs::sections::{self as sec, Drop, FIELD, LIVE, SECTIONS, Section, naming};
use opencode_i18n::t;
use rune::{Drag, Key};
use rune_kit::shell::TabItemStatus;

/// `text-[11.5px] font-medium`, a touch open: quieter than a tab's title.
const LABEL: TextStyle = TextStyle::new(11.5, 520.0).lh(16.0).tracking(0.12);
/// A folded section's count, in even figures.
const COUNT: TextStyle = TextStyle::new(11.5, 500.0).lh(16.0).features("\"tnum\"");
/// The disclosure triangle's quarter turn.
const TURN: Spring = Spring::new(0.2, 0.0);
/// The triangle coming and going with the pointer.
const SHOW: Spring = Spring::new(0.14, 0.0);
/// The triangle's 12 pt box, centred on the header's start edge: its ink hangs in the gutter,
/// 6 pt clear of the label.
const HANG: f32 = 6.0;
/// The header's height, and the name field's inside it.
const H: f32 = 26.0;
const FIELD_H: f32 = 24.0;

/// What a folded section's tabs are doing, most pressing first.
fn mark(s: &App, tabs: &[usize]) -> Option<TabItemStatus> {
    let statuses: Vec<TabItemStatus> = tabs.iter().filter_map(|&i| s.tabs.get(i)).filter_map(|t| crate::view::tab_strip::status(s, t)).collect();
    [TabItemStatus::Busy, TabItemStatus::Attention].into_iter().find(|m| statuses.contains(m)).or_else(|| (!statuses.is_empty()).then_some(TabItemStatus::Unread))
}

pub(super) fn header(s: &App, p: &Pal, section: &Section, tabs: &[usize]) -> El {
    let st = sec::state(s);
    let collapsed = section.collapsed;
    let editing = st.naming.editing().filter(|(id, _)| *id == section.id).map(|(_, field)| field);
    let carried = st.drag.carried_section(&section.id);
    let direct_target = matches!(st.drag.drop(), Some(Drop::Into { id, .. }) if *id == section.id);
    let member_target = !direct_target && st.drag.joining_section(&section.members);
    let target = direct_target || member_target;
    let dissolving = st.drag.head_dissolving(&section.id);
    let head_shift = st.drag.head_shift(&section.id);

    let (toggle_id, rename_id, menu_id, drag_id, layout_key) = (
        section.id.clone(),
        section.id.clone(),
        section.id.clone(),
        section.id.clone(),
        format!("head:{}", section.id),
    );
    let a11y = if collapsed { format!("{} ({})", section.name, tabs.len()) } else { section.name.clone() };
    let mut head = row()
        .key(format!("sec-head-{}", section.id))
        .role("button")
        .label(a11y)
        .expanded(!collapsed)
        .w_full()
        .h(H)
        .center_y()
        .radius(6.0)
        .focus_inset(p)
        .hover_group_focus()
        .hover_motion(INSTANT, INSTANT)
        .motion(super::GLIDE)
        .offset(0.0, head_shift)
        .opacity(if dissolving { 0.28 } else { 1.0 })
        .on_layout_tx(move |s: &mut App, r: [f32; 4], tx: &mut Tx| {
            tx.live_only();
            s.kernel.slots.untracked(&LIVE).rects.borrow_mut().insert(layout_key.clone(), r);
        })
        .transition(rune::Transition::opacity().springs(rune::SpringPair::new(super::GLIDE, Spring::new(0.16, 0.0))));
    if let Some(field) = editing {
        return head.child(name_field(p, field));
    }
    let triangle = icon(&ic::FILL_TRIANGLE_DOWN, 12.0)
        .key("triangle")
        .color(p.icon_faint)
        .group_color(p.icon_muted)
        .rotate(if collapsed { -std::f32::consts::FRAC_PI_2 } else { 0.0 })
        .rotate_motion(TURN)
        .mirror_in_rtl();
    let disclosure = el().key("disclosure").pin(Some(-HANG), Some((H - 12.0) / 2.0), None, None).size(12.0, 12.0).center().a11y_hidden().child(triangle);
    let disclosure = if carried.is_some() {
        disclosure.opacity(0.0)
    } else if collapsed {
        disclosure
    } else {
        session_view::style::hover_reveal(disclosure).group_motion(SHOW, SHOW)
    };
    let label = text(section.name.clone())
        .key("name")
        .font(LABEL)
        .color(if target || carried.is_some() { p.text } else { p.faint })
        .group_color(p.text)
        .grow(1.0)
        .shrink()
        .min_w(0.0)
        .truncate()
        .dir(rune::Direction::Auto);
    let mut slot = el().key("end").size(super::row::SLOT, super::row::SLOT);
    let mut beside = None;
    if collapsed {
        let count = text(tabs.len().to_string()).key("count").font(COUNT).color(p.faint);
        match mark(s, tabs).map(|m| super::row::status_mark(p, m)) {
            Some(m) => {
                beside = Some(count);
                slot = slot.child(super::row::in_slot(m).group_opacity(1.0, 0.0));
            }
            None => slot = slot.child(super::row::in_slot(count).group_opacity(1.0, 0.0)),
        }
    }
    let id = section.id.clone();
    let remove = row()
        .key("remove")
        .role("button")
        .label_static(t!("rune.sections.remove"))
        .help(t!("rune.sections.remove"))
        .absolute(0.0, 0.0)
        .size(super::row::SLOT, super::row::SLOT)
        .center()
        .radius(4.0)
        .focus_inset(p)
        .hover(p.hover)
        .press(p.pressed)
        .on_press_tx(move |s: &mut App, tx: &mut Tx| sec::remove_section(s, &id, tx))
        .child(icon(&ic::XMARK_SMALL, 14.0).color(p.icon_muted));
    let end = row().key("tail").gap(3.0).center_y().children(beside).child(slot.child(session_view::style::hover_reveal(remove)));
    let wash = if direct_target {
        p.accent.alpha(if p.dark { 0.16 } else { 0.12 })
    } else if member_target {
        p.accent.alpha(if p.dark { 0.08 } else { 0.06 })
    } else {
        Color::TRANSPARENT
    };
    let ring = if direct_target {
        0.32
    } else if member_target {
        0.18
    } else {
        0.0
    };
    head = head
        .pl(8.0)
        .pr(4.0)
        .gap(8.0)
        .child(disclosure)
        .child(label)
        .child(end)
        .fill(wash)
        .box_shadow(Pal::ring_in(p.accent.alpha(ring)))
        .on_press_tx(move |s: &mut App, tx: &mut Tx| sec::header_press(s, &toggle_id, tx))
        .on_drag(move |s: &mut App, d: &mut Drag, tx: &mut Tx| sec::header_drag(s, &drag_id, d, tx))
        .on_double_press_tx(move |s: &mut App, tx: &mut Tx| {
            sec::start_rename(s, &rename_id, tx);
        })
        .on_secondary_press(move |s: &mut App, press: rune::PointerPress, tx: &mut Tx| super::menu::header(s, &menu_id, press.into(), tx));
    head
}

/// The name field: a quiet well the width of a tab's pill under a hairline accent ring, the name
/// selected so typing replaces it, its text on the label's spine.
fn name_field(p: &Pal, field: &rune::TextEdit) -> El {
    let input = session_view::style::field(field, LABEL)
        .key(FIELD)
        .label(t!("rune.sections.name"))
        .placeholder(t!("rune.sections.name"), p.faint)
        .spellcheck(false)
        .single_line()
        .dir(rune::Direction::Auto)
        .truncate_fade()
        .w_full()
        .color(p.text)
        .caret(p.text, p.selection)
        .selection_style(p.field_selection)
        .on_edit(|s: &mut App| {
            let st = s.kernel.slots.get_mut(&SECTIONS);
            match st.naming.field_mut() {
                Some(f) => f,
                None => &mut st.scratch,
            }
        })
        .on_input_key(|s: &mut App, ev, tx| {
            let event = match ev.key {
                Key::Enter => naming::Event::Commit,
                Key::Escape => naming::Event::Cancel,
                _ => return false,
            };
            sec::naming_step(s, event, tx);
            tx.blur();
            true
        });
    row()
        .key("name-field")
        .transition(rune::Transition::opacity().springs(rune::SpringPair::new(super::GLIDE, Spring::new(0.08, 0.0))))
        .grow(1.0)
        .shrink()
        .min_w(0.0)
        .h(FIELD_H)
        .px(8.0)
        .center_y()
        .radius(6.0)
        .fill(p.hover)
        .box_shadow(Pal::ring_in(p.accent.alpha(0.5)))
        .child(input)
}
