//! A provider's steps in the connect dialog (`ProviderSteps` in `providers/connect/dialog.tsx`):
//! the methods, a method's form, the API key and code entries, the browser and command waits,
//! failures, the accounts list, Claude Agent's setup, the Console's sign-in and the models to
//! start with.

mod accounts;
mod auth;
mod form;
mod models;

use opencode_ui::focus::FocusExt;
use rune::{El, TextStyle, Tx, col, icon, row, spacer, text};
use rune_kit::ButtonVariant;
use rune_theme_opencode::icons as ic;
use opencode_kit::integration::MethodKind;
use opencode_kit::model::Model;
use opencode_ui::palette::{Pal, kit};

use crate::catalog::{self, ProviderInfo};
use crate::dialog::{self, Body, CONSOLE, Dialog, Event, Frame, Step};
use crate::form::{Choice, Kind as FormKind};
use crate::machine::{self as conn, Event as Input, Screen, State as Conn};
use crate::ui::{self, BODY, BODY_MED};
use crate::ConnectHost;

/// Console device code digit (`console.tsx:34`, `#63829`): `font-mono text-[28px] font-[530]`.
const CODE_DIGIT: TextStyle = TextStyle::new(28.0, 530.0).mono().lh(32.0);

/// Which step the inputs belong to: a new one starts them empty.
pub fn inputs_id(d: &Dialog) -> String {
    let Some(p) = &d.provider else { return "picker".into() };
    let step = match &p.step {
        Step::Models { .. } => "models".to_owned(),
        Step::Accounts { .. } => "accounts".to_owned(),
        s => match s.connect() {
            Some(Conn::Form(_, f)) => format!("form:{}", f.key()),
            Some(c) => format!("{:?}:{}", conn::screen(c), conn::sel(c).map_or(0, |s| s.method)),
            None => String::new(),
        },
    };
    format!("{}:{}:{step}", d.session, p.id)
}

/// A text step's prefilled answer.
pub fn field_default(d: &Dialog) -> String {
    match d.provider.as_ref().and_then(|p| p.step.connect()) {
        Some(Conn::Form(_, f)) => match &f.step.kind {
            FormKind::Text { text, .. } => text.clone(),
            FormKind::Choice { choice: Choice::Custom { text }, .. } => text.clone(),
            _ => String::new(),
        },
        _ => String::new(),
    }
}

/// The providers a connection to `id` brings and their models, in the server's order.
pub fn connection_groups(host: &dyn ConnectHost, id: &str) -> Vec<(ProviderInfo, Vec<Model>)> {
    let models = crate::connection_models(host, id);
    let mut infos: Vec<ProviderInfo> = catalog::provider_infos(host, crate::connect_dir(host))
        .unwrap_or_default()
        .iter()
        .filter(|p| p.id == id || p.integration.as_deref() == Some(id))
        .cloned()
        .collect();
    if infos.is_empty() && models.iter().any(|m| m.provider == id) {
        infos.push(ProviderInfo { id: id.to_owned(), name: crate::provider_name(host, id), integration: None });
    }
    infos
        .into_iter()
        .map(|p| {
            let list = models.iter().filter(|m| m.provider == p.id).cloned().collect();
            (p, list)
        })
        .filter(|(_, m): &(_, Vec<_>)| !m.is_empty())
        .collect()
}

pub(super) fn input(host: &mut dyn ConnectHost, event: Input, tx: &mut Tx) {
    tx.instant();
    crate::connect_event(host, Event::Input(event), tx);
}

pub(super) fn open_url(host: &mut dyn ConnectHost, url: &str, tx: &mut Tx) {
    let _ = host.open_external(url, tx);
}

pub(super) fn step_text(p: &Pal, words: String) -> El {
    text(words).font(BODY).color(p.muted).wrap().w_full()
}

pub(super) fn current(host: &dyn ConnectHost) -> &Conn {
    host.connect()
        .dialog()
        .and_then(|o| o.dialog.provider.as_ref())
        .and_then(|p| p.step.connect())
        .unwrap_or(&Conn::Disposed)
}

pub(super) fn provider_id(host: &dyn ConnectHost) -> String {
    host.connect()
        .dialog()
        .and_then(|o| o.dialog.provider.as_ref())
        .map(|p| p.id.clone())
        .unwrap_or_default()
}

pub fn body<H: ConnectHost + 'static>(s: &H, p: &Pal, frame: Frame) -> El {
    let Some(open) = s.connect().dialog() else { return col() };
    let d = &open.dialog;
    let Some(provider) = d.provider.as_ref() else { return col() };
    let console = provider.id == CONSOLE;
    let kind = dialog::body(d);
    let is_methods = matches!(kind, Body::Screen(Screen::Methods));
    let content = match kind {
        Body::Models => return models::view(s, p, &provider.id),
        Body::Accounts => accounts::view(s, p),
        Body::Claude { checking, can_check, timed_out } => claude(s, p, checking, can_check, timed_out),
        Body::NoModels { pending, refreshing } => no_models::<H>(p, pending, refreshing),
        Body::ConsoleSignIn { copied } => col().px(8.0).child(console_sign_in(s, p, copied)),
        Body::Screen(Screen::Busy) => col().px(8.0).child(ui::status_row(p, "connect-busy", opencode_i18n::t!("provider.connect.status.inProgress"))),
        Body::Screen(Screen::EnvironmentOnly) => col().px(8.0).child(text(opencode_i18n::t!("dialog.provider.environmentOnly")).font(BODY).color(p.muted)),
        Body::Screen(Screen::Methods) => methods(s, p),
        Body::Screen(Screen::Form) => form::view(s, p),
        Body::Screen(Screen::Failed) => col().px(8.0).child(failed(s, p, console)),
        Body::Screen(Screen::Command) => auth::command(s, p),
        Body::Screen(Screen::Key) => auth::key(s, p, console),
        Body::Screen(Screen::Code) => auth::code(s, p),
        Body::Screen(Screen::Browser) => auth::browser(s, p),
        Body::Picker => col(),
    };
    // Every step fits its content and scrolls in it past the dialog's height
    // (`provider-connect-content`: `flex min-h-0 flex-1 flex-col overflow-y-auto`, `dialog.tsx`),
    // with a plain `overflow-y-auto` box's bar and `pb-2` for non-methods provider steps.
    let body = col().w_full().child(content.w_full());
    let pad = if console && frame != Frame::Accounts {
        16.0
    } else if frame == Frame::Provider && !is_methods {
        8.0
    } else {
        0.0
    };
    let inner = opencode_ui::scroller::scroller("connect-content").build::<H>(p, col().w_full().shrink().min_h(0.0).pb(pad), body);
    let (px, pb) = if frame == Frame::Accounts { (8.0, 8.0) } else { (8.0, if console { 0.0 } else { 8.0 }) };
    col().key("connect-body").w_full().grow(1.0).shrink().min_h(0.0).px(px).pb(pb).child(inner)
}

fn method_icon(kind: MethodKind) -> &'static rune::Icon {
    match kind {
        MethodKind::Oauth => &ic::SELECT_ELEMENT,
        MethodKind::Key | MethodKind::Env => &ic::KEYBOARD,
        MethodKind::Command => &ic::TERMINAL,
        MethodKind::External => &ic::SERVER,
    }
}

fn methods<H: ConnectHost + 'static>(s: &H, p: &Pal) -> El {
    let list = conn::catalog(current(s)).map(|c| c.methods.clone()).unwrap_or_default();
    let description = opencode_i18n::t!("provider.connect.methods.description", provider = crate::provider_name(s, &provider_id(s)));
    let rows = list.into_iter().enumerate().map(|(at, m)| {
        let title = m.title();
        let glyph = method_icon(m.kind);
        // `MethodSelection` (`dialog.tsx:844-859`, `#63829`): 32 pt row, method icon, bold label,
        // trailing chevron revealed on hover/focus, inset focus ring.
        row()
            .key(format!("connect-method-{at}"))
            .role("button")
            .label(title.clone())
            .w_full()
            .h(32.0)
            .px(8.0)
            .gap(8.0)
            .center_y()
            .radius(6.0)
            .hover_group()
            .hover(p.hover)
            .press(p.pressed)
            .focus_inset(p)
            .on_press_tx(move |s: &mut H, tx: &mut Tx| input(s, Input::MethodSelected(at), tx))
            .child(icon(glyph, 16.0).color(p.icon_muted))
            .child(text(title).font(BODY_MED).color(p.text).grow(1.0).shrink().min_w(0.0).truncate())
            .child(icon(&ic::CHEVRON_RIGHT, 16.0).mirror_in_rtl().color(p.icon_muted).group_opacity(0.0, 1.0))
    });
    col().key("connect-methods").child(step_text(p, description).px(8.0).pb(12.0)).children(rows)
}

/// `ErrorRow` (`dialog.tsx:903-916`, `#63829`): ban sign, error message, right-aligned retry.
fn failed<H: ConnectHost + 'static>(s: &H, p: &Pal, console: bool) -> El {
    let error = match current(s) {
        Conn::Failed(_, e, _) => e.clone(),
        _ => String::new(),
    };
    let retry_label = if console {
        opencode_i18n::t!("provider.connect.console.retry")
    } else {
        opencode_i18n::t!("common.retry")
    };
    let retry = ui::button(retry_label, ButtonVariant::Submit)
        .on_press_tx(|s: &mut H, tx: &mut Tx| input(s, Input::Retry, tx))
        .build(kit(p))
        .key("connect-retry");
    build_failed_layout(p, console, error, retry)
}

fn build_failed_layout(p: &Pal, console: bool, error: String, retry: El) -> El {
    let words = if console { error } else { opencode_i18n::t!("provider.connect.status.failed", error = error) };
    col()
        .key("connect-failed")
        .gap(20.0)
        .child(
            row()
                .w_full()
                .gap(8.0)
                .child(icon(&ic::CIRCLE_BAN_SIGN, 14.0).color(p.danger).mt(3.0))
                .child(text(words).role("alert").font(BODY).color(p.text).wrap().grow(1.0).shrink()),
        )
        .child(ui::step_actions().child(retry))
}

/// The Console's device sign-in (`console.tsx`).
fn console_sign_in<H: ConnectHost + 'static>(s: &H, p: &Pal, copied: Option<bool>) -> El {
    let c = current(s);
    let failed = matches!(c, Conn::Waiting(_, conn::Signin::Browser { failed: true, .. }));
    let code = auth::confirmation_code(c);
    let label = if copied == Some(true) {
        opencode_i18n::t!("provider.connect.console.linkCopied")
    } else {
        opencode_i18n::t!("provider.connect.console.copyLink")
    };
    let copy_btn = ui::button(label, ButtonVariant::Ghost)
        .disabled(code.is_none())
        .on_press_tx(|s: &mut H, tx: &mut Tx| copy_link(s, tx))
        .build(kit(p));
    build_console_sign_in_layout(p, failed, code, copied, copy_btn)
}

fn build_console_sign_in_layout(p: &Pal, failed: bool, code: Option<String>, copied: Option<bool>, copy_btn: El) -> El {
    let mut device = col().gap(8.0).child(text(opencode_i18n::t!("provider.connect.console.deviceCode")).font(BODY_MED).color(p.text));
    if let Some(code) = &code {
        let mut digits = row().dir(rune::Direction::Ltr).role("group").w_full().gap(6.0);
        for ch in code.chars() {
            digits = digits.child(if ch == '-' {
                row().h(64.0).mx(4.0).center_y().child(text("-").font(CODE_DIGIT).color(p.muted))
            } else {
                row()
                    .h(64.0)
                    .grow(1.0)
                    .shrink()
                    .min_w(0.0)
                    .center()
                    .radius(6.0)
                    .border(1.0, p.border)
                    .fill(p.layer2)
                    .child(text(ch.to_string()).font(CODE_DIGIT).color(p.text))
            });
        }
        device = device.child(digits.label(opencode_i18n::t!("provider.connect.console.deviceCode.label", code = code)));
    }
    let waiting = if code.is_some() {
        opencode_i18n::t!("provider.connect.console.waiting")
    } else {
        opencode_i18n::t!("provider.connect.console.openingBrowser")
    };
    let actions = row()
        .key("console-actions")
        .w_full()
        .gap(8.0)
        .center_y()
        .child(spacer())
        .child(copy_btn)
        .child(auth::wait_button(p, waiting));
    let lead = if failed {
        opencode_i18n::t!("provider.connect.console.browserFailed")
    } else {
        opencode_i18n::t!("provider.connect.console.instructions")
    };
    let mut e = col().key("console-authorization").gap(20.0).child(step_text(p, lead.into())).children(code.is_some().then_some(device));
    if copied == Some(false) {
        e = e.child(text(opencode_i18n::t!("provider.connect.console.copyFailed")).role("alert").font(BODY).color(p.muted));
    }
    e.child(actions)
}

fn copy_link(host: &mut dyn ConnectHost, tx: &mut Tx) {
    let Some((session, auth)) = host.connect().dialog().and_then(|o| {
        let auth = o.dialog.provider.as_ref().and_then(|p| p.step.connect()).and_then(conn::authorization).cloned()?;
        Some((o.dialog.session, auth))
    }) else {
        return;
    };
    tx.copy(auth.url);
    crate::connect_event(host, Event::LinkCopied { session, attempt_id: auth.attempt, copied: true }, tx);
}

fn no_models<H: ConnectHost + 'static>(p: &Pal, pending: bool, refreshing: bool) -> El {
    let open_again = (!pending).then(|| {
        ui::button(opencode_i18n::t!("provider.connect.console.openAgain"), ButtonVariant::Ghost)
            .on_press_tx(|s: &mut H, tx: &mut Tx| open_url(s, "https://apibrother.com/console", tx))
            .build(kit(p))
    });
    let refresh = ui::button(opencode_i18n::t!("provider.connect.console.refresh"), ButtonVariant::Submit)
        .disabled(refreshing)
        .on_press_tx(|s: &mut H, tx: &mut Tx| input(s, Input::Refresh, tx))
        .build(kit(p))
        .busy(refreshing);
    build_no_models_layout(p, pending, open_again, refresh)
}

fn build_no_models_layout(p: &Pal, pending: bool, open_again: Option<El>, refresh: El) -> El {
    let about = if pending {
        opencode_i18n::t!("provider.connect.console.modelsLoading")
    } else {
        opencode_i18n::t!("provider.connect.console.noModels")
    };
    let actions = ui::step_actions().children(open_again).child(refresh);
    col()
        .key("connect-no-models")
        .role("status")
        .px(8.0)
        .gap(20.0)
        .child(
            col()
                .child(
                    row()
                        .gap(8.0)
                        .center_y()
                        .child(icon(&ic::CIRCLE_CHECK, 16.0).color(p.text))
                        .child(text(opencode_i18n::t!("provider.connect.console.connected")).font(BODY_MED).color(p.text)),
                )
                .child(step_text(p, about.into())),
        )
        .child(actions)
}

fn claude<H: ConnectHost + 'static>(s: &H, p: &Pal, checking: bool, can_check: bool, timed_out: bool) -> El {
    let problem = catalog::integration_of(s, crate::connect_dir(s), dialog::CLAUDE)
        .and_then(|i| i.accounts().first().and_then(|a| a.status.clone()));
    let words = problem
        .as_ref()
        .map(|st| st.message.clone())
        .or_else(|| timed_out.then(|| opencode_i18n::t!("provider.connect.claudeAgent.timeout").to_owned()));
    let mut e = col()
        .key("claude-agent-setup")
        .px(8.0)
        .gap(20.0)
        .child(step_text(p, opencode_i18n::t!("provider.connect.claudeAgent.description").into()));
    if let Some(w) = words {
        e = e.child(
            row()
                .gap(8.0)
                .child(icon(&ic::CIRCLE_BAN_SIGN, 14.0).color(p.danger).mt(3.0))
                .child(text(w).role("alert").font(BODY).color(p.text).wrap().grow(1.0).shrink()),
        );
    }
    e = e.child(auth::copy_field::<H>(
        p,
        "claude-command",
        None,
        Some(opencode_i18n::t!("provider.connect.claudeAgent.signIn.command")),
        None,
        "claude auth login".into(),
    ));
    let again = crate::accounts_of(s, dialog::CLAUDE).count > 0;
    let url = problem.and_then(|st| st.url).unwrap_or_else(|| "https://code.claude.com/docs/en/setup".into());
    let install = ui::button(opencode_i18n::t!("provider.connect.claudeAgent.install"), ButtonVariant::Ghost)
        .on_press_tx(move |s: &mut H, tx: &mut Tx| open_url(s, &url, tx))
        .build(kit(p));
    let primary = if checking {
        auth::wait_button(p, opencode_i18n::t!("provider.connect.claudeAgent.checking")).key("claude-checking")
    } else {
        let submit_label = if again {
            opencode_i18n::t!("provider.connect.claudeAgent.checkAgain")
        } else {
            opencode_i18n::t!("provider.connect.claudeAgent.connect")
        };
        opencode_ui::widgets::not_allowed(ui::button(submit_label, ButtonVariant::Submit)
            .disabled(!can_check)
            .on_press_tx(|s: &mut H, tx: &mut Tx| trigger_claude_check(s, tx))
            .build(kit(p))
            .key("connect-submit"), !can_check)
    };
    e.child(ui::step_actions().child(install).child(primary))
}

fn trigger_claude_check(host: &mut dyn ConnectHost, tx: &mut Tx) {
    tx.instant();
    let replacing = catalog::integration_of(host, crate::connect_dir(host), dialog::CLAUDE)
        .map(|i| i.accounts().iter().map(|a| a.id.clone()).collect())
        .unwrap_or_default();
    crate::connect_event(host, Event::ClaudeCheck { replacing }, tx);
}
