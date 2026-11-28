//! Side region `"New tab"` launcher cards, `Recently closed` list, and tab/`+` context menus.

use opencode_ui::keymap::{self, Bind};
use rune::{Align, Anim, El, Icon, PointerPress, ScrollSpec, Side, Tx, col, el, icon, row, text};
use opencode_ui::palette::Pal;
use opencode_ui::popover::{self, MENU, Opt};
use opencode_ui::style::{UI, UI_SMALL};

use crate::host::PanelsHost;
use crate::state::{NewTabFocus, ResolvedRegion, SideMenu, close_side_tab, release_strip_width_holds, reopen_closed_side_tab, resolve_side_region};
use crate::tabs::{ClosedSideTab, split_panel_key};
use crate::{Launch, PanelLauncherDesc, SessionPane};

const INSTANT: Anim = Anim::INSTANT;

pub fn new_tab_view<H: PanelsHost + 'static>(host: &H, p: &Pal, reg: &ResolvedRegion, pane: SessionPane, pane_w: f32) -> El {
    let focus_token = match host.side().new_tab_focus {
        NewTabFocus::Requested { pane: fp, token } if fp == pane && !host.side().edge_flight.is_flying() => Some(token),
        _ => None,
    };
    let top = host.panel_new_tab_slots(p, "session.panel.new", &reg.session_id, pane, pane_w, focus_token);
    let extra_list = host.panel_new_tab_slots(p, "session.panel.new.list", &reg.session_id, pane, pane_w, None);

    // `Launchers`: rows 12 apart in a 560 column, 48 from the strip (or browser bar).
    let mut list = col().key("side-launchers").w_full().max_w(560.0).pt(48.0).pb(40.0).gap(12.0);

    for &(plugin, ref desc, launch) in &reg.launchers {
        if desc.opened {
            continue;
        }
        let id = desc.id;
        let card = launcher_card_shell(host, p, plugin, desc)
            .on_press_tx(move |s: &mut H, tx: &mut Tx| on_launch_card_press(s, launch, id, pane, tx))
            .on_hover_change(move |s: &mut H, inside, tx: &mut Tx| on_launch_hover(s, launch, id, inside, tx));
        list = list.child(card);
    }

    for extra in extra_list {
        list = list.child(extra);
    }

    let closed_items = visible_closed_items(host, reg);
    if !closed_items.is_empty() {
        let mut recent = col().key("side-recently-closed").w_full().pt(16.0).gap(2.0).child(
            el().px(8.0)
                .pb(4.0)
                .child(text(opencode_i18n::t!("home.recentlyClosed")).font(UI_SMALL).color(p.muted)),
        );
        for (i, item) in closed_items.into_iter().enumerate() {
            let reopen_key = item.key.clone();
            let r = closed_row_shell(host, p, i, item)
                .on_press_tx(move |s: &mut H, tx: &mut Tx| on_reopen_closed(s, &reopen_key, tx));
            recent = recent.child(r);
        }
        list = list.child(recent);
    }

    let has_top = !top.is_empty();
    let scroller = col()
        .key("side-new-tab-scroll")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .px(24.0)
        .pt(if has_top { 44.0 } else { 0.0 })
        .scroll(ScrollSpec { stick_bottom: false })
        .child(row().w_full().center().child(list));

    let mut view = col()
        .key("side-new-tab-view")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .child(scroller);
    if has_top {
        // Pinned as the last child so the top browser bar stays fixed above scrolled rows and its
        // omnibox suggestions dropdown paints and hit-tests above the launchers (`side-region.tsx:636`).
        view = view.child(
            col()
                .key("side-new-tab-top")
                .pin(Some(0.0), Some(0.0), Some(0.0), None)
                .children(top),
        );
    }
    view
}

fn launcher_card_shell(host: &dyn PanelsHost, p: &Pal, plugin: &'static str, desc: &PanelLauncherDesc) -> El {
    let mut left = row()
        .gap(8.0)
        .center_y()
        .grow(1.0)
        .min_w(0.0)
        .child(icon(desc.icon, 16.0).color(p.icon_muted))
        .child(text(desc.title.clone()).font(UI).color(p.text));
    if let Some(detail) = &desc.description {
        left = left.child(text(detail.clone()).font(UI).color(p.muted).truncate_fade().shrink());
    }

    // `LauncherRow`: 41 tall, 8 corners, a 0.5 `border-base` edge on `layer-01`.
    let mut card = row()
        .key(format!("side-launch-{plugin}-{}", desc.id))
        .role("button")
        .label(desc.title.clone())
        .w_full()
        .h(41.0)
        .px(14.0)
        .gap(16.0)
        .center_y()
        .radius(8.0)
        .fill(p.layer1)
        .border(0.5, p.border)
        .border_layout()
        .hover(p.layer2)
        .hover_motion(INSTANT, INSTANT)
        .child(left);
    // Its shortcut as Desktop's neutral `Keybind` chips.
    let caps = desc.keycaps(host);
    if !caps.is_empty() {
        card = card.child(keymap::chips(p, caps));
    }
    card
}

fn visible_closed_items<'a>(host: &'a dyn PanelsHost, reg: &ResolvedRegion) -> Vec<&'a ClosedSideTab> {
    let open_keys: std::collections::HashSet<&str> = reg.strip.iter().map(|e| e.key.as_str()).collect();
    host.side()
        .sessions
        .get(&reg.session_id)
        .map(|st| {
            st.closed
                .iter()
                .filter(|c| !open_keys.contains(c.key.as_str()))
                .take(6)
                .collect()
        })
        .unwrap_or_default()
}

fn closed_row_shell(host: &dyn PanelsHost, p: &Pal, i: usize, item: &ClosedSideTab) -> El {
    let glyph = host.resolve_closed_icon(&item.key, item.icon_name.as_deref());
    let mut label_row = row()
        .gap(6.0)
        .center_y()
        .grow(1.0)
        .min_w(0.0)
        .child(icon(glyph, 14.0).color(p.icon_muted))
        .child(text(item.title.clone()).font(UI).color(p.text).truncate_fade());
    if let Some(det) = item.detail.as_deref().or(item.file.as_deref()) {
        label_row = label_row.child(
            text(det.to_owned())
                .font(UI_SMALL)
                .color(p.faint)
                .truncate_fade()
                .shrink(),
        );
    }
    let mut r = row()
        .key(format!("side-closed-{i}"))
        .role("button")
        .label(opencode_i18n::t!("rune.tabs.reopen", title = item.title))
        .w_full()
        .h(30.0)
        .px(8.0)
        .gap(8.0)
        .center_y()
        .radius(6.0)
        .hover(p.hover)
        .press(p.pressed)
        .hover_motion(INSTANT, INSTANT)
        .child(label_row);
    if i == 0 {
        r = r.child(text(Bind::TabReopen.label()).font(UI_SMALL).color(p.faint));
    }
    r
}

/// The side panel's tab and `+` menus, drawn on every platform with the one popover every menu
/// is built from (`popover::MENU`), at the right-click.
pub fn context_menus<H: PanelsHost + 'static>(host: &H, p: &Pal) -> Option<El> {
    // Desktop's strip menu (`side-region.tsx:375-401`): Close tab, Close other tabs and Close tabs
    // to the right, each disabled when there's nothing it closes; tabs move between panes by
    // dragging.
    match &host.side().menu {
        SideMenu::Closed => None,
        SideMenu::Tab { key: target_key, x, y } => {
            let (tk, x, y) = (target_key.clone(), *x, *y);
            let meta = tab_menu_meta(host, target_key);
            let has_others = !meta.others.is_empty();
            let has_after = !meta.after.is_empty();
            let others = meta.others;
            let after = meta.after;

            let mut rows = col().key("side-tab-menu-rows").w_full().padding(MENU.inset);
            // A browser tab's page: open it in the default browser, or copy its address.
            if let Some((id, url)) = meta.browser {
                let copy = url.clone();
                rows = rows
                    .child(menu_row::<H>(
                        p,
                        "side-tab-menu-open-out",
                        None,
                        opencode_i18n::t!("plugin.browser.menu.openPage"),
                        "",
                        url.is_some(),
                        move |s: &mut H, tx: &mut Tx| on_tab_menu_open_out(s, id, tx),
                    ))
                    .child(menu_row::<H>(
                        p,
                        "side-tab-menu-copy-url",
                        None,
                        opencode_i18n::t!("plugin.browser.link.copy"),
                        "",
                        copy.is_some(),
                        move |s: &mut H, tx: &mut Tx| on_tab_menu_copy_url(s, copy.as_deref(), tx),
                    ))
                    .child(col().w_full().py(4.0).child(popover::divider(p, "side-tab-menu-divider")));
            }
            let rows = rows
                .child(menu_row::<H>(
                    p,
                    "side-tab-menu-close",
                    None,
                    opencode_i18n::t!("common.closeTab"),
                    "",
                    meta.can_close,
                    move |s: &mut H, tx: &mut Tx| on_tab_menu_close(s, &tk, tx),
                ))
                .child(menu_row::<H>(
                    p,
                    "side-tab-menu-close-others",
                    None,
                    opencode_i18n::t!("session.tab.closeOthers"),
                    "",
                    has_others,
                    move |s: &mut H, tx: &mut Tx| on_tab_menu_close_list(s, &others, tx),
                ))
                .child(menu_row::<H>(
                    p,
                    "side-tab-menu-close-after",
                    None,
                    meta.after_label,
                    "",
                    has_after,
                    move |s: &mut H, tx: &mut Tx| on_tab_menu_close_list(s, &after, tx),
                ));
            let menu = popover::surface::<H>(p, MENU, "side-tab-menu", opencode_i18n::t!("common.key.tab"))
                .min_w(160.0)
                .child(rows);
            Some(layer(host, (x, y), menu, clear_menu))
        }
        SideMenu::Add { pane, x, y } => {
            let (pane, x, y) = (*pane, *x, *y);
            let reg = resolve_side_region(host)?;
            let mut rows = col().key("side-add-menu-rows").w_full().padding(MENU.inset);
            for &(_pl, ref desc, launch) in &reg.launchers {
                let key = format!("side-add-menu-{}", desc.id);
                let id = desc.id;
                let hint = launcher_hint(host, desc);
                let row = menu_row::<H>(
                    p,
                    &key,
                    Some(desc.icon),
                    &desc.title,
                    &hint,
                    true,
                    move |s: &mut H, tx: &mut Tx| on_add_menu_pick(s, launch, id, pane, tx),
                );
                rows = rows.child(row.on_hover_change(move |s: &mut H, inside, tx: &mut Tx| {
                    on_launch_hover(s, launch, id, inside, tx);
                }));
            }
            let menu = popover::surface::<H>(p, MENU, "side-add-menu", opencode_i18n::t!("session.tab.new"))
                .w(220.0)
                .child(rows);
            Some(layer(host, (x, y), menu, clear_menu))
        }
    }
}

struct TabMenuMeta {
    can_close: bool,
    others: Vec<String>,
    after: Vec<String>,
    after_label: &'static str,
    browser: Option<(u64, Option<String>)>,
}

fn tab_menu_meta(host: &dyn PanelsHost, target_key: &str) -> TabMenuMeta {
    let reg = resolve_side_region(host);
    let entry = reg.as_ref().and_then(|r| r.entry(target_key));
    let can_close = entry.is_some_and(|e| e.tab.can_close());
    let pane = entry.map_or(SessionPane::Side, |e| e.pane);
    let others: Vec<String> = reg
        .as_ref()
        .map(|r| {
            r.pane_entries(pane)
                .into_iter()
                .filter(|e| e.key != *target_key && e.tab.can_close())
                .map(|e| e.key.clone())
                .collect()
        })
        .unwrap_or_default();
    // Close tabs to the right (left, right to left) of the target, in its pane (`after`).
    let after: Vec<String> = reg
        .as_ref()
        .map(|r| {
            r.pane_entries(pane)
                .into_iter()
                .skip_while(|e| e.key != *target_key)
                .skip(1)
                .filter(|e| e.tab.can_close())
                .map(|e| e.key.clone())
                .collect()
        })
        .unwrap_or_default();
    let after_label = if opencode_i18n::direction() == opencode_i18n::Direction::Rtl {
        opencode_i18n::t!("common.closeTabsToLeft")
    } else {
        opencode_i18n::t!("common.closeTabsToRight")
    };
    let browser = split_panel_key(target_key)
        .filter(|(plugin, _)| *plugin == "browser")
        .and_then(|(_, tid)| tid.parse::<u64>().ok())
        .map(|id| (id, host.browser_tab_external_url(id)));
    TabMenuMeta { can_close, others, after, after_label, browser }
}

fn launcher_hint(host: &dyn PanelsHost, desc: &PanelLauncherDesc) -> String {
    match keymap::first_label(desc.shortcut) {
        l if l.is_empty() => desc.command.map(|c| keymap::sequence::label(&host.command_strokes(c))).unwrap_or_default(),
        l => l,
    }
}

/// `menu` at the point (`x`, `y`), kept inside the window, over a layer whose press closes it.
fn layer<H: PanelsHost + 'static>(host: &H, (x, y): (f32, f32), menu: El, close: fn(&mut dyn PanelsHost)) -> El {
    el().key("side-menu-backdrop")
        .pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
        .on_background_press_tx(move |s: &mut H, tx: &mut Tx| close_menu_layer(s, close, tx))
        .on_secondary_press(move |s: &mut H, _: PointerPress, tx: &mut Tx| close_menu_layer(s, close, tx))
        .child(el().key("side-menu-point").absolute(x, y).size(0.0, 0.0))
        .child(popover::place(host.knobs(), menu, "side-menu-point", Side::Bottom, Align::Start, 2.0))
}

/// A menu row as the shared popover draws one: lit while the pointer is on it.
#[allow(clippy::too_many_arguments)]
fn menu_row<H: PanelsHost + 'static>(
    p: &Pal,
    key: &str,
    glyph: Option<&'static Icon>,
    label: &str,
    hint: &str,
    enabled: bool,
    pick: impl Fn(&mut H, &mut Tx) + 'static,
) -> El {
    let mut o = Opt::new(key.to_owned(), label.to_owned()).role("menuitem").disabled(!enabled);
    if let Some(glyph) = glyph {
        o = o.lead(icon(glyph, popover::SLOT).color(p.icon_muted));
    }
    if !hint.is_empty() {
        o = o.trailing(popover::hint(p, hint));
    }
    popover::option::<H>(p, MENU, o, |_| {}, pick)
        .hover(p.row_active)
        .hover_motion(INSTANT, INSTANT)
}

fn on_launch_card_press(host: &mut dyn PanelsHost, launch: Launch, id: &str, pane: SessionPane, tx: &mut Tx) {
    tx.instant();
    launch.run(host, id, pane, tx);
}

fn on_launch_hover(host: &mut dyn PanelsHost, launch: Launch, id: &str, inside: bool, tx: &mut Tx) {
    if inside {
        launch.hover(host, id, tx);
    }
}

fn on_reopen_closed(host: &mut dyn PanelsHost, key: &str, tx: &mut Tx) {
    let _ = reopen_closed_side_tab(host, Some(key), tx);
}

fn on_tab_menu_open_out(host: &mut dyn PanelsHost, id: u64, tx: &mut Tx) {
    tx.instant();
    host.side_mut().menu = SideMenu::Closed;
    host.open_browser_tab_externally(id, tx);
}

fn on_tab_menu_copy_url(host: &mut dyn PanelsHost, url: Option<&str>, tx: &mut Tx) {
    tx.instant();
    host.side_mut().menu = SideMenu::Closed;
    if let Some(url) = url {
        tx.copy(url.to_owned());
    }
}

fn on_tab_menu_close(host: &mut dyn PanelsHost, key: &str, tx: &mut Tx) {
    release_strip_width_holds(host);
    host.side_mut().menu = SideMenu::Closed;
    close_side_tab(host, key, tx);
}

fn on_tab_menu_close_list(host: &mut dyn PanelsHost, keys: &[String], tx: &mut Tx) {
    tx.instant();
    release_strip_width_holds(host);
    host.side_mut().menu = SideMenu::Closed;
    for k in keys {
        close_side_tab(host, k, tx);
    }
}

fn on_add_menu_pick(host: &mut dyn PanelsHost, launch: Launch, id: &str, pane: SessionPane, tx: &mut Tx) {
    tx.instant();
    host.side_mut().menu = SideMenu::Closed;
    launch.run(host, id, pane, tx);
}

fn close_menu_layer(host: &mut dyn PanelsHost, close: fn(&mut dyn PanelsHost), tx: &mut Tx) {
    tx.instant();
    close(host);
}

fn clear_menu(host: &mut dyn PanelsHost) {
    host.side_mut().menu = SideMenu::Closed;
}
