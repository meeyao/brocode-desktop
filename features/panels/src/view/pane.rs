//! Single pane frame (`pane_box`): memoized tab bar, active panel/launcher content, and empty state.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use rune::{Color, El, col, el, text};
use opencode_ui::palette::Pal;

use crate::host::PanelsHost;
use crate::keys::closing_side_entry;
use crate::menus::new_tab_view;
use crate::state::{NewTabEntry, ResolvedEntry, ResolvedRegion, bottom_open, live_new_tabs, routed_unlisted, shown_new_tab, side_open};
use crate::tabs::SessionTabs;
use crate::{PanelViewCtx, SessionPane};

use super::strip::{is_pane_focused, tab_bar};

pub(super) fn pane_slug(pane: SessionPane) -> &'static str {
    match pane {
        SessionPane::Side => "side",
        SessionPane::Split => "split",
        SessionPane::Bottom => "bottom",
    }
}

pub(crate) fn pane_box<H: PanelsHost + 'static>(
    host: &H,
    p: &Pal,
    reg: &ResolvedRegion,
    pane: SessionPane,
    pane_size: (f32, f32),
    env_scale: f32,
    reserve_toggle: bool,
) -> El {
    let empty_tabs = SessionTabs::default();
    let stored_state = host.side().sessions.get(&reg.session_id);
    let stored = stored_state.map_or(&empty_tabs, |st| &st.tab_state.tabs);
    let preview_key = stored_state.and_then(|st| st.tab_state.preview.as_deref());
    let held = closing_side_entry(host, reg, pane);
    let pane_active = held.map(|e| e.key.clone()).or_else(|| reg.pane_active(pane, stored));
    let shown_new = shown_new_tab(host, &reg.session_id, pane, pane_active.as_deref());
    let live_new = live_new_tabs(host, &reg.session_id, pane, pane_active.as_deref());
    let mut entries = reg.pane_entries(pane);
    entries.extend(held);

    let reserve_toggle = reserve_toggle && !host.stage_active();
    let bar_hash = compute_bar_hash(
        host,
        p,
        pane,
        &reg.session_id,
        pane_size.0,
        &entries,
        pane_active.as_deref(),
        preview_key,
        shown_new,
        &live_new,
        reserve_toggle,
    );
    let bar = rune::memo(format!("side-tabbar-{}", pane_slug(pane)), bar_hash, || {
        tab_bar(
            host,
            p,
            pane,
            (&reg.session_id, pane_size.0),
            &entries,
            pane_active.as_deref(),
            preview_key,
            shown_new.map(|n| n.id.as_str()),
            &live_new,
            reserve_toggle,
        )
    });

    let content = if shown_new.is_some() {
        new_tab_view(host, p, reg, pane, pane_size.0)
    } else if let Some(selected) = held.or_else(|| pane_active.as_deref().and_then(|k| reg.entry(k))) {
        render_selected_panel(host, p, reg, pane, pane_size, env_scale, selected)
    } else if !stored.all.is_empty() && routed_unlisted(host) {
        // Desktop lists a restored tab hidden while its plugin loads the session's store (`btw/renderer.tsx`):
        // no tab is drawn and the pane stays blank; the placeholder is for a strip that stores no tab.
        col().key("side-pending").w_full().grow(1.0).min_h(0.0)
    } else {
        empty_pane_view(p)
    };

    col()
        .key(format!("pane-{}", pane_slug(pane)))
        .w_full()
        .min_w(0.0)
        .min_h(0.0)
        .on_background_press(move |s: &mut H| focus_pane_background(s, pane))
        .child(bar)
        // `tabs.css:589`: `border-bottom: 0.5px solid border-base`, a device pixel of room.
        .child(rune::rule(0.5, p.border))
        .child(content)
}

#[allow(clippy::too_many_arguments)]
fn compute_bar_hash(
    host: &dyn PanelsHost,
    p: &Pal,
    pane: SessionPane,
    session_id: &str,
    pane_w: f32,
    entries: &[&ResolvedEntry],
    pane_active: Option<&str>,
    preview_key: Option<&str>,
    shown_new: Option<&NewTabEntry>,
    live_new: &[&NewTabEntry],
    reserve_toggle: bool,
) -> u64 {
    let mut h = DefaultHasher::new();
    let focused = is_pane_focused(host, pane, entries, pane_active);
    (
        (
            (p.theme, p.generation, p.dark),
            opencode_i18n::locale().id(),
            opencode_ui::keymap::overrides::version(),
            pane as u8,
            session_id,
            pane_w.to_bits(),
            pane_active,
        ),
        (
            preview_key,
            shown_new.map(|n| n.id.as_str()),
            (reserve_toggle, super::toggle_in_strip(host)),
            focused,
            host.side().pane_drag.dragging(),
            entries.len(),
            live_new.len(),
            host.side().width_holds[pane as usize]
                .active(session_id, pane_w, entries.len() + live_new.len())
                .to_bits(),
        ),
    )
        .hash(&mut h);
    for e in entries {
        let active = shown_new.is_none() && pane_active == Some(e.key.as_str());
        (
            (
                &e.key,
                e.plugin,
                &e.tab.id,
                &e.tab.title,
                e.tab.icon.map(|ic| ic.name),
                &e.tab.file,
                &e.tab.badge,
            ),
            (
                e.tab.pinned,
                e.tab.first,
                e.tab.closable as u8,
                e.tab.missing,
                e.tab.transient,
                e.tab.draggable,
                e.provider.lead_stamp(host, p, &e.tab, active && focused),
            ),
        )
            .hash(&mut h);
    }
    for nt in live_new {
        nt.id.hash(&mut h);
    }
    h.finish()
}

fn render_selected_panel(
    host: &dyn PanelsHost,
    p: &Pal,
    reg: &ResolvedRegion,
    pane: SessionPane,
    pane_size: (f32, f32),
    env_scale: f32,
    selected: &ResolvedEntry,
) -> El {
    let visible = match pane {
        SessionPane::Bottom => bottom_open(host),
        _ => side_open(host) || host.stage_active(),
    };
    let bar_h = 48.5;
    let side = host.side();
    let vctx = PanelViewCtx {
        plugin: selected.plugin,
        session_id: &reg.view_session,
        directory: &reg.directory,
        project_id: &reg.project_id,
        pane,
        pane_size: (pane_size.0, (pane_size.1 - bar_h).max(1.0)),
        env_scale,
        tab: &selected.tab,
        visible,
        focused: side.focus.is_focused() && side.active_pane_focus == pane,
        mobile: false,
    };
    let panel_el = selected.provider.render(host, p, &vctx);
    let content_key = match selected.tab.group {
        Some(g) => format!("panel-{}-{g}", selected.plugin),
        None => format!("panel-{}", selected.key),
    };
    col()
        .key(content_key)
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .child(panel_el)
}

/// A pane that shows nothing (`side-region.tsx`'s empty state): OpenCode's mark, 56 wide at 10%,
/// over "Select a file to open" in `text-weak`, raised a little off centre.
fn empty_pane_view(p: &Pal) -> El {
    // `Mark` on its 16 by 20 grid at 3.5x: the O in `icon-strong-base`, its shadow in `icon-weak-base`.
    let u = 3.5;
    let bar = |x: f32, y: f32, w: f32, h: f32, c: Color| el().absolute(x * u, y * u).size(w * u, h * u).fill(c);
    let mark = el()
        .key("side-empty-mark")
        .size(16.0 * u, 20.0 * u)
        .opacity(0.1)
        .child(bar(4.0, 8.0, 8.0, 8.0, p.v1_icon_weak))
        .child(bar(0.0, 0.0, 16.0, 4.0, p.v1_icon_strong))
        .child(bar(0.0, 16.0, 16.0, 4.0, p.v1_icon_strong))
        .child(bar(0.0, 4.0, 4.0, 12.0, p.v1_icon_strong))
        .child(bar(12.0, 4.0, 4.0, 12.0, p.v1_icon_strong));
    col()
        .key("side-empty")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .center()
        .px(24.0)
        // `pt-2`, `-mt-4` and `pb-42` centre the group at (height - 192) / 2.
        .pb(8.0 + 16.0 + 168.0)
        .gap(24.0)
        .child(mark)
        // Desktop's line box measures 25.2 (14 at 1.8).
        .child(
            text(opencode_i18n::t!("session.files.selectToOpen"))
                .font(rune::TextStyle::new(14.0, 400.0).lh(25.2))
                .color(p.v1_text_weak)
                .max_w(224.0),
        )
}

fn focus_pane_background(host: &mut dyn PanelsHost, pane: SessionPane) {
    {
        let side = host.side_mut();
        side.focus.press();
        side.active_pane_focus = pane;
    }
    host.set_composer_focused(false);
}
