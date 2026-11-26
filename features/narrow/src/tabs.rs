//! The narrow titlebar's tabs (`titlebar.tsx:581-706`, `titlebar.css:32-107`): one trigger in
//! place of Home, Search and the tab strip, showing the open tab's avatar, title and the tab
//! count, which opens a drawer of every tab, New session, Home, Settings and Help.

use opencode_i18n::t;
use rune::{El, Icon, PointerPress, TextStyle, Tx, col, el, icon, row, text};

/// `sessions-stack` (`packages/ui/src/icons/catalog.ts:393`, `titlebar.tsx:796,879`, `opencode#66594`).
static SESSIONS_STACK: Icon = Icon {
    name: "sessions-stack",
    view: 16.0,
    body: r#"<path d="M3.5 3.5H12.5M1.5 5.5V12.5H14.5V5.5H12.5H3.5H1.5Z" stroke-linecap="round" stroke-linejoin="round" stroke="currentColor"/>"#,
};
use rune_theme_opencode::icons as ic;
use opencode_ui::focus::FocusExt;
use opencode_ui::palette::{Pal, kit};
use opencode_ui::style::{INSTANT, activity_ring, tab_busy_glyph};
use opencode_ui::tab::TabExt;

use crate::host::{NarrowHost, NarrowTab, NarrowTabStatus};
use crate::machine::TabsEvent;

/// The drawer's rows' 13 / 16 at 440.
const ROW: TextStyle = TextStyle::new(13.0, 440.0).lh(16.0);
/// A tab's title in the drawer (`tab-title`: 530, `text-base`).
const TITLE: TextStyle = TextStyle::new(13.0, 530.0).lh(16.0);

pub const SHEET: &str = "narrow-tabs-sheet";

/// Toggles the narrow tabs drawer open or closed.
pub fn toggle_tabs_drawer(host: &mut dyn NarrowHost, tx: &mut Tx) {
    let open = !host.narrow_state().tabs.is_open();
    crate::tabs_drawer(host, TabsEvent::Changed { open }, tx);
}

fn press_new_session(host: &mut dyn NarrowHost, tx: &mut Tx) {
    host.new_session(tx);
    crate::tabs_drawer(host, TabsEvent::Close, tx);
}

fn press_home(host: &mut dyn NarrowHost, tx: &mut Tx) {
    if !host.is_home_route() {
        host.toggle_home(tx);
    }
    crate::tabs_drawer(host, TabsEvent::Close, tx);
}

fn press_settings(host: &mut dyn NarrowHost, tx: &mut Tx) {
    let now = host.clock();
    crate::tabs_drawer(host, TabsEvent::SettingsRequested { now }, tx);
}

fn press_help(host: &mut dyn NarrowHost, tx: &mut Tx) {
    crate::tabs_drawer(host, TabsEvent::Close, tx);
    harness_log::open_url(tx, "https://github.com/meeyao/brocode-desktop/issues/new");
}

fn pick_tab(host: &mut dyn NarrowHost, uid: &str, tx: &mut Tx) {
    host.select_tab(uid, tx);
    crate::tabs_drawer(host, TabsEvent::Close, tx);
}

fn close_tab(host: &mut dyn NarrowHost, uid: &str, tx: &mut Tx) {
    host.close_tab(uid, tx);
}

fn open_tab_menu(host: &mut dyn NarrowHost, uid: &str, press: PointerPress, tx: &mut Tx) {
    host.open_tab_menu(uid, press, tx);
}

/// `mobile-tabs-trigger`: 28 tall, `px-2 gap-2`, 6 pt corners; the layer-02 wash under the
/// hover overlay while its drawer is open.
pub fn trigger<H: NarrowHost + 'static>(s: &H, p: &Pal) -> El {
    let current = s.current_tab_uid();
    let tab = current.as_deref().and_then(|uid| s.narrow_tab(uid));
    let lead = match &tab {
        None => icon(&SESSIONS_STACK, 16.0).color(p.icon),
        Some(t) if t.is_draft => icon(&ic::EDIT, 16.0).color(p.icon),
        Some(t) => s.tab_avatar(p, &t.uid),
    };
    let title = match &tab {
        None => t!("home.title").to_owned(),
        Some(t) => t.title.clone(),
    };
    let busy = tab.as_ref().is_some_and(|t| !t.is_draft && t.status == Some(NarrowTabStatus::Busy));
    let open = s.narrow_state().tabs.is_open();
    let mut b = row()
        .key("narrow-tabs-trigger")
        .role("button")
        .label_static(t!("titlebar.tabs"))
        .expanded(open)
        .h(28.0)
        .grow(1.0)
        .shrink()
        .min_w(0.0)
        .px(8.0)
        .gap(8.0)
        .center_y()
        .radius(6.0)
        .focus_inset(p)
        .no_window_drag()
        .on_press_tx(|s: &mut H, tx: &mut Tx| toggle_tabs_drawer(s, tx))
        .children(open.then(|| wash(p.hover, 6.0)))
        .child(row().key("avatar").size(16.0, 16.0).center().child(lead))
        .child(text(title).key("title").font(ROW).color(p.text).truncate().grow(1.0).shrink().min_w(0.0))
        .children(busy.then(|| activity_ring(p)))
        .child(text(s.tab_count().to_string()).key("count").font(ROW).color(p.muted));
    if open {
        b = b.fill(p.layer2);
    }
    b
}

/// The drawer's contents (`mobile-tabs-drawer`): 8 below the handle, its sections 8 apart.
pub fn drawer<H: NarrowHost + 'static>(s: &H, p: &Pal) -> El {
    let home = s.is_home_route();
    let current = s.current_tab_uid();
    let tabs = s
        .narrow_tabs()
        .iter()
        .fold(col().key("narrow-tabs-list").w_full().gap(2.0), |list, t| list.child(tab_row(s, p, t, current.as_deref())));
    let new = button("narrow-tabs-new", None, &ic::PLUS, 16.0, t!("command.session.new"), p.text, p.icon)
        .focus_inset(p)
        .hover(p.layer2)
        .on_press_tx(|s: &mut H, tx: &mut Tx| press_new_session(s, tx));
    let mut home_row = button(
        "narrow-tabs-home",
        home.then_some(p.pressed),
        &SESSIONS_STACK,
        16.0,
        t!("home.title"),
        if home { p.text } else { p.faint },
        if home { p.icon } else { p.icon_faint },
    )
    .w_full()
    .focus_inset(p)
    .on_press_tx(|s: &mut H, tx: &mut Tx| press_home(s, tx));
    if home {
        home_row = home_row.fill(p.layer2);
    }
    let settings = button("narrow-tabs-settings", None, &ic::SETTINGS_GEAR, 14.0, t!("sidebar.settings"), p.faint, p.icon_faint)
        .grow(1.0)
        .shrink()
        .min_w(0.0)
        .focus_inset(p)
        .hover(p.layer2)
        .on_press_tx(|s: &mut H, tx: &mut Tx| press_settings(s, tx));
    let help = button("narrow-tabs-help", None, &ic::HELP, 14.0, t!("sidebar.help"), p.faint, p.icon_faint)
        .focus_inset(p)
        .hover(p.layer2)
        .on_press_tx(|s: &mut H, tx: &mut Tx| press_help(s, tx));
    let hairline = s.chrome().hairline;
    col()
        .key("narrow-tabs-drawer")
        .w_full()
        .mt(8.0)
        .gap(8.0)
        .min_h(0.0)
        // The drawer's vertical tab strip: no bar, as Desktop hides it (`no-scrollbar`, `tab-strip.tsx:629`).
        .child(opencode_ui::scroller::scroller("narrow-tabs-scroll").hidden(col().w_full().shrink().min_h(0.0), tabs))
        .child(new.w_full())
        .child(
            col()
                .key("narrow-tabs-foot")
                .w_full()
                .gap(4.0)
                .pt(8.0)
                .child(el().key("rule").pin(Some(0.0), Some(0.0), Some(0.0), None).h(hairline).fill(p.border_muted))
                .child(home_row)
                .child(row().key("narrow-tabs-utility").w_full().gap(4.0).center_y().child(settings).child(help)),
        )
}

/// A drawer button: 28 tall, `px-2 gap-2`, 6 pt corners, `glyph` at `size` and `label`.
fn button(key: &str, washed: Option<rune::Color>, glyph: &'static rune::Icon, size: f32, label: &str, ink: rune::Color, glyph_ink: rune::Color) -> El {
    row()
        .key(key.to_owned())
        .role("button")
        .label(label.to_owned())
        .h(28.0)
        .px(8.0)
        .gap(8.0)
        .center_y()
        .radius(6.0)
        .hover_motion(INSTANT, INSTANT)
        .children(washed.map(|c| wash(c, 6.0)))
        .child(row().size(16.0, 16.0).center().child(icon(glyph, size).color(glyph_ink)))
        .child(text(label.to_owned()).font(ROW).color(ink).truncate().shrink().min_w(0.0))
}

/// One tab in the drawer (`TitlebarTabStrip` vertical in `mobile-tabs-drawer`): 44 tall, `px-1.5
/// gap-1.5`, its avatar and title at 530, the 44 pt close button always showing at its end, the
/// open tab on the pressed wash.
fn tab_row<H: NarrowHost + 'static>(s: &H, p: &Pal, t: &NarrowTab, current: Option<&str>) -> El {
    let uid = t.uid.clone();
    let active = current == Some(uid.as_str());
    let lead = if t.is_draft { icon(&ic::EDIT, 16.0).color(p.icon) } else { s.tab_avatar(p, &uid) };
    // Mixed servers' tabs, in strip order, with no server badge (`TitlebarTabStrip`).
    let title = t.title.clone();
    let status = t.status.filter(|_| !active);
    let (select, close, middle, menu_uid) = (uid.clone(), uid.clone(), uid.clone(), uid.clone());
    let mut r = row()
        .key(format!("narrow-tab-{uid}"))
        .role("button")
        .label(title.clone())
        .w_full()
        .min_h(44.0)
        .h(44.0)
        .px(6.0)
        .gap(6.0)
        .center_y()
        .radius(6.0)
        .focus_inset(p)
        // A middle click closes it, as the titlebar's tabs do (`TabNavItem`'s `auxclick`).
        .closable_tab(move |s: &mut H, tx: &mut Tx| close_tab(s, &middle, tx))
        .on_press_tx(move |s: &mut H, tx: &mut Tx| pick_tab(s, &select, tx))
        .children(active.then(|| wash(p.pressed, 6.0)))
        .child(row().size(16.0, 16.0).center().child(lead))
        .child(
            text(title)
                .key("title")
                .font(TITLE)
                .color(p.text)
                .truncate()
                .grow(1.0)
                .shrink()
                .min_w(0.0)
                .mr(if status.is_some() { 58.0 - 6.0 } else { 38.0 - 6.0 }),
        )
        .children(status.map(|st| status_mark(p, st).pin(None, Some(12.0), Some(44.0), None)))
        .child(
            row()
                .key("close")
                .role("button")
                .label_static(t!("common.close"))
                .size(44.0, 44.0)
                .radius(4.0)
                .focus_inset(p)
                .center()
                .pin(None, Some(0.0), Some(0.0), None)
                .on_press_tx(move |s: &mut H, tx: &mut Tx| close_tab(s, &close, tx))
                .child(icon(&ic::XMARK_SMALL, 16.0).color(p.icon_faint)),
        );
    if active {
        r = r.fill(p.bg_deep);
    }
    r.on_secondary_press(move |s: &mut H, press: PointerPress, tx: &mut Tx| open_tab_menu(s, &menu_uid, press, tx))
}

/// `background: linear-gradient(over, over), base`'s overlay: the first child of an element
/// filled with `base`, under its content.
pub fn wash(over: rune::Color, radius: f32) -> El {
    el().key("wash").pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0)).radius(radius).fill(over)
}

/// The tab's mark in its 20 pt slot, 12 down, before the close button.
fn status_mark(p: &Pal, st: NarrowTabStatus) -> El {
    let dot = |c| el().size(7.0, 7.0).radius(3.5).fill(c);
    let mark = match st {
        NarrowTabStatus::Busy => tab_busy_glyph(kit(p), 14.0),
        NarrowTabStatus::Attention => dot(p.warning),
        NarrowTabStatus::Unread => dot(p.bg_accent),
        NarrowTabStatus::Failed => dot(p.danger),
    };
    row().key("status").size(20.0, 20.0).center().child(mark)
}
