//! Pane tab strip (`tab_bar`), tab triggers, New tab triggers, close button, and strip width layout.

use std::rc::Rc;

use opencode_ui::keymap::Bind;
use rune::{Anim, Color, Drag, El, Icon, PointerPress, ScrollFade, TextStyle, Tx, WheelEvent, el, icon, row, text};
use rune_theme_opencode::icons as ic;
use opencode_ui::focus::FocusExt;
use opencode_ui::palette::Pal;
use opencode_ui::style::{UI, UI_SMALL};
use opencode_ui::tab::TabExt;

use crate::drag::{choose_side_new_tab, side_new_tab_gesture, side_tab_gesture};
use crate::host::PanelsHost;
use crate::state::{
    FocusMode, MouseClose, NewTabEntry, ResolvedEntry, SideMenu, StripScroll, WidthHoldEvent, close_side_tab,
    dismiss_side_new_tab, open_side_new_tab, promote_preview_tab, release_pane_width_hold, select_side_tab,
    toggle_bottom_region,
};
use crate::{CloseButtonMode, SessionPane, TERMINAL_PLUGIN};

use super::pane::pane_slug;

const INSTANT: Anim = Anim::INSTANT;
/// A strip's narrowest tab (`tabs.css:503`).
const TAB_MIN_W: f32 = 72.0;

/// A tab's words: Desktop's trigger sets `font-variant-numeric: tabular-nums` (`tabs.css:554`),
/// whose `tnum` in Inter changes letters' advances too (`New tab` 50.797 against 50.891).
pub const TAB_UI: TextStyle = UI.features("\"tnum\"");
/// File tab filename (`packages/gui-plugins/src/file/label.tsx:14`, `packages/ui/src/styles/theme.css:9,13,19`,
/// `packages/ui/src/navigation/tabs/tabs.css:556,561`): `13px`, `font-medium` (`--font-weight-medium: 530`),
/// `leading-[calc(var(--font-size-base)*1.5)]` (`14px * 1.5 = 21px`), `tracking-normal` (`0`), inheriting `tnum`.
const FILE_TAB_UI: TextStyle = TextStyle::new(13.0, 530.0).lh(21.0).features("\"tnum\"");
const TAB_SMALL: TextStyle = UI_SMALL.features("\"tnum\"");

/// What a closable tab takes after its label: the wrapper's 8 pt gap, the 20 pt close button and
/// the 4 pt end padding (`tabs-trigger-wrapper`, `tabs.css`).
const CLOSE_END: f32 = 8.0 + 20.0 + 4.0;

/// A tab key as an element key.
pub(crate) fn safe_id(key: &str) -> String {
    key.replace(':', "-").replace('/', "_")
}

pub(super) fn is_pane_focused(
    host: &dyn PanelsHost,
    pane: SessionPane,
    entries: &[&ResolvedEntry],
    pane_active: Option<&str>,
) -> bool {
    // While the keyboard is the chat's, or a menu's (Desktop's side focus is the last focus, click
    // or key inside `#review-panel`; a menu's items sit in a portal outside it), the selected tab
    // rests at the strip's muted ink.
    // A plugin's field in the selected tab (the browser's address) holds the keyboard there too:
    // a press on it is a press inside the region, though it doesn't reach the pane's own press.
    let field_in_tab = entries
        .iter()
        .find(|e| pane_active == Some(e.key.as_str()))
        .is_some_and(|e| host.plugin_field_focused(e.plugin));
    // The composer's focus is the side region's input: reading the composer would rebuild the
    // region on every keystroke in it.
    let composing = host.composer_focused_untracked();
    let side = host.side();
    // A focused input inside `#review-panel` fires `focusin` there, lighting the tab even if the
    // region itself was unfocused (`closed-tabs.ts:146-153,186-193`, `side-region.tsx:108`,
    // `tabs.css:561-582`).
    match side.focus.mode() {
        FocusMode::Quiet => false,
        FocusMode::Active => (side.active_pane_focus == pane || field_in_tab) && !composing && !host.popup_holds_focus(),
        FocusMode::Unfocused => field_in_tab && !composing && !host.popup_holds_focus(),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn tab_bar<H: PanelsHost + 'static>(
    host: &H,
    p: &Pal,
    pane: SessionPane,
    (session_id, pane_w): (&str, f32),
    entries: &[&ResolvedEntry],
    pane_active: Option<&str>,
    _preview_key: Option<&str>,
    shown_new_id: Option<&str>,
    live_new: &[&NewTabEntry],
    reserve_toggle: bool,
) -> El {
    // Desktop's panel strip (`tabs.css`, `#review-panel` / `#bottom-panel`): 28 tall tabs 8 apart
    // from a 12 inset, the "+" after the last. The tabs share the room evenly, none wider than
    // its content nor narrower than 72 (`tabs.css:501`); past that the strip scrolls, the "+"
    // staying at its end, and each edge feathers by as much as is scrolled past it, up to 24
    // (`tab-strip-scroll.ts`).
    let slug = pane_slug(pane);
    let controls = pane == SessionPane::Bottom || reserve_toggle;
    let room = pane_w - 24.0 - if controls { 36.0 } else { 0.0 } - 36.0;
    let new_natural = 10.0 + rune::text::measure(opencode_i18n::t!("session.tab.new"), &[], TAB_UI, None).0 + CLOSE_END;
    let focused = is_pane_focused(host, pane, entries, pane_active);
    let active_of = |e: &ResolvedEntry| shown_new_id.is_none() && pane_active == Some(e.key.as_str());
    // A panel's own label (`PanelTab.label`) takes the icon's and the title's place, dimmed when the pane is unfocused (`tabs.css:561-582`).
    let leads: Vec<Option<El>> = entries.iter().map(|e| e.provider.lead(host, p, &e.tab, active_of(e) && focused)).collect();
    let naturals: Vec<f32> = entries
        .iter()
        .zip(&leads)
        .map(|(e, l)| tab_natural(e, l.is_some()) + host.panel_tab_extra_w(e.plugin, &e.tab))
        .chain(live_new.iter().map(|_| new_natural))
        .collect();
    let total_tabs = naturals.len();
    let slot = pane as usize;
    let hold = host.side().width_holds[slot].active(session_id, pane_w, total_tabs);
    let gaps = 8.0 * total_tabs.saturating_sub(1) as f32;
    let computed = strip_widths(&naturals, (room - hold).max(0.0) - gaps);
    let total_drawn_w: f32 = computed.iter().sum::<f32>() + gaps;
    let overflowing = total_drawn_w > room + 0.5;
    let mut widths = computed.into_iter();
    let lead = entries.iter().find(|e| !e.tab.pinned).is_some_and(|e| e.tab.first);
    let scroll_key = format!("side-tab-scroll-{slug}");
    let at = host.side().strips.clone();
    let (wheel_at, laid_at) = (at.clone(), at.clone());
    let (wheel_key, laid_key) = (scroll_key.clone(), scroll_key.clone());
    let sid = session_id.to_owned();
    let mut list = row()
        .key(format!("side-tab-list-{slug}"))
        .gap(8.0)
        .center_y()
        // A strip that grows glides to its end, to show the new tab, or to its start when the
        // tab opened at its lead (`nextTabStripScrollLeft`); the first layout of a session's
        // strip only notes where it stands.
        .on_layout_tx(move |_: &mut H, [_, _, w, _]: [f32; 4], tx: &mut Tx| {
            on_strip_layout(&laid_at, slot, &sid, lead, &laid_key, w, tx);
        });

    let mut tab_idx = 0usize;
    for (entry, lead_el) in entries.iter().zip(leads) {
        let dragging = host.side().pane_drag.dragging() == Some(entry.key.as_str());
        let width = widths.next().unwrap_or(TAB_MIN_W);
        let close_meta = MouseClose {
            session_id: session_id.to_owned(),
            pane_w,
            closed_w: width,
            is_last: tab_idx + 1 == total_tabs,
            overflowing,
            remaining: total_tabs.saturating_sub(1),
            gap: 8.0,
        };
        list = list.child(tab_trigger::<H>(p, pane, entry, lead_el, width, active_of(entry), focused, dragging, close_meta));
        tab_idx += 1;
    }

    for nt in live_new {
        let width = widths.next().unwrap_or(TAB_MIN_W);
        let active = shown_new_id == Some(nt.id.as_str());
        let close_meta = MouseClose {
            session_id: session_id.to_owned(),
            pane_w,
            closed_w: width,
            is_last: tab_idx + 1 == total_tabs,
            overflowing,
            remaining: total_tabs.saturating_sub(1),
            gap: 8.0,
        };
        list = list.child(new_tab_trigger::<H>(p, pane, &nt.id, width, active, focused, close_meta));
        tab_idx += 1;
    }

    let viewport = row()
        .key(scroll_key)
        .h(28.0)
        .shrink()
        .min_w(0.0)
        .scroll_x()
        .scroll_fade(ScrollFade::feather(24.0))
        .on_scroll_offset(move |pos| at[slot].x.set(pos.offset[0]))
        // A notch of an upright wheel moves the strip 50 sideways, down to the right
        // (`tab-strip-scroll.ts`); a sideways one scrolls it as any horizontal viewport.
        .on_wheel(move |_: &mut H, e: WheelEvent, tx: &mut Tx| on_strip_wheel(&wheel_at, slot, &wheel_key, e, tx))
        .child(list);

    // The `+` right after the tabs, and at the strip's end once they scroll (`side-region.tsx`,
    // the list's sticky last child).
    let strip = row()
        .key(format!("side-tabs-{slug}"))
        .gap(8.0)
        .center_y()
        .grow(1.0)
        .min_w(0.0)
        // Leaving the strip releases any mouse-close tab width hold (`tab-strip-widths.ts:74`).
        .on_hover_change(move |s: &mut H, inside: bool, tx: &mut Tx| {
            if !inside && release_pane_width_hold(s, pane) {
                tx.instant();
            } else {
                tx.live_only();
            }
        })
        .child(viewport)
        .child(
            el().key(format!("side-add-{slug}"))
                .role("button")
                .label_static(opencode_i18n::t!("session.tab.new"))
                .size(28.0, 28.0)
                .radius(6.0)
                .center()
                .hover(p.hover)
                .press(p.pressed)
                .hover_motion(INSTANT, INSTANT)
                .on_press_tx(move |s: &mut H, tx: &mut Tx| on_open_new_tab(s, pane, tx))
                .on_secondary_press(move |s: &mut H, ev: PointerPress, tx: &mut Tx| on_open_add_menu(s, pane, ev, tx))
                .child(icon(&ic::PLUS, 16.0).color(p.icon_muted)),
        );

    // Every pane's strip is `--tabs-bar-height` tall; its controls end at the header's inset.
    let mut bar = row()
        .w_full()
        .h(48.0)
        .pl(12.0)
        .pr(12.0)
        .gap(8.0)
        .center_y()
        .window_drag()
        .child(strip);

    if pane == SessionPane::Bottom {
        bar = bar.child(
            el().key("bottom-hide")
                .role("button")
                .label(Bind::BottomToggle.tip(opencode_i18n::t!("session.panel.bottom.hide")))
                .size(28.0, 28.0)
                .radius(6.0)
                .center()
                .hover(p.hover)
                .press(p.pressed)
                .hover_motion(INSTANT, INSTANT)
                .on_press(|s: &mut H| toggle_bottom_region(s))
                .child(icon(&ic::CHEVRON_DOWN, 16.0).color(p.icon_muted)),
        );
    } else if reserve_toggle {
        // Reserves space at the end of the top tab bar for the floating `side-toggle` button (`size-7`).
        // While the region is open the toggle sits in it, in the strip's Tab order.
        let toggle = super::toggle_in_strip(host).then(|| super::strip_toggle(host, p)).flatten();
        bar = bar.child(el().key("side-toggle-slot").size(28.0, 28.0).children(toggle));
    }

    bar
}

fn new_tab_trigger<H: PanelsHost + 'static>(
    p: &Pal,
    pane: SessionPane,
    nid: &str,
    width: f32,
    active: bool,
    focused: bool,
    close_meta: MouseClose,
) -> El {
    let middle_meta = close_meta.clone();
    let drag_nid = nid.to_owned();
    let press_nid = nid.to_owned();
    let close_nid = nid.to_owned();
    let middle_nid = nid.to_owned();
    let shown = active && focused;
    row()
        .key(format!("side-new-{nid}"))
        .role("tab")
        .label_static(opencode_i18n::t!("session.tab.new"))
        .h(28.0)
        .w(width)
        .pl(10.0)
        .pr(4.0)
        .gap(6.0)
        .center_y()
        .radius(6.0)
        .focus_inset(p)
        .hover_group()
        .fill(if active { p.layer2 } else { Color::TRANSPARENT })
        .hover(if active { p.layer2 } else { p.hover })
        .hover_motion(INSTANT, INSTANT)
        // Selected as the mouse button goes down, as the pane's other tabs are (Kobalte's
        // `Tabs.Trigger`); a key or a tap selects it on the press.
        .on_drag(move |s: &mut H, d: &mut Drag, tx: &mut Tx| side_new_tab_gesture(s, pane, &drag_nid, shown, d, tx))
        .on_press_tx(move |s: &mut H, tx: &mut Tx| on_new_tab_press(s, pane, &press_nid, tx))
        .child(
            text(opencode_i18n::t!("session.tab.new"))
                .font(TAB_UI)
                .color(if active && focused { p.text } else { p.muted })
                .truncate_fade()
                .shrink(),
        )
        .child(close_button::<H>(
            p,
            format!("side-new-close-{close_nid}"),
            active,
            move |s: &mut H, tx: &mut Tx| on_mouse_dismiss_new_tab(s, pane, &close_nid, &close_meta, !tx.focus_visible(), tx),
        ))
        // A middle click closes it, as its × does (`side-region.tsx`'s `onMiddleClick`).
        .closable_tab(move |s: &mut H, tx: &mut Tx| on_mouse_dismiss_new_tab(s, pane, &middle_nid, &middle_meta, true, tx))
}

/// How wide a strip's tabs are, given each one's content width and the room they share (the
/// strip's, less its gaps): Desktop's `flex: 1 1 0; min-width: 72px; max-width: max-content`
/// (`tabs.css:501`). The room is shared evenly; a tab whose content needs less than its share
/// keeps to its content and leaves the rest to the others; none is narrower than 72, so past 72
/// each the strip is wider than its room and scrolls.
fn strip_widths(naturals: &[f32], room: f32) -> Vec<f32> {
    let mut order: Vec<usize> = (0..naturals.len()).collect();
    order.sort_by(|&a, &b| naturals[a].total_cmp(&naturals[b]));
    let mut out = vec![TAB_MIN_W; naturals.len()];
    let (mut left, mut rest) = (room, naturals.len());
    for i in order {
        let w = naturals[i].min(left / rest as f32).max(TAB_MIN_W);
        out[i] = w;
        left -= w;
        rest -= 1;
    }
    out
}

/// The width tab `entry`'s content takes unshrunk (CSS `max-content`), as `tab_trigger` lays
/// it out: 10 in, the icon and 6, the title, the badge, then 8 to the close button and 4 from the
/// end, or 10 without one. A panel's own label (`lead`) is taken as the icon and the title it shows; a
/// terminal's or browser's title stops at 160 (`TerminalTabLabel`, `browser/renderer.tsx` `max-w-40`).
fn tab_natural(entry: &ResolvedEntry, lead: bool) -> f32 {
    let style = if !lead && entry.tab.file.is_some() { FILE_TAB_UI } else { TAB_UI };
    let title = rune::text::measure(&entry.tab.title, &[], style, None).0.ceil();
    let body = match (lead, entry.tab.icon) {
        (true, _) if matches!(entry.plugin, TERMINAL_PLUGIN | "browser") => 22.0 + title.min(160.0),
        // `packages/gui-plugins/src/context/renderer.tsx:23`: `gap-2` (8 pt) between 16 pt ring and title.
        (true, _) if entry.plugin == "context" => 24.0 + title,
        (true, _) => 22.0 + title,
        (false, Some(glyph)) if entry.tab.file.is_some() => tab_icon_size(entry, glyph) + 8.0 + title,
        (false, Some(glyph)) => tab_icon_size(entry, glyph) + 6.0 + title,
        (false, None) => title,
    };
    let badge = entry
        .tab
        .badge
        .as_ref()
        .map_or(0.0, |b| 6.0 + rune::text::measure(b, &[], TAB_SMALL, None).0);
    let start = if !lead && entry.tab.file.is_some() { 8.0 } else { 10.0 };
    let end = if entry.tab.can_close() { CLOSE_END } else { 10.0 };
    start + body + badge + end
}

/// A tab icon's size: Review (`review/renderer.tsx:188`), Files (`file/renderer.tsx:457`),
/// file badges, and browser page icons all render at 16 pt (`Icon` default `size="small"` = 16 px).
fn tab_icon_size(_entry: &ResolvedEntry, _glyph: &Icon) -> f32 {
    16.0
}

/// One tab of a pane's strip (`PanelTrigger`): 28 tall, 10 in, 72 at the narrowest, the
/// selected one on `layer-02`; 13/440 labels in the muted ink, the selected one's in the base
/// ink while its pane has the keyboard; the 20 pt close button 4 from its end. A press selects
/// it as the button goes down, a drag moves it between panes, a right click opens its menu.
#[allow(clippy::too_many_arguments)]
fn tab_trigger<H: PanelsHost + 'static>(
    p: &Pal,
    pane: SessionPane,
    entry: &ResolvedEntry,
    lead: Option<El>,
    width: f32,
    active: bool,
    focused: bool,
    dragging: bool,
    close_meta: MouseClose,
) -> El {
    let middle_meta = close_meta.clone();
    let select_key = entry.key.clone();
    let promote_key = entry.key.clone();
    let menu_key = entry.key.clone();
    let close_key = entry.key.clone();
    let middle_key = entry.key.clone();
    let drag_key = entry.key.clone();
    let drag_title = entry.tab.title.clone();
    let id = safe_id(&entry.key);

    let label_color = if entry.tab.missing {
        p.faint
    } else if active && focused {
        p.text
    } else {
        p.muted
    };

    let is_file = lead.is_none() && entry.tab.file.is_some();
    let mut pill = row()
        .key(format!("side-tab-{id}"))
        .role("tab")
        .label(entry.tab.title.clone())
        .h(28.0)
        .w(width)
        .pl(if is_file { 8.0 } else { 10.0 })
        .pr(if entry.tab.can_close() { 4.0 } else { 10.0 })
        .gap(6.0)
        .center_y()
        .radius(6.0)
        .focus_inset(p)
        .hover_group()
        .fill(if active { p.layer2 } else { Color::TRANSPARENT })
        .hover(if active { p.layer2 } else { p.hover })
        .hover_motion(INSTANT, INSTANT)
        .opacity(if dragging { 0.4 } else { 1.0 })
        .on_press_tx(move |s: &mut H, tx: &mut Tx| on_tab_press(s, &select_key, tx))
        .on_double_press(move |s: &mut H| promote_preview_tab(s, &promote_key))
        .on_secondary_press(move |s: &mut H, ev: PointerPress, tx: &mut Tx| on_open_tab_menu(s, &menu_key, ev, tx));
    // A middle click closes a tab that can close, on its release over the tab
    // (`panel-trigger.tsx`'s `onMiddleClick`, Desktop's `auxclick`).
    if entry.tab.can_close() {
        pill = pill.closable_tab(move |s: &mut H, tx: &mut Tx| on_mouse_close_tab(s, pane, &middle_key, &middle_meta, true, tx));
    }
    let (draggable, shown) = (!entry.tab.pinned && entry.tab.draggable, active && focused);
    pill = pill.on_drag(move |s: &mut H, d: &mut Drag, tx: &mut Tx| {
        side_tab_gesture(s, &drag_key, &drag_title, draggable, shown, d, tx);
    });

    if let Some(label) = lead {
        pill = pill.child(label.shrink().min_w(0.0));
    } else if entry.tab.file.is_some() {
        // `packages/gui-plugins/src/file/label.tsx:8-20`: `<div class="flex items-center gap-x-2 min-w-0">`
        // with 16 pt file icon (`tab-fileicon-color` / `-mono`) and `font-medium` `tracking-normal` label.
        let mut visual = row().gap(8.0).center_y().shrink().min_w(0.0);
        if let Some(glyph) = entry.tab.icon {
            let badge = icon(glyph, 16.0).key("icon").color(label_color);
            visual = visual.child(if active {
                badge
            } else {
                el().key("icon")
                    .size(16.0, 16.0)
                    .child(badge.key("icon-color").absolute(0.0, 0.0).group_opacity(0.0, 1.0))
                    .child(
                        icon(glyph, 16.0)
                            .key("icon-mono")
                            .absolute(0.0, 0.0)
                            .icon_one_color()
                            .color(label_color)
                            .group_opacity(1.0, 0.0),
                    )
            });
        }
        visual = visual.child(
            text(entry.tab.title.clone())
                .font(FILE_TAB_UI)
                .color(label_color)
                .truncate_fade()
                .shrink(),
        );
        pill = pill.child(visual);
    } else {
        if let Some(glyph) = entry.tab.icon {
            pill = pill.child(icon(glyph, tab_icon_size(entry, glyph)).color(if active { p.icon } else { p.icon_muted }));
        }
        pill = pill.child(
            text(entry.tab.title.clone())
                .font(TAB_UI)
                .color(label_color)
                .truncate_fade()
                .shrink(),
        );
    }

    if let Some(badge) = &entry.tab.badge {
        pill = pill.child(text(badge.clone()).font(TAB_SMALL).color(p.faint));
    }

    if entry.tab.can_close() {
        let always_visible = entry.tab.closable == CloseButtonMode::Standard || active;
        pill = pill.child(close_button::<H>(
            p,
            format!("side-tab-close-{id}"),
            always_visible,
            move |s: &mut H, tx: &mut Tx| on_mouse_close_tab(s, pane, &close_key, &close_meta, !tx.focus_visible(), tx),
        ));
    }

    pill
}

/// A tab's close button (`tabs-trigger-close-button`): 20 pt, the 16 pt small cross, shown on
/// the selected tab and on hover, 8 after the label (the tab's 6 pt gap and 2 more), as the
/// trigger wrapper's gap puts it.
fn close_button<H: PanelsHost + 'static>(
    p: &Pal,
    key: String,
    shown: bool,
    close: impl Fn(&mut H, &mut Tx) + 'static,
) -> El {
    el().key(key)
        .ml(2.0)
        .role("button")
        .label_static(opencode_i18n::t!("common.closeTab"))
        .size(20.0, 20.0)
        .radius(4.0)
        .center()
        .hover(p.hover)
        .hover_motion(INSTANT, INSTANT)
        .on_press_tx(close)
        .child(
            icon(&ic::CLOSE, 16.0)
                .color(if shown { p.icon_muted } else { Color::TRANSPARENT })
                .group_color(p.icon_muted),
        )
}

fn on_strip_layout(
    laid_at: &Rc<[StripScroll; 3]>,
    slot: usize,
    sid: &str,
    lead: bool,
    laid_key: &str,
    w: f32,
    tx: &mut Tx,
) {
    let laid = laid_at[slot].laid.replace(Some((sid.to_owned(), w, lead)));
    let Some((_, was_w, was_lead)) = laid.filter(|l| l.0 == sid) else { return };
    if w <= was_w {
        return;
    }
    tx.scroll_to(laid_key.to_owned(), if !was_lead && lead { 0.0 } else { w }, 0.0);
}

fn on_strip_wheel(wheel_at: &Rc<[StripScroll; 3]>, slot: usize, wheel_key: &str, e: WheelEvent, tx: &mut Tx) -> bool {
    // Off macOS Shift turns a wheel sideways, but Chromium hands the page its upright delta.
    let shifted = !cfg!(target_os = "macos") && e.mods.shift && e.dy == 0.0;
    let (dx, dy) = if shifted { (0.0, e.dx) } else { (e.dx, e.dy) };
    if dy.abs() <= dx.abs() {
        return false;
    }
    tx.scroll_to_x(
        wheel_key.to_owned(),
        wheel_at[slot].x.get() + if dy < 0.0 { 50.0 } else { -50.0 },
    );
    true
}

fn on_new_tab_press(host: &mut dyn PanelsHost, pane: SessionPane, nid: &str, tx: &mut Tx) {
    if host.side_mut().pane_drag.take_suppressed() {
        return tx.live_only();
    }
    choose_side_new_tab(host, pane, nid, tx);
}

fn on_mouse_close_tab(
    host: &mut dyn PanelsHost,
    pane: SessionPane,
    key: &str,
    meta: &MouseClose,
    pointer: bool,
    tx: &mut Tx,
) {
    let event = if pointer { WidthHoldEvent::MouseClose(meta.clone()) } else { WidthHoldEvent::Release };
    host.side_mut().width_holds[pane as usize].step(event);
    close_side_tab(host, key, tx);
}

fn on_mouse_dismiss_new_tab(
    host: &mut dyn PanelsHost,
    pane: SessionPane,
    nid: &str,
    meta: &MouseClose,
    pointer: bool,
    tx: &mut Tx,
) {
    tx.instant();
    let event = if pointer { WidthHoldEvent::MouseClose(meta.clone()) } else { WidthHoldEvent::Release };
    host.side_mut().width_holds[pane as usize].step(event);
    dismiss_side_new_tab(host, nid);
}

fn on_open_new_tab(host: &mut dyn PanelsHost, pane: SessionPane, tx: &mut Tx) {
    tx.instant();
    release_pane_width_hold(host, pane);
    open_side_new_tab(host, pane);
    // A press on `+` is a press inside the region: in Desktop the button takes the focus, so the
    // region is `data-focused` and its new tab shows in the selected ink (`tabs.css:578-584`).
    host.side_mut().focus.press();
    host.set_composer_focused(false);
}

fn on_open_add_menu(host: &mut dyn PanelsHost, pane: SessionPane, ev: PointerPress, tx: &mut Tx) {
    tx.instant();
    host.wake_panel_launchers();
    host.side_mut().menu = SideMenu::Add { pane, x: ev.x, y: ev.y };
}

fn on_tab_press(host: &mut dyn PanelsHost, key: &str, tx: &mut Tx) {
    // The press already selected it, and the release that ends a drag isn't a click on
    // the tab under it: either draws nothing. A key or a tap selects it here.
    if host.side_mut().pane_drag.take_suppressed() {
        return tx.live_only();
    }
    select_side_tab(host, key, tx);
}

fn on_open_tab_menu(host: &mut dyn PanelsHost, key: &str, ev: PointerPress, tx: &mut Tx) {
    tx.instant();
    host.side_mut().menu = SideMenu::Tab { key: key.to_owned(), x: ev.x, y: ev.y };
}
