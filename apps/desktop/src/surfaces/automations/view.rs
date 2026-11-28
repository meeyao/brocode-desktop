//! Automations screen: header, searchable status-grouped list, detail inspector, and delete
//! confirmation modal (packages/app/src/automations/{route,view}.tsx, opencode#66393).

use opencode_i18n::t;
use opencode_ui::palette::{Pal, kit};
use opencode_ui::widgets::{BODY, button};
use rune::{Align, El, TextStyle, Tx, col, el, row, spacer, text};
use rune_kit::{ButtonVariant, ControlSize};

use crate::App;

use super::AUTOMATIONS;
use super::source::{self, Entry};
use super::{detail, list};

const HEADER_TITLE: TextStyle = TextStyle::new(14.0, 530.0).lh(20.0).tracking(-0.04);
const SEARCH_TEXT: TextStyle = TextStyle::new(12.0, 440.0).lh(16.0);

/// The list's width beside the inspector (`route.tsx:107`, `minmax(300px, 360px)`).
const LIST_W: f32 = 320.0;
/// The most the list takes over the inspector in a narrow window: a header, four rows and half
/// of the fifth, so the cut falls through a row and shows there is more to scroll to.
const LIST_BAND_H: f32 = 288.0;

pub fn page(s: &App, p: &Pal, panel_sz: [f32; 2]) -> El {
    let all = source::entries(s);
    let filtered = source::filtered_entries(s, &all);
    let selected = source::selected_entry(s, &filtered);
    let target = source::draft_target(s, selected.as_ref());
    let wide = panel_sz[0] == 0.0 || panel_sz[0] >= 860.0;
    // Nothing to search until there is an automation.
    let show_search = !all.is_empty() && (panel_sz[0] == 0.0 || panel_sz[0] >= 640.0);

    let body = if all.is_empty() {
        empty_state(s, p, target.is_some())
    } else {
        split_body(s, p, &all, &filtered, selected.as_ref(), wide)
    };

    col()
        .key("automations-panel")
        .w_full()
        .h_full()
        .min_h(0.0)
        .child(header(s, p, target.is_some(), show_search))
        .child(body)
}

fn header(s: &App, p: &Pal, has_draft: bool, show_search: bool) -> El {
    let st = s.kernel.slots.get(&AUTOMATIONS);
    let k = kit(p);
    let mut right = row().key("automations-header-actions").gap(8.0).center_y();
    if show_search {
        let search = session_view::style::field(&st.query, SEARCH_TEXT)
            .key("automations-search")
            .label_static(t!("automations.search"))
            .single_line()
            .placeholder(t!("automations.search"), p.faint)
            .w(200.0)
            .h(28.0)
            .px(10.0)
            .py(6.0)
            .radius(6.0)
            .fill(p.layer1)
            // The hairline the inspector's cards have: `layer-01` alone is lost on a light base.
            .border(1.0, p.border_muted)
            .color(p.text)
            .caret(p.text, p.selection)
            .selection_style(p.field_selection)
            .focus_ring(1.0, 0.0, p.q.border_strong)
            .on_edit(|s: &mut App| &mut s.kernel.slots.get_mut(&AUTOMATIONS).query);
        right = right.child(search);
    }
    if has_draft {
        let new_btn = button(t!("automations.new"), ButtonVariant::Contrast)
            .control_size(ControlSize::Regular)
            .on_press_tx(|s: &mut App, tx: &mut Tx| super::open_new_automation(s, "/automation ", tx))
            .build(k)
            .key("automations-new");
        right = right.child(new_btn);
    }

    col()
        .key("automations-header")
        .w_full()
        .child(
            row()
                .w_full()
                .h(47.0)
                .px(10.0)
                .gap(8.0)
                .center_y()
                .window_drag()
                .child(crate::view::sidebar::collapsed_slot(s))
                .child(text(t!("automations.title")).key("automations-title").font(HEADER_TITLE).color(p.text).truncate())
                .child(spacer())
                .child(right),
        )
        .child(el().w_full().h(1.0).fill(p.border).a11y_hidden())
}

fn empty_state(s: &App, p: &Pal, has_draft: bool) -> El {
    let loading = source::is_loading(s);
    let failed = source::is_failed(s);
    let heading = if failed {
        t!("automations.failed")
    } else if loading {
        t!("automations.loading")
    } else {
        t!("automations.empty.title")
    };

    let mut inner = col()
        .key("automations-empty")
        .max_w(480.0)
        .px(24.0)
        .gap(12.0)
        .items_center()
        .child(text(heading).font(HEADER_TITLE).color(p.text).align(Align::Center).wrap());

    if has_draft && !loading && !failed {
        let example = t!("automations.example");
        let ex_btn = button(example, ButtonVariant::Neutral)
            .control_size(ControlSize::Regular)
            .on_press_tx(move |s: &mut App, tx: &mut Tx| {
                super::open_new_automation(s, &format!("/automation {example}"), tx);
            })
            .build(kit(p))
            .key("automations-example");
        inner = inner
            .child(text(t!("automations.empty.description")).font(BODY).color(p.muted).align(Align::Center).wrap())
            .child(ex_btn);
    }

    col().w_full().grow(1.0).shrink().min_h(0.0).center().child(inner)
}

fn split_body(s: &App, p: &Pal, all: &[Entry], filtered: &[Entry], selected: Option<&Entry>, wide: bool) -> El {
    // Project names only tell rows apart when they differ (model.ts:context).
    let multi_proj = source::multi_project(all);
    let context = |entry: &Entry| {
        if multi_proj && !entry.project_name.trim().is_empty() { vec![entry.project_name.clone()] } else { Vec::new() }
    };
    let detail = match selected {
        Some(entry) => detail::inspector(s, p, entry, &context(entry)),
        None => el(),
    };
    // Rows and the article fade out under the edge they scroll past, only while something is past it.
    let fade = || rune::ScrollFade::strength(16.0);
    let detail_viewport = col().grow(1.0).shrink().min_w(0.0).min_h(0.0).scroll_fade(fade());
    let split = if wide { row() } else { col() }.key("automations-split").w_full().grow(1.0).shrink().min_h(0.0);
    // The list keeps its width and the inspector takes the rest; in a narrow window the list is a
    // band over the inspector, as tall as its rows up to [`LIST_BAND_H`].
    let (list_viewport, rule, detail_viewport) = if wide {
        (col().w(LIST_W).h_full().min_h(0.0).scroll_fade(fade()), el().w(1.0).h_full(), detail_viewport.h_full())
    } else {
        (col().w_full().max_h(LIST_BAND_H).min_h(0.0).scroll_fade(fade()), el().w_full().h(1.0), detail_viewport.w_full())
    };
    split
        .child(list::pane(s, p, list_viewport, filtered, selected, context))
        .child(rule.fill(p.border).a11y_hidden())
        // The bar's room is kept, so the article doesn't shift when a body opens past the pane.
        .child(session_view::scroller::scroller("automations-detail-scroll").stable_gutter().build::<App>(p, detail_viewport, detail))
}
