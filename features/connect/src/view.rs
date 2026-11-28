//! The connect dialog on screen (`providers/connect/dialog.tsx`, `#63829`, `#64882`, `#64927`):
//! a large surface matching palette width (`680 × 600`) on the base fill with 12 pt corners;
//! every step fits its content up to the large height, and the top edge stays where that height
//! puts it. The list is the search dialog's (`search-list.css` `.search-dialog`): a bare search
//! under a compact header, a hairline across.

use rune::{Color, Cx, El, Key, KeyEvent, Spring, TextStyle, Transition, Tx, col, el, icon, row, spacer, text};
use rune_kit::TextInput;
use rune_theme_opencode::icons as ic;
use opencode_ui::dialog::DialogSize;
use opencode_ui::focus::FocusExt;
use opencode_ui::palette::{Pal, kit};

use crate::catalog;
use crate::dialog::{self, BackTarget, Event, Frame};
use crate::ui::{self, BODY, BODY_MED, DIALOG_CLOSE, Fit};
use crate::{AuthHost, ConnectHost, Open};

/// The search dialog's title and description (`#64882`): 13 on a 20 line.
const TITLE: TextStyle = TextStyle::new(13.0, 440.0).lh(20.0).tracking(-0.04);
const DESCRIPTION: TextStyle = TextStyle::new(13.0, 440.0).lh(20.0).tracking(-0.04);
/// The list's group labels: 12 on 18 at 440, tracking 0.05, faint.
const GROUP: TextStyle = TextStyle::new(12.0, 440.0).lh(18.0).tracking(0.05);

/// The integrations the list offers, in the TUI's groups: connected first by name, then the
/// popular ones in their order, then the rest by name. MCP sign-ins live in the MCPs dialog.
/// Go is listed only with a saved Go key, before OpenCode (`providers/connect/dialog.tsx`, `#65466`).
// `packages/app/src/providers/catalog/order.ts` (`opencode#66736`).
const POPULAR: [&str; 9] = ["opencode-go", "opencode", "openai", "anthropic", "claude-agent", "github-copilot", "google", "openrouter", "vercel"];

/// Muted note beside OpenCode Go while unconnected (`providerNote`, `#65466`).
fn provider_note(id: &str) -> Option<&'static str> {
    (id == "opencode-go").then(|| opencode_i18n::t!("dialog.provider.opencodeGo.tagline"))
}

pub struct Row {
    pub id: String,
    pub name: String,
    pub account: Option<String>,
    pub needs_sign_in: bool,
    /// Environment variables only: nothing to connect from here.
    pub disabled: bool,
}

pub fn groups(host: &dyn ConnectHost) -> Vec<(&'static str, Vec<Row>)> {
    let open = host.connect().dialog();
    let query = open.map_or(String::new(), |o| o.search.text.trim().to_owned());
    let fold = ui::search_fold();
    let list = catalog::integrations(host, crate::connect_dir(host)).unwrap_or_default();
    let rows: Vec<Row> = list
        .iter()
        .filter(|i| i.source.as_deref() != Some("mcp"))
        // OpenCode Go connects through OpenCode: listed only for someone who saved a Go key (`#65466`).
        .filter(|i| i.id != "opencode-go" || i.connections.iter().any(|c| c.credential))
        .filter(|i| fold.contains(&format!("{} {}", i.id, i.name), &query))
        .map(|i| Row {
            id: i.id.clone(),
            name: i.name.clone(),
            account: i.connections.first().map(|c| c.label.clone()),
            needs_sign_in: i.connections.first().is_some_and(|c| c.status.is_some()),
            disabled: i.connect_methods().is_empty() && i.accounts().is_empty(),
        })
        .collect();
    let connected = |r: &Row| r.account.is_some();
    let (mut connected_rows, rest): (Vec<Row>, Vec<Row>) = rows.into_iter().partition(connected);
    connected_rows.sort_by(|a, b| rune::text::compare_names(&a.name, &b.name));
    let (mut popular, mut services): (Vec<Row>, Vec<Row>) = rest.into_iter().partition(|r| POPULAR.contains(&r.id.as_str()));
    popular.sort_by_key(|r| POPULAR.iter().position(|p| *p == r.id));
    services.sort_by(|a, b| rune::text::compare_names(&a.name, &b.name));
    [
        (opencode_i18n::t!("dialog.provider.connected"), connected_rows),
        (opencode_i18n::t!("dialog.provider.group.popular"), popular),
        (opencode_i18n::t!("dialog.provider.group.services"), services),
    ]
    .into_iter()
    .filter(|(_, r)| !r.is_empty())
    .collect()
}

/// The rows the arrows move through: every one but those connected from the environment only.
fn rows(host: &dyn ConnectHost) -> Vec<String> {
    groups(host).into_iter().flat_map(|(_, r)| r).filter(|r| !r.disabled).map(|r| r.id).collect()
}

/// ↑ ↓ and ↵ in the list.
pub fn picker_key(host: &mut dyn ConnectHost, ev: &KeyEvent, tx: &mut Tx) -> bool {
    let ids = rows(host);
    let Some(open) = host.connect_mut().dialog_mut() else { return false };
    match ev.key {
        Key::Down | Key::Up if !ids.is_empty() => {
            let at = open.active.as_ref().and_then(|a| ids.iter().position(|i| i == a));
            let n = ids.len();
            let next = match (at, ev.key == Key::Down) {
                (None, true) => 0,
                (None, false) => n - 1,
                (Some(i), true) => (i + 1) % n,
                (Some(i), false) => (i + n - 1) % n,
            };
            open.active = Some(ids[next].clone());
            tx.instant();
            tx.scroll_into_view_in("connect-list", format!("provider-{}", ids[next]));
            true
        }
        Key::Enter => {
            let Some(id) = open.active.clone() else { return false };
            pick(host, &id, tx);
            true
        }
        _ => false,
    }
}

/// The dialog's name: the list's, or the step's title.
pub fn title(host: &dyn ConnectHost) -> String {
    let Some(open) = host.connect().dialog() else { return String::new() };
    let Some(p) = &open.dialog.provider else { return opencode_i18n::t!("dialog.provider.title").into() };
    let name = step_name(host, &p.id);
    match dialog::body(&open.dialog) {
        dialog::Body::Models => opencode_i18n::t!("provider.connect.models.title", provider = name),
        _ => opencode_i18n::t!("provider.connect.title", provider = name),
    }
}

fn pick(host: &mut dyn ConnectHost, id: &str, tx: &mut Tx) {
    tx.instant();
    let accounts = crate::accounts_of(host, id);
    crate::connect_event(host, Event::Selected { provider: id.to_owned(), from_picker: true, accounts }, tx);
}

fn close_on_backdrop(host: &mut dyn ConnectHost, authorizing: bool) {
    if !authorizing {
        crate::close(host, &mut Tx::default());
    }
}

fn navigate(host: &mut dyn ConnectHost, target: BackTarget, tx: &mut Tx) {
    tx.instant();
    crate::connect_event(host, Event::Navigate(target), tx);
}

fn clear_active(host: &mut dyn ConnectHost, tx: &mut Tx) {
    tx.instant();
    if let Some(o) = host.connect_mut().dialog_mut() {
        o.active = None;
    }
}

fn set_active_on_hover(host: &mut dyn ConnectHost, id: &str, inside: bool, tx: &mut Tx) {
    if inside && let Some(o) = host.connect_mut().dialog_mut() {
        tx.instant();
        o.active = Some(id.to_owned());
    }
}

fn step_name(host: &dyn ConnectHost, id: &str) -> String {
    if id == dialog::CONSOLE {
        opencode_i18n::t!("provider.connect.opencode.name").into()
    } else {
        crate::provider_name(host, id)
    }
}

fn remote_label(host: &dyn ConnectHost, p: &Pal, id: &str) -> Option<El> {
    if id != dialog::CONSOLE {
        return None;
    }
    let AuthHost::Remote(name) = &host.connect().dialog()?.auth_host else { return None };
    let label = opencode_i18n::t!("provider.connect.remote.label", server = name);
    Some(
        row()
            .key("connect-remote")
            .max_w(160.0)
            .shrink()
            .mr(12.0)
            .center_y()
            .gap(6.0)
            .help(opencode_i18n::t!("provider.connect.remote.description"))
            .child(icon(&ic::SERVER, 14.0).color(p.icon_muted))
            .child(text(label).font(TITLE).color(p.muted).truncate()),
    )
}

/// The Connect provider dialog modal layer (`DialogSize::Large`, content fit).
pub fn layer<H: ConnectHost + 'static>(s: &H, p: &Pal, cx: &Cx<H>) -> Vec<El> {
    if !s.connect().is_open() {
        return Vec::new();
    }
    let win = ui::dialog_window(s, cx);
    dialog(s, p, win)
}

/// Builds the Connect provider dialog modal sized for `win`.
pub fn dialog<H: ConnectHost + 'static>(s: &H, p: &Pal, win: [f32; 2]) -> Vec<El> {
    let Some(open) = s.connect().dialog() else { return Vec::new() };
    let frame = dialog::frame(&open.dialog);
    let authorizing = open.dialog.provider.as_ref().and_then(|p| p.step.connect()).is_some_and(crate::machine::authorizing);
    let label = title(s);
    let surface = if frame == Frame::Models {
        let id = open.dialog.provider.as_ref().map(|pr| pr.id.clone()).unwrap_or_default();
        let pname = step_name(s, &id);
        let body = crate::steps::models::view(s, open, p, &id, &pname, remote_label(s, p, &id));
        ui::overlay(p, col())
            .key("connect-dialog")
            .role("dialog")
            .label(label)
            .radius(12.0)
            .fill(p.bg_base)
            .clip_soft(0.0)
            .transition(Transition::none())
            .motion(Spring::NONE)
            .child(body)
            .modal(true, move |s: &mut H| close_on_backdrop(s, authorizing))
    } else {
        let head = header(s, open, p, frame);
        let content = match frame {
            Frame::Picker => picker(s, open, p),
            _ => build_focus_host(label.clone(), crate::steps::body(s, p, frame)),
        };
        build_dialog_surface(p, label, head, content)
            // A stray press beside it mustn't drop a sign-in the user is finishing in the browser.
            .modal(true, move |s: &mut H| close_on_backdrop(s, authorizing))
    };
    // `<Dialog size="large" fit>` (`providers/connect/dialog.tsx`, `#64882`): every step fits its
    // content within the large height, and the top edge stays put.
    ui::sized_modal("connect", p, DialogSize::Large, Fit::Content, win, surface)
}

fn build_dialog_surface(p: &Pal, label: String, head: El, content: El) -> El {
    ui::overlay(p, col())
        .key("connect-dialog")
        .role("dialog")
        .label(label)
        .radius(12.0)
        .fill(p.bg_base)
        .clip_soft(0.0)
        .transition(Transition::none())
        .motion(Spring::NONE)
        .child(head)
        .child(content)
}

fn build_focus_host(label: String, body: El) -> El {
    col()
        .key("connect-focus-host")
        .focusable(true)
        .tab_index(-1)
        .no_focus_ring()
        .role("group")
        .label(label)
        .w_full()
        .shrink()
        .min_h(0.0)
        .child(body)
}

fn close_button<H: ConnectHost + 'static>(p: &Pal) -> El {
    close_button_shell(p).on_press_tx(|s: &mut H, tx: &mut Tx| crate::close(s, tx))
}

fn close_button_shell(p: &Pal) -> El {
    el().key("connect-close")
        .role("button")
        .label_static(opencode_i18n::t!("common.close"))
        .size(20.0, 20.0)
        .radius(4.0)
        .center()
        .hover(p.hover)
        .press(p.pressed)
        .child(icon(&DIALOG_CLOSE, 16.0).color(p.icon_muted))
}

fn crumb<H: ConnectHost + 'static>(content: El, key: &'static str, name: String, target: Option<BackTarget>) -> El {
    let el = content.key(key);
    match target {
        None => el,
        Some(target) => el.role("button").label(name).on_press_tx(move |s: &mut H, tx: &mut Tx| navigate(s, target, tx)),
    }
}

fn header<H: ConnectHost + 'static>(s: &H, open: &Open, p: &Pal, frame: Frame) -> El {
    let close = close_button::<H>(p);
    match frame {
        // `.search-dialog`: 10 pt above and at the end, 14 before, 6 below.
        Frame::Picker => build_picker_header(p, close),
        Frame::Models => el(),
        Frame::Provider | Frame::Accounts => {
            let Some(provider) = open.dialog.provider.as_ref() else { return row() };
            let root = opencode_i18n::t!("provider.connect.breadcrumb.root");
            let name = step_name(s, &provider.id);
            let target = dialog::back_target(&open.dialog).filter(|t| *t != BackTarget::Picker);
            let root_crumb = crumb::<H>(
                text(root).font(TITLE).color(p.faint),
                "connect-crumb-root",
                root.into(),
                (!open.dialog.standalone).then_some(BackTarget::Picker),
            );
            let provider_crumb = crumb::<H>(
                row()
                    .shrink()
                    .min_w(0.0)
                    .gap(6.0)
                    .center_y()
                    .child(ui::provider_mark(&provider.id, p.icon))
                    .child(text(name.clone()).font(BODY_MED).color(p.text).truncate()),
                "connect-crumb-provider",
                name,
                target,
            );
            build_provider_header(p, root_crumb, provider_crumb, remote_label(s, p, &provider.id), close)
        }
    }
}

fn build_picker_header(p: &Pal, close: El) -> El {
    // `.search-dialog` (`#64882`): 14 pt above, 16 at the sides, 8 below.
    row()
        .key("connect-header")
        .w_full()
        .pt(14.0)
        .px(16.0)
        .pb(8.0)
        .child(
            col()
                .grow(1.0)
                .shrink()
                .gap(2.0)
                .child(text(opencode_i18n::t!("dialog.provider.title")).font(TITLE).color(p.text))
                .child(text(opencode_i18n::t!("dialog.provider.description")).font(DESCRIPTION).color(p.muted)),
        )
        .child(close.mr(-2.0))
}

fn build_provider_header(p: &Pal, root_crumb: El, provider_crumb: El, remote: Option<El>, close: El) -> El {
    let nav = row()
        .role("navigation")
        .label_static(opencode_i18n::t!("provider.connect.breadcrumb.label"))
        .grow(1.0)
        .shrink()
        .min_w(0.0)
        .gap(4.0)
        .center_y()
        .child(root_crumb)
        .child(icon(&ic::CHEVRON_RIGHT, 16.0).mirror_in_rtl().color(p.icon_muted))
        .child(provider_crumb);
    row()
        .key("connect-header")
        .w_full()
        .pt(14.0)
        .px(16.0)
        .pb(8.0)
        .child(nav)
        .children(remote)
        .child(close.mr(-2.0))
}

/// The searchable list (`ProviderPicker`).
fn picker<H: ConnectHost + 'static>(s: &H, open: &Open, p: &Pal) -> El {
    let field = TextInput::new(&open.search)
        .placeholder(opencode_i18n::t!("common.search.placeholder"))
        .label(opencode_i18n::t!("dialog.provider.search.placeholder"))
        .spellcheck(s.spellcheck())
        .leading_icon(&ic::MAGNIFYING_GLASS)
        .width(None)
        .height(32.0)
        .plain()
        .on_edit(|s: &mut H| s.connect_mut().search_mut())
        .on_change(|s: &mut H, tx: &mut Tx| clear_active(s, tx))
        .on_key(|s: &mut H, ev, tx| crate::key(s, ev, tx))
        .build(kit(p))
        .key("connect-search");
    let groups = groups(s);
    let sections: Vec<El> = groups
        .into_iter()
        .map(|(title, rows)| {
            let row_els: Vec<El> = rows
                .iter()
                .map(|r| provider_row::<H>(p, r, open.active.as_deref() == Some(r.id.as_str())))
                .collect();
            build_group_section(p, title, row_els)
        })
        .collect();
    build_picker_frame(p, field, sections)
}

fn build_group_section(p: &Pal, title: &'static str, rows: Vec<El>) -> El {
    col()
        .key(format!("connect-group-{title}"))
        .w_full()
        .gap(2.0)
        .child(el().px(8.0).pt(10.0).pb(2.0).child(text(title).font(GROUP).color(p.faint)))
        .children(rows)
}

fn build_picker_frame(p: &Pal, field: El, sections: Vec<El>) -> El {
    // The hairline reaches across the body's 8 pt padding (`dialog.tsx:266,328`: `px-2 pb-2`, `after:!-mx-2`).
    let search = col()
        .key("connect-search-row")
        .w_full()
        .gap(6.0)
        .child(field)
        .child(rune::hairline(p.border).key("connect-search-rule").mx(-8.0));
    let mut list = col().key("connect-groups").w_full().gap(2.0).pb(32.0);
    if sections.is_empty() {
        list = list.child(row().key("connect-empty").w_full().h(96.0).center().child(text(opencode_i18n::t!("dialog.provider.empty")).font(BODY).color(p.muted)));
    } else {
        list = list.children(sections);
    }
    col()
        .key("connect-picker")
        .w_full()
        .grow(1.0)
        .shrink()
        .min_h(0.0)
        .px(8.0)
        .pb(8.0)
        .gap(2.0)
        .child(search)
        // No bar, as Desktop hides it (`[scrollbar-width:none]`, `providers/connect/dialog.tsx:338`).
        .child(opencode_ui::scroller::scroller("connect-list").hidden(col().w_full().shrink().min_h(0.0).clip_fade(0.0, 0.0, 0.0, 40.0), list))
}

fn provider_row<H: ConnectHost + 'static>(p: &Pal, r: &Row, active: bool) -> El {
    let base = build_provider_row_shell(p, r, active);
    if r.disabled {
        return base;
    }
    let (id, hover_id) = (r.id.clone(), r.id.clone());
    base.on_hover_change(move |s: &mut H, inside, tx: &mut Tx| set_active_on_hover(s, &hover_id, inside, tx))
        .on_press_tx(move |s: &mut H, tx: &mut Tx| pick(s, &id, tx))
}

fn build_provider_row_shell(p: &Pal, r: &Row, active: bool) -> El {
    let ink = if r.disabled { p.faint } else { p.text };
    let mut e = row()
        .key(format!("provider-{}", r.id))
        .role("button")
        .label(r.name.clone())
        .w_full()
        .min_h(32.0)
        .py(6.0)
        .pl(8.0)
        .pr(6.0)
        .gap(8.0)
        .center_y()
        .radius(6.0)
        .focus_inset(p)
        .fill(if active { p.hover } else { Color::TRANSPARENT })
        .transition(Transition::none())
        .child(ui::provider_mark(&r.id, p.icon).opacity(if r.disabled { 0.5 } else { 1.0 }))
        .child(text(r.name.clone()).font(BODY).color(ink).shrink().min_w(0.0).truncate());
    if r.disabled {
        return e.child(text(opencode_i18n::t!("dialog.provider.environmentOnly")).font(BODY).color(p.faint).shrink().truncate());
    }
    if let Some(account) = &r.account {
        e = e.child(ui::badge(p, account.clone()).shrink().min_w(0.0));
    } else if let Some(note) = provider_note(&r.id) {
        e = e.child(text(note).key("provider-note").font(BODY).color(p.muted).shrink().min_w(0.0).truncate());
    }
    if r.needs_sign_in {
        e = e
            .child(spacer())
            .child(text(opencode_i18n::t!("dialog.provider.needsSignIn")).key("provider-status").font(BODY).color(p.warning).pr(4.0));
    }
    e
}
