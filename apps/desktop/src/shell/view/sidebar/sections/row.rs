//! A side tab in the sections list: pixel-identical to vertical `TabStrip` (`tab_strip/tab.rs`):
//! 28 pt tall, `px(8) gap(8)`, a 16 pt avatar, `text-[13px] font-medium` in `p.faint` (`p.text`
//! when open or picked) running the full pill width at rest, and an out-of-flow 20 pt status/close
//! slot pinned 4 pt from the pill's end so inactive titles lose zero width.

use super::*;
use crate::tabs::sections::drag::Carried;
use crate::tabs::sections::{self as sec, LIVE};
use opencode_i18n::t;
use rune::{Drag, PointerPress};
use rune_kit::shell::TabItemStatus;

/// The slot at a row's end, 4 pt in from it (matching `TabStrip`, `tab-nav.css:4`): a tab's
/// mark, a folded section's, and the button that takes the mark's place under the pointer, all
/// on one column down the list.
pub(super) const SLOT: f32 = 20.0;
const SLOT_END: f32 = 4.0;
const END_CLEAR: f32 = SLOT + SLOT_END;
const TITLE_FADE: f32 = 16.0;
const BADGE_FONT: TextStyle = TextStyle::new(11.0, 560.0).lh(14.0).features("\"tnum\"");

/// `what` centred in the slot, out of flow: the button that shares the slot can't push it aside.
pub(super) fn in_slot(what: El) -> El {
    row().key("mark").absolute(0.0, 0.0).size(SLOT, SLOT).center().a11y_hidden().child(what)
}

/// What a status draws in the slot: the busy glyph, or a 7 pt dot.
pub(super) fn status_mark(p: &Pal, status: TabItemStatus) -> El {
    let dot = |key: &'static str, c: Color| el().key(key).size(7.0, 7.0).radius(3.5).fill(c).a11y_hidden();
    match status {
        TabItemStatus::Busy => session_view::style::tab_busy_glyph(crate::palette::kit(p), 14.0),
        TabItemStatus::Attention => dot("attention", p.warning),
        TabItemStatus::Unread => dot("unread", p.bg_accent),
        TabItemStatus::Failed => dot("failed", p.danger),
    }
}

/// `over`: this tab flies above the others (its section was just made).
pub(super) fn tab_row(s: &App, p: &Pal, t: &crate::tabs::Tab, index: usize, nav_order: &[String], show_hints: bool, over: bool) -> El {
    let st = sec::state(s);
    let uid = t.uid();
    let draft = t.is_draft();
    let face = crate::view::tab_strip::face(s, t);
    let target = crate::view::tab_strip::target(s, t);
    let marked = s.kernel.slots.get(&crate::session_menu::MENUS).on.as_ref() == Some(&target);
    let active = !s.settings_open() && s.current_tab().as_deref() == Some(uid.as_str());
    let picked = st.selection.contains(&uid);
    let carried = st.drag.carried(&uid);
    let lifted = matches!(carried, Some(Carried::Lead(_)));
    let drag_count = st.drag.carried_count(&uid);
    let raw_mark = crate::view::tab_strip::status(s, t);
    let muted = !draft && s.project_navigation() && crate::session_menu::preferences::muted(&t.server, &t.id) && raw_mark != Some(TabItemStatus::Busy);
    let mark = if muted { None } else { raw_mark };
    let shortcut = nav_order.iter().position(|u| u == &uid).filter(|&i| i < 9).map(|i| i + 1);
    let avatar = match (draft, shortcut) {
        (true, Some(n)) => crate::view::tab_strip::draft_avatar(p, active, n, show_hints),
        (true, None) => icon(&crate::view::tab_strip::DRAFT_PENCIL, 16.0).color(if active { p.icon } else { p.icon_muted }),
        (false, _) => crate::icons::tab_badge(s, p, face.project, &project_or_dir(face.project, face.dir), shortcut, show_hints),
    };
    let status = mark.filter(|m| !active || matches!(m, TabItemStatus::Busy | TabItemStatus::Attention)).map(|m| status_mark(p, m));
    let editing = crate::session_menu_view::field(s, p, &target, UI_MED);
    let is_editing = editing.is_some();
    let font = TextStyle { weight: UI_MED.weight, tracking: 0.0, ..UI };
    let ink = if active || picked { p.text } else { p.faint };
    let title = match editing {
        Some(field) => row().key("title").grow(1.0).shrink().min_w(0.0).child(field),
        None if active || status.is_some() || drag_count.is_some() => {
            let label = text(face.title.clone()).font(font).color(ink).grow(1.0).shrink().a11y_hidden().dir(rune::Direction::Auto);
            row()
                .key("title")
                .grow(1.0)
                .shrink()
                .min_w(0.0)
                .mr(END_CLEAR)
                .clip_fade_linear(0.0, 0.0, TITLE_FADE, 0.0)
                .child(label.truncate_fade_width(0.0).mr(-END_CLEAR))
        }
        None => {
            let make = || text(face.title.clone()).font(font).color(ink).grow(1.0).shrink().a11y_hidden().dir(rune::Direction::Auto);
            let rest = row().key("rest").pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0)).center_y().child(make().truncate_fade_width(TITLE_FADE / font.size)).group_opacity(1.0, 0.0);
            let hov = row()
                .key("emph")
                .pin(Some(0.0), Some(0.0), Some(END_CLEAR), Some(0.0))
                .center_y()
                .clip_fade_linear(0.0, 0.0, TITLE_FADE, 0.0)
                .child(make().truncate_fade_width(0.0).mr(-END_CLEAR))
                .group_opacity(0.0, 1.0);
            el().key("title").grow(1.0).shrink().min_w(0.0).h(28.0).child(rest).child(hov)
        }
    };
    let cut = |e: El| e.transition(rune::Transition::none()).motion_intent(rune::MotionIntent::CUT);
    let tail = (!is_editing).then(|| match drag_count {
        Some(n) => cut(row()
            .key("drag-count")
            .pin(None, Some(5.0), Some(SLOT_END), None)
            .h(18.0)
            .min_w(18.0)
            .px(5.0)
            .center()
            .radius(9.0)
            .fill(p.bg_accent)
            .a11y_hidden()
            .child(text(n.to_string()).font(BADGE_FONT).color(Color::WHITE))),
        None => {
            let close_uid = uid.clone();
            let close = row()
                .key(format!("sec-close-{uid}"))
                .role("button")
                .label_static(t!("common.closeTab"))
                .focusable(true)
                .absolute(0.0, 0.0)
                .size(SLOT, SLOT)
                .center()
                .radius(4.0)
                .focus_inset(p)
                .hover(p.hover)
                .press(p.pressed)
                .on_press_tx(move |s: &mut App, tx: &mut Tx| s.close_tab(&close_uid, tx))
                .child(icon(&ic::XMARK_SMALL, 14.0).color(p.icon_muted));
            cut(el().key("tail")
                .pin(None, Some((28.0 - SLOT) * 0.5), Some(SLOT_END), None)
                .size(SLOT, SLOT)
                .children(status.map(|m| in_slot(m).group_opacity(1.0, 0.0)))
                .child(session_view::style::hover_reveal(close).instant_changes()))
        }
    });

    let (press_uid, middle_uid, hover_uid, drag_uid, menu_uid) = (uid.clone(), uid.clone(), uid.clone(), uid.clone(), uid.clone());
    let anchor = format!("sec-tab-{uid}");
    let hover_anchor = anchor.clone();
    let layout_key = format!("row:{uid}");
    let rename = target.clone();
    let landing = !st.drag.is_gesture() && st.drag.landing(&uid);
    let raised = over || lifted || landing;
    let fill = if picked {
        p.accent.alpha(if p.dark { 0.15 } else { 0.11 })
    } else if active || lifted {
        p.pressed
    } else if marked {
        p.hover
    } else {
        Color::TRANSPARENT
    };
    let mut r = row()
        .key(anchor)
        .role("tab")
        .label(face.title.clone())
        .focus_selected(active)
        .w_full()
        .h(28.0)
        .px(8.0)
        .gap(8.0)
        .center_y()
        .radius(6.0)
        .focus_inset(p)
        .fill(p.bg_deep)
        .hover(if picked || active || lifted || marked || is_editing { Color::TRANSPARENT } else { p.hover })
        .hover_group_focus()
        .hover_motion(INSTANT, INSTANT)
        .motion(super::GLIDE)
        .on_layout_tx(move |s: &mut App, r: [f32; 4], tx: &mut Tx| {
            tx.live_only();
            s.kernel.slots.untracked(&LIVE).rects.borrow_mut().insert(layout_key.clone(), r);
        })
        .child(el().key("wash").pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0)).radius(6.0).fill(fill).motion(super::GLIDE).a11y_hidden())
        .child(el().size(16.0, 16.0).center().a11y_hidden().child(avatar))
        .child(title)
        .children(tail);
    r = if lifted {
        r.elevate(p.floating)
    } else {
        r.box_shadow(Pal::ring_in(p.accent.alpha(if picked { 0.14 } else { 0.0 })))
    };
    if !is_editing {
        r = r
            .on_press_tx(move |s: &mut App, tx: &mut Tx| sec::press(s, &press_uid, tx))
            .closable_tab(move |s: &mut App, tx: &mut Tx| s.close_tab(&middle_uid, tx))
            .on_double_press_tx(move |s: &mut App, tx: &mut Tx| {
                tx.instant();
                s.begin_rename(&rename);
            })
            .on_hover_change(move |s: &mut App, inside, tx: &mut Tx| {
                if inside {
                    s.prefetch_session(&hover_uid);
                }
                s.preview_hover_at(&hover_uid, &hover_anchor, inside, tx);
            })
            .on_drag(move |s: &mut App, d: &mut Drag, tx: &mut Tx| sec::tab_drag(s, &drag_uid, index, d, tx))
            .on_secondary_press(move |s: &mut App, press: PointerPress, tx: &mut Tx| super::menu::tab(s, &menu_uid, press, tx));
    }

    let stacked = drag_count.map(|_| {
        el().key("stack")
            .pin(Some(3.0), Some(super::REACH + 3.0), Some(3.0), None)
            .h(28.0)
            .radius(6.0)
            .fill(p.bg_deep)
            .box_shadow(Pal::ring_in(p.accent.alpha(0.32)))
            .a11y_hidden()
    });
    let mut slot = el()
        .key(format!("sec-slot-{uid}"))
        .w_full()
        .h(28.0 + super::REACH * 2.0)
        .py(super::REACH)
        .motion(super::GLIDE)
        .matched(format!("{}-{uid}", crate::view::tab_strip::MATCHED))
        .children(stacked)
        .child(r);
    if muted {
        slot = super::super::pins::marked_slot(slot, super::super::pins::muted_mark(p), raw_mark);
    }
    slot = if raised { slot.raise() } else { slot };
    match carried {
        Some(Carried::Lead(dy)) => slot.opacity(1.0).opacity_motion(rune::Spring::NONE).offset(0.0, dy).offset_motion(rune::Anim::INSTANT).motion_intent(rune::MotionIntent::CUT),
        Some(Carried::Tucked(dy)) => slot.opacity(0.0).opacity_motion(rune::Spring::NONE).offset(0.0, dy).offset_motion(rune::Anim::INSTANT).motion_intent(rune::MotionIntent::CUT),
        None => slot.opacity(1.0).opacity_motion(rune::Spring::NONE).offset(0.0, st.drag.tab_shift(&uid)).motion_intent(rune::MotionIntent::CUT),
    }
}
