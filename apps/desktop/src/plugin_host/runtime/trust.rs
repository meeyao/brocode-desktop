//! The one-time prompt before a plugin from outside the app first runs (one from
//! `plugins-override/` or `<data>/plugins/`). Its OK is for the plugin's files as they are: their
//! SHA-256 is remembered, and a changed plugin asks again. Built-ins and `--plugin-dir` don't ask.
//! What the user chose is the plugin's record in the app's database (`super::boot::save_record`).

use rune::{El, TextStyle, Tx, col, el, row, spacer, text};

use crate::App;
use crate::palette::Pal;
use crate::plugin_host::{Life, Trust, TrustEvent, step_trust};
use crate::view::{UI, UI_MED};

/// The plugin whose prompt shows now: the first waiting one the user hasn't put off.
fn shown(app: &App) -> Option<usize> {
    app.plugins.list.iter().position(|p| matches!(p.life, Life::AwaitingTrust) && matches!(p.trust, Trust::Unknown { .. }))
}

pub fn dialog(s: &App, p: &Pal, cx: &rune::Cx<App>) -> Vec<El> {
    let Some(i) = shown(s) else { return Vec::new() };
    let plugin = &s.plugins.list[i];
    let (id, name) = (plugin.manifest.id.clone(), plugin.manifest.name.clone());
    let button = |key: &str, label: &str, ink: rune::Color, fill: Option<rune::Color>| {
        let mut b = row().key(key.to_owned()).role("button").label(label.to_owned()).h(28.0).px(12.0).radius(6.0).center().hover(p.hover);
        if let Some(f) = fill {
            b = b.fill(f).hover(f.alpha(0.9));
        }
        b.child(text(label.to_owned()).font(UI_MED).color(ink))
    };
    let mut card = col()
        .key("luau-trust-card")
        .padding(20.0)
        .gap(6.0)
        .radius(12.0)
        .fill(p.bg_base)
        .box_shadow(Pal::ring_out(p.ring))
        .shadow(8.0, 32.0, p.shadow_far)
        .child(text(format!("Enable plugin \"{name}\"?")).font(TextStyle::new(15.0, 600.0).lh(24.0)).color(p.text))
        .child(text(format!("{name} {} can read and change your files, run programs and reach the network. Only enable plugins you trust.", plugin.manifest.version)).font(UI).color(p.muted).wrap());
    if let Some(d) = plugin.manifest.description.as_ref().filter(|d| !d.is_empty()) {
        card = card.child(text(d.clone()).font(UI).color(p.muted).wrap().mt(6.0));
    }
    card = card.child(text(plugin.origin.describe()).font(crate::view::UI_SMALL).color(p.faint).wrap().selectable(true).mt(6.0));
    let (later, ok, esc) = (id.clone(), id.clone(), id);
    card = card
        .role("dialog")
        .label(format!("Enable plugin \"{name}\"?"))
        .modal_tx(false, move |s: &mut App, tx: &mut Tx| answer(s, &esc, TrustEvent::Later, tx))
        .child(
            row()
                .key("luau-trust-footer")
                .gap(8.0)
                .mt(14.0)
                .child(spacer())
                .child(button("luau-trust-later", "Not now", p.text, None).on_press_tx(move |s: &mut App, tx: &mut Tx| answer(s, &later, TrustEvent::Later, tx)))
                .child(button("luau-trust-enable", "Enable", p.on_fill, Some(p.bg_accent)).on_press_tx(move |s: &mut App, tx: &mut Tx| answer(s, &ok, TrustEvent::Accept, tx))),
        );
    // No Desktop counterpart: a confirmation, so the normal size.
    let card = crate::widgets::sized(card, crate::widgets::DialogSize::Normal, crate::widgets::Fit::Content, crate::widgets::dialog_window(cx, *s.zoom));
    vec![
        el().key("luau-trust-scrim").absolute(0.0, 0.0).w_full().h_full().fill(p.dim),
        el().key("luau-trust-modal").absolute(0.0, 0.0).w_full().h_full().center().child(card).raise(),
    ]
}

fn answer(app: &mut App, id: &str, event: TrustEvent, tx: &mut Tx) {
    tx.instant();
    if let Some(i) = app.plugins.find(id) {
        step_trust(app, i, event);
    }
}
