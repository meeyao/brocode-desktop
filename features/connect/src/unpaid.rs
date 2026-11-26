//! Unpaid free-model picker (`packages/app/src/providers/models/unpaid.tsx`, `#64927`).
//!
//! Shown instead of the full model dialog when the user has no paid provider connected and at
//! least one free OpenCode model is available: lists free models above a card grid of popular
//! providers (OpenCode Go, OpenCode Console, OpenAI, Anthropic, Google, GitHub Copilot) and a
//! link to view all providers in the Connect dialog.

use opencode_ui::focus::FocusExt;
use opencode_i18n::t;
use opencode_kit::model::Model;
use opencode_ui::dialog::DialogSize;
use opencode_ui::palette::{Elevate as _, Pal};
use rune::{Color, Cx, El, Key, KeyEvent, Spring, TextStyle, Transition, Tx, col, el, icon, row, spacer, text};
use rune_theme_opencode::icons as ic;

use crate::catalog;
use crate::ui::{self, BODY, BODY_MED, DIALOG_CLOSE, Fit};
use crate::{ConnectHost, Modal, ModalEvent};

/// Header title (`dialog.tsx` / `unpaid.tsx:87`): `15px`, `530`, `-0.13px`.
const TITLE: TextStyle = TextStyle::new(15.0, 530.0).lh(16.0).tracking(-0.13);

/// Featured provider order (`unpaid.tsx:19`): OpenCode, then top providers. Go connects through
/// the OpenCode connection, so it is not featured (`#65466`).
const FEATURED: [(&str, &str); 5] = [
    ("opencode", "Brocode"),
    ("openai", "OpenAI"),
    ("anthropic", "Anthropic"),
    ("google", "Google"),
    ("github-copilot", "GitHub Copilot"),
];

enum Target {
    Model { provider: String, id: String },
    Provider(String),
    AllProviders,
}

/// Strips trailing `(free)` / `free` suffix (`unpaid.tsx:21`).
fn display_model_name(name: &str) -> String {
    let trimmed = name.trim_end();
    let lower = trimmed.to_ascii_lowercase();
    for suffix in [" (free)", " free"] {
        if lower.ends_with(suffix) {
            return trimmed[..trimmed.len() - suffix.len()].trim_end().to_owned();
        }
    }
    trimmed.to_owned()
}

fn featured_providers(host: &dyn ConnectHost) -> Vec<(String, String)> {
    let dir = host.active_directory();
    let dir = (!dir.is_empty()).then_some(dir.as_str());
    let listed = catalog::integrations(host, dir);
    let mut out = Vec::new();
    for (id, fallback) in FEATURED {
        let name = match listed {
            Some(list) => match list.iter().find(|i| i.id == id) {
                Some(i) => {
                    if id == "opencode" {
                        t!("provider.connect.opencode.name").to_owned()
                    } else {
                        i.name.clone()
                    }
                }
                None => continue,
            },
            None => {
                if id == "opencode" {
                    t!("provider.connect.opencode.name").to_owned()
                } else {
                    fallback.to_owned()
                }
            }
        };
        out.push((id.to_owned(), name));
    }
    out
}

fn free_models(host: &dyn ConnectHost) -> Vec<Model> {
    host.models().iter().filter(|m| m.free).cloned().collect()
}

fn targets(host: &dyn ConnectHost) -> Vec<Target> {
    let mut out: Vec<Target> = free_models(host)
        .into_iter()
        .map(|m| Target::Model { provider: m.provider, id: m.id })
        .collect();
    for (id, _) in featured_providers(host) {
        out.push(Target::Provider(id));
    }
    out.push(Target::AllProviders);
    out
}

/// Opens the unpaid free-model picker modal.
pub fn open(host: &mut dyn ConnectHost, tx: &mut Tx) {
    tx.instant();
    host.before_dialog_open();
    let dir = host.active_directory();
    let dir = (!dir.is_empty()).then_some(dir);
    catalog::sync_catalogs(host, dir);
    crate::apply_modal(host, ModalEvent::OpenUnpaid, tx);
}

/// Opens the unpaid free-model picker directly for scenes/previews.
pub fn preview(host: &mut dyn ConnectHost) {
    host.connect_mut().modal = Modal::Unpaid { active: None };
}

/// Closes the unpaid free-model picker modal and restores composer focus.
pub fn close(host: &mut dyn ConnectHost, tx: &mut Tx) {
    if !host.connect().is_unpaid_open() {
        return;
    }
    tx.instant();
    crate::apply_modal(host, ModalEvent::CloseUnpaid, tx);
}

fn activate(host: &mut dyn ConnectHost, target: Target, tx: &mut Tx) {
    match target {
        Target::Model { provider, id } => {
            host.select_free_model(&provider, &id, tx);
            close(host, tx);
        }
        Target::Provider(id) => crate::open(host, Some(&id), false, tx),
        Target::AllProviders => crate::open(host, None, false, tx),
    }
}

/// Esc closes; Up/Down cycle through free models and provider cards; Enter activates.
pub fn key(host: &mut dyn ConnectHost, ev: &KeyEvent, tx: &mut Tx) -> bool {
    let Some(active) = host.connect().unpaid_active() else {
        return false;
    };
    let list = targets(host);
    match ev.key {
        Key::Escape => close(host, tx),
        Key::Down | Key::Up if !list.is_empty() => {
            let n = list.len();
            let next = match (active, ev.key == Key::Down) {
                (None, true) => 0,
                (None, false) => n - 1,
                (Some(i), true) => (i + 1) % n,
                (Some(i), false) => (i + n - 1) % n,
            };
            tx.instant();
            if let Modal::Unpaid { active } = &mut host.connect_mut().modal {
                *active = Some(next);
            }
        }
        Key::Enter => {
            if let Some(target) = active.and_then(|i| list.into_iter().nth(i)) {
                activate(host, target, tx);
            }
        }
        _ => {}
    }
    true
}

/// Modal overlay layer for the unpaid free-model picker (`DialogSize::Large`, content fit).
pub fn layer<H: ConnectHost + 'static>(s: &H, p: &Pal, cx: &Cx<H>) -> Vec<El> {
    if !s.connect().is_unpaid_open() {
        return Vec::new();
    }
    let win = ui::dialog_window(s, cx);
    dialog(s, p, win)
}

pub fn dialog<H: ConnectHost + 'static>(s: &H, p: &Pal, win: [f32; 2]) -> Vec<El> {
    let Some(active) = s.connect().unpaid_active() else {
        return Vec::new();
    };
    let models = free_models(s);
    let providers = featured_providers(s);
    let current_key = s.current_model_key();
    let two_col = win[0] >= 640.0;

    let close_btn = el()
        .key("unpaid-models-close")
        .role("button")
        .label_static(t!("common.close"))
        .size(20.0, 20.0)
        .radius(4.0)
        .center()
        .hover(p.hover)
        .press(p.pressed)
        .on_press_tx(|s: &mut H, tx: &mut Tx| close(s, tx))
        .child(icon(&DIALOG_CLOSE, 16.0).color(p.icon_muted));

    // `unpaid.tsx:87` (`#64927`): `px-5 pb-1` header with 15px 530 title.
    let header = row()
        .key("unpaid-models-header")
        .w_full()
        .pt(16.0)
        .px(20.0)
        .pb(4.0)
        .center_y()
        .child(text(t!("dialog.model.select.title")).font(TITLE).color(p.text).grow(1.0).shrink())
        .child(close_btn);

    let mut free_section = col()
        .key("unpaid-free-models")
        .w_full()
        .pb(12.0)
        .child(
            row()
                .w_full()
                .h(32.0)
                .px(12.0)
                .pb(8.0)
                .center_y()
                .child(text(t!("dialog.model.unpaid.freeModels.title")).font(BODY.features("\"tnum\"")).color(p.muted)),
        );

    for (idx, m) in models.iter().enumerate() {
        let display = display_model_name(&m.name);
        let is_current = current_key.as_deref() == Some(m.key.as_str());
        let is_active = active == Some(idx);
        let (provider, id) = (m.provider.clone(), m.id.clone());
        let mut r = row()
            .key(format!("unpaid-model-{}", m.key))
            .role("button")
            .label(display.clone())
            .w_full()
            .px(12.0)
            .py(8.0)
            .gap(6.0)
            .center_y()
            .radius(6.0)
            .fill(if is_active { p.hover } else { Color::TRANSPARENT })
            .hover(p.hover)
            .press(p.pressed)
            .focus_inset(p)
            .on_press_tx(move |s: &mut H, tx: &mut Tx| {
                s.select_free_model(&provider, &id, tx);
                close(s, tx);
            })
            .child(text(display).font(BODY_MED).color(p.text).shrink().min_w(0.0).truncate())
            .child(ui::badge(p, t!("model.tag.free")));
        if m.latest {
            r = r.child(ui::badge(p, t!("model.tag.latest")));
        }
        if is_current {
            r = r.child(spacer()).child(icon(&ic::CHECK_SMALL, 16.0).color(p.icon));
        }
        free_section = free_section.child(r);
    }

    // `unpaid.tsx:138-190` (`#64927`): light uses `layer2` container + elevated `bg_base` cards;
    // dark uses `layer1` container + `bg_base` cards with `inset 0 0 0 0.5px border-strong`.
    let card_index_base = models.len();
    let provider_card = |idx: usize, id: String, name: String| -> El {
        let is_active = active == Some(card_index_base + idx);
        let note = (id == "opencode").then(|| t!("dialog.provider.opencode.note"));
        let pick_id = id.clone();
        let mut label_col = col()
            .grow(1.0)
            .shrink()
            .min_w(0.0)
            .child(text(name.clone()).font(BODY_MED).color(p.text).truncate());
        if let Some(tagline) = note {
            label_col = label_col.child(text(tagline).font(BODY).color(p.muted).truncate());
        }
        let card = row()
            .key(format!("unpaid-provider-{id}"))
            .role("button")
            .label(name)
            .w_full()
            .min_h(44.0)
            .px(12.0)
            .py(10.0)
            .gap(8.0)
            .radius(6.0)
            .fill(if is_active { p.layer1 } else { p.bg_base })
            .hover(p.layer1)
            .press(p.pressed)
            .focus_inset(p)
            .on_press_tx(move |s: &mut H, tx: &mut Tx| crate::open(s, Some(&pick_id), false, tx))
            .child(el().mt(2.0).child(ui::provider_mark(&id, p.icon)))
            .child(label_col);
        if p.dark {
            card.box_shadow(Pal::ring_in(p.q.border_strong))
        } else {
            card.elevate(p.raised)
        }
    };

    let mut grid = col().w_full().gap(6.0);
    if two_col {
        for chunk in providers.chunks(2).enumerate() {
            let (row_idx, pair) = chunk;
            let mut r = row().w_full().gap(8.0);
            for (col_idx, (id, name)) in pair.iter().enumerate() {
                r = r.child(
                    col()
                        .grow(1.0)
                        .shrink()
                        .min_w(0.0)
                        .child(provider_card(row_idx * 2 + col_idx, id.clone(), name.clone())),
                );
            }
            if pair.len() == 1 {
                r = r.child(col().grow(1.0).shrink().min_w(0.0));
            }
            grid = grid.child(r);
        }
    } else {
        for (idx, (id, name)) in providers.iter().enumerate() {
            grid = grid.child(provider_card(idx, id.clone(), name.clone()));
        }
    }

    let more_idx = card_index_base + providers.len();
    let more_btn = row()
        .key("unpaid-view-more-providers")
        .role("button")
        .label_static(t!("dialog.model.unpaid.viewMoreProviders"))
        .w_full()
        .h(32.0)
        .px(12.0)
        .center_y()
        .radius(6.0)
        .fill(if active == Some(more_idx) { p.hover } else { Color::TRANSPARENT })
        .hover(p.hover)
        .press(p.pressed)
        .focus_inset(p)
        .on_press_tx(|s: &mut H, tx: &mut Tx| crate::open(s, None, false, tx))
        .child(text(t!("dialog.model.unpaid.viewMoreProviders")).font(BODY).color(p.muted));

    let add_more = col()
        .key("unpaid-add-more")
        .w_full()
        .radius(8.0)
        .fill(if p.dark { p.layer1 } else { p.layer2 })
        .box_shadow(Pal::ring_in(p.border_muted))
        .px(10.0)
        .pt(8.0)
        .pb(10.0)
        .child(
            row()
                .w_full()
                .h(32.0)
                .px(2.0)
                .pb(8.0)
                .center_y()
                .child(text(t!("dialog.model.unpaid.addMore.title")).font(BODY.features("\"tnum\"")).color(p.muted)),
        )
        .child(grid.child(more_btn));

    let body_content = col().w_full().px(8.0).pb(8.0).child(free_section).child(add_more);
    let body = opencode_ui::scroller::scroller("unpaid-models-scroll").build::<H>(
        p,
        col().w_full().shrink().min_h(0.0),
        body_content,
    );

    let surface = ui::overlay(p, col())
        .key("unpaid-models-dialog")
        .role("dialog")
        .label_static(t!("dialog.model.select.title"))
        .radius(12.0)
        .fill(p.bg_base)
        .clip_soft(0.0)
        .transition(Transition::none())
        .motion(Spring::NONE)
        .child(header)
        .child(body)
        .modal(true, |s: &mut H| close(s, &mut Tx::default()));

    ui::sized_modal("unpaid-models", p, DialogSize::Large, Fit::Content, win, surface)
}
