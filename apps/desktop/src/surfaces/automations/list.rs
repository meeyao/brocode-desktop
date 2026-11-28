//! The list beside the inspector: every automation under its status, Completed folded away
//! until asked for (packages/app/src/automations/route.tsx, view.tsx:AutomationListRow).

use opencode_i18n::t;
use opencode_kit::model::{AutomationOutcome, AutomationStatus, AutomationTrigger};
use opencode_ui::palette::Pal;
use opencode_ui::widgets::BODY;
use rune::{Anim, Color, Direction, El, TextStyle, Tx, col, el, icon, row, text};
use rune_theme_opencode::icons as ic;
use session_view::focus::FocusExt as _;

use crate::App;

use super::AUTOMATIONS;
use super::format::short_duration;
use super::source::Entry;

/// A group's name, set as the sidebar's section headers are (`sidebar/sections/header.rs`: 11.5,
/// a touch open) but a step heavier and in the muted ink, so it stands over the rows' faint
/// second lines instead of reading as one more of them.
const GROUP_LABEL: TextStyle = TextStyle::new(11.5, 550.0).lh(16.0).tracking(0.12);
/// Its count, in even figures and the faint ink: beside the name, never competing with it.
const GROUP_COUNT: TextStyle = TextStyle::new(11.5, 500.0).lh(16.0).features("\"tnum\"");
const GROUP_H: f32 = 24.0;
/// The spine every header, rule and row keeps: 10 pt inside a row's pill, under the page title.
const SPINE: f32 = 10.0;
const ROW_TITLE: TextStyle = TextStyle::new(13.0, 530.0).lh(20.0).tracking(-0.04);
const CHIP: TextStyle = TextStyle::new(12.0, 530.0).lh(16.0).features("\"tnum\"");
const ROW_SUB: TextStyle = TextStyle::new(12.0, 440.0).lh(16.0);

const STATUSES: [AutomationStatus; 3] = [AutomationStatus::Active, AutomationStatus::Paused, AutomationStatus::Done];

/// The list in `viewport`, which the page sizes: a column beside the inspector, or a band over it.
pub(super) fn pane(s: &App, p: &Pal, viewport: El, filtered: &[Entry], selected: Option<&Entry>, context: impl Fn(&Entry) -> Vec<String>) -> El {
    let st = s.kernel.slots.get(&AUTOMATIONS);
    let has_query = !st.query.text.trim().is_empty();
    // 10 over the first header, as [`group_rule`] leaves over the others.
    let mut content = col().key("automations-list").w_full().padding(8.0).pt(10.0);
    let mut first = true;

    if filtered.is_empty() {
        content = content.child(
            row()
                .w_full()
                .px(12.0)
                .py(32.0)
                .justify_center()
                .child(text(t!("automations.empty.filtered")).font(BODY).color(p.faint).wrap()),
        );
    }
    for status in STATUSES {
        let items: Vec<&Entry> = filtered.iter().filter(|e| e.row.status == status).collect();
        if items.is_empty() {
            continue;
        }
        let done = status == AutomationStatus::Done;
        let open = !done || st.show_done || has_query;
        let label = match status {
            AutomationStatus::Active => t!("automations.status.active"),
            AutomationStatus::Paused => t!("automations.status.paused"),
            AutomationStatus::Done => t!("automations.status.done"),
        };
        let mut section = col().key(format!("auto-group-{}", status.as_str())).w_full().gap(2.0);
        // A hairline parts a group from the one above it; the first has the page header's.
        if !std::mem::take(&mut first) {
            content = content.child(group_rule(p));
        }
        section = section.child(group_header(p, label, items.len(), done.then_some(open)));
        if open {
            for entry in items {
                let is_selected = selected.is_some_and(|cur| cur.key == entry.key);
                section = section.child(list_row(s, p, entry, is_selected, context(entry)));
            }
        }
        content = content.child(section);
    }

    session_view::scroller::scroller("automations-list-scroll").build::<App>(p, viewport, content)
}

/// The hairline between two groups, from the spine to the rows' trailing edge: 8 pt under the
/// last pill and 10 over the header, so the name sits nearer its own rows than the line.
fn group_rule(p: &Pal) -> El {
    row().w_full().px(SPINE).pt(8.0).pb(10.0).a11y_hidden().child(el().w_full().h(1.0).fill(p.border_muted))
}

/// A group's name and count on the rows' spine. `fold` is whether a group that folds (Completed)
/// is open: its header is then the button that shows or hides it, a disclosure triangle after
/// the count (the sidebar sections' own glyph and quarter turn) saying which.
fn group_header(p: &Pal, label: &'static str, count: usize, fold: Option<bool>) -> El {
    let head = row()
        .w_full()
        .h(GROUP_H)
        .px(SPINE)
        .gap(6.0)
        .center_y()
        .child(text(label).font(GROUP_LABEL).color(p.muted).group_color(p.text))
        .child(text(count.to_string()).font(GROUP_COUNT).color(p.faint).group_color(p.muted));
    let Some(open) = fold else { return head.role("heading").label(format!("{label} {count}")) };
    let triangle = icon(&ic::FILL_TRIANGLE_DOWN, 12.0)
        .color(p.icon_faint)
        .group_color(p.icon_muted)
        .rotate(if open { 0.0 } else { -std::f32::consts::FRAC_PI_2 })
        .rotate_motion(Anim::INSTANT)
        .mirror_in_rtl();
    head.key("auto-group-done-toggle")
        .role("button")
        .label(format!("{label} {count}"))
        .expanded(open)
        .radius(6.0)
        .focus_inset(p)
        .hover_group()
        .on_press_tx(|s: &mut App, tx: &mut Tx| {
            tx.instant();
            let st = s.kernel.slots.get_mut(&AUTOMATIONS);
            st.show_done = !st.show_done;
        })
        // The glyph's box is mostly air: pulled in, its ink stands a gap from the count.
        .child(el().size(12.0, 12.0).ml(-5.0).center().a11y_hidden().child(triangle))
}

/// The chip at a row's end (view.tsx:AutomationListRow `note`): when an active one next runs,
/// Live while it watches for an event, Failed when a failure paused it, Ended when it timed out.
enum Note {
    Due(String),
    Live,
    Failed,
    Ended,
}

fn note(s: &App, info: &opencode_kit::model::AutomationInfo) -> Option<Note> {
    match (info.status, info.last_outcome) {
        (AutomationStatus::Paused, Some(AutomationOutcome::Failed)) => Some(Note::Failed),
        (AutomationStatus::Done, Some(AutomationOutcome::Expired)) => Some(Note::Ended),
        (AutomationStatus::Active, _) if matches!(info.trigger, AutomationTrigger::Event { .. }) => Some(Note::Live),
        (AutomationStatus::Active, _) => info.next_due_at.map(|at| Note::Due(short_duration(at, *s.now_ms))),
        _ => None,
    }
}

/// `h-6 min-w-10 rounded-full border px-2 gap-1`, 12 px medium tabular text, no fill. Official's
/// Live dot pulses (`motion-safe:animate-pulse`); here it holds still.
fn chip(p: &Pal, note: Note) -> El {
    let (ink, border, label) = match &note {
        Note::Due(text) => (p.muted, p.border, text.clone()),
        Note::Live => (p.v2.success.fg, p.v2.success.border, t!("automations.note.live").to_owned()),
        Note::Failed => (p.v2.danger.fg, p.v2.danger.border, t!("automations.note.failed").to_owned()),
        Note::Ended => (p.muted, p.border, t!("automations.note.ended").to_owned()),
    };
    let lead = match note {
        Note::Due(_) => None,
        Note::Live => Some(el().size(6.0, 6.0).radius(3.0).fill(ink).a11y_hidden()),
        Note::Failed => Some(icon(&ic::WARNING, 12.0).color(ink).a11y_hidden()),
        Note::Ended => Some(icon(&super::CLOCK, 12.0).color(ink).a11y_hidden()),
    };
    row()
        .key("auto-row-note")
        .h(24.0)
        .min_w(40.0)
        .px(8.0)
        .gap(4.0)
        .center()
        .radius(12.0)
        .border(1.0, border)
        .children(lead)
        .child(text(label).font(CHIP).color(ink))
}

fn list_row(s: &App, p: &Pal, entry: &Entry, selected: bool, context: Vec<String>) -> El {
    let info = &entry.row;
    // The trigger leads, then the project when there are several (view.tsx:AutomationListRow).
    let sub = std::iter::once(entry.trigger.clone()).chain(context).collect::<Vec<_>>().join(" · ");
    // A finished automation steps back: its name in the muted ink.
    let name_ink = if info.status == AutomationStatus::Done { p.muted } else { p.text };

    let key = entry.key.clone();
    let lines = col()
        .grow(1.0)
        .shrink()
        .min_w(0.0)
        .gap(2.0)
        .child(text(info.name.clone()).font(ROW_TITLE).color(name_ink).w_full().truncate().dir(Direction::Auto))
        .child(text(sub).font(ROW_SUB).color(p.faint).w_full().truncate());

    row()
        .key(format!("auto-row-{}", info.id))
        .role("button")
        .label(info.name.clone())
        .focus_selected(selected)
        .w_full()
        .px(SPINE)
        .py(8.0)
        .gap(12.0)
        .center_y()
        .radius(7.0)
        .focus_inset(p)
        .fill(if selected { p.layer2 } else { Color::TRANSPARENT })
        .hover(if selected { Color::TRANSPARENT } else { p.hover })
        .on_press_tx(move |s: &mut App, tx: &mut Tx| {
            tx.instant();
            s.kernel.slots.get_mut(&AUTOMATIONS).selected = Some(key.clone());
        })
        .child(lines)
        .children(note(s, info).map(|n| chip(p, n)))
}
