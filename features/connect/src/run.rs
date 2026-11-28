//! The connect dialog's effects against the server (Desktop's `providers/connect/controller.ts`):
//! each request runs as a [`ConnectJob`] and its answer comes back as a [`ConnectReply`] applied
//! via [`apply_reply`]. A window that can't write answers a write with the read-only message
//! instead.

use std::time::Duration;

use opencode_kit::client::Server;
use opencode_kit::integration::{AttemptStatus, Method, MethodKind};
use rune::Tx;
use rune::machine::Attempt;
use serde_json::Value;

use crate::catalog::{self, CatalogRead, SyncAfter};
use crate::dialog::{CLAUDE, CLAUDE_CHECK_TIMEOUT_S, CONSOLE, Effect, Event, Snapshot};
use crate::machine::{Catalog, Effect as SignIn, Event as Answer, Outcome, Signin};
use crate::picker_reads::{self, PickerList, PickerRead};
use crate::ConnectHost;

/// How long the Console's workspace providers may take to show after a sign-in.
const CONSOLE_SETTLE_TRIES: u32 = 20;

/// A background server request, timer, or next-poll deferred reply scheduled by the host.
#[derive(Clone, Debug)]
pub enum ConnectJob {
    /// Run `request` against the live [`Server`] after `delay` (`Duration::ZERO` for immediate),
    /// then deliver the resulting [`ConnectReply`] via [`apply_reply`].
    Server { delay: Duration, request: ConnectRequest },
    /// Wait `delay` (without hitting the server) and deliver `reply` via [`apply_reply`].
    Timer { delay: Duration, reply: ConnectReply },
    /// Deliver `reply` on the next UI poll (e.g. a read-only write refusal or browser-open ack).
    Defer { reply: ConnectReply },
}

/// Off-thread server work executed by [`execute_request`].
#[derive(Clone, Debug)]
pub enum ConnectRequest {
    SyncCatalogs {
        dir: Option<String>,
        after: SyncAfter,
    },
    PickerRead {
        generation: Attempt,
        list: PickerList,
        read: PickerRead,
    },
    LoadIntegration {
        session: u32,
        provider: String,
        dir: Option<String>,
        attempt: u32,
    },
    StartSignIn {
        session: u32,
        provider: String,
        dir: Option<String>,
        attempt: u32,
        method_id: String,
        kind: MethodKind,
        answer: Option<Value>,
        window_id: String,
    },
    PollSignIn {
        session: u32,
        provider: String,
        dir: Option<String>,
        attempt: u32,
        signin: Signin,
    },
    PasteOauth {
        session: u32,
        provider: String,
        dir: Option<String>,
        attempt: u32,
        attempt_id: String,
        value: String,
    },
    CancelSignIn {
        provider: String,
        dir: Option<String>,
        command: bool,
        attempt_id: String,
    },
    SubmitEntry {
        session: u32,
        provider: String,
        dir: Option<String>,
        attempt: u32,
        signin: Signin,
        value: String,
        answer: Option<Value>,
    },
    AccountActivate {
        session: u32,
        attempt: u32,
        provider_name: String,
        account_label: String,
        account: String,
    },
    AccountRemove {
        session: u32,
        attempt: u32,
        provider_name: String,
        account_label: String,
        account: String,
    },
    AccountRename {
        dir: Option<String>,
        credential: String,
        label: String,
    },
    CredentialRemove {
        account: String,
    },
}

/// Result of a [`ConnectRequest`] or timer, applied on the UI thread by [`apply_reply`].
#[derive(Clone, Debug)]
pub enum ConnectReply {
    CatalogsSynced {
        dir: Option<String>,
        read: CatalogRead,
        after: SyncAfter,
    },
    PickerRead {
        generation: Attempt,
        list: PickerList,
        read: PickerRead,
        value: Option<Value>,
        client_error: bool,
    },
    DialogResult {
        session: u32,
        provider: String,
        event: Answer,
        catalog: Option<Snapshot>,
    },
    AccountActivated {
        session: u32,
        attempt: u32,
        provider_name: String,
        account_label: String,
        result: Result<(), String>,
    },
    AccountRemoved {
        session: u32,
        attempt: u32,
        provider_name: String,
        account_label: String,
        result: Result<(), String>,
    },
    AccountRenamed {
        dir: Option<String>,
        result: Result<(), String>,
    },
    ClaudeTimedOut {
        session: u32,
        check: u32,
    },
    RetryConsoleRefresh {
        dir: Option<String>,
        provider: String,
        session: u32,
        attempt: u32,
        tries: u32,
    },
    None,
}

pub(crate) fn read_only() -> String {
    opencode_i18n::t!("rune.jobs.readOnly").to_owned()
}

/// Executes a [`ConnectRequest`] off the UI thread against the live [`Server`].
pub fn execute_request(srv: &Server, request: ConnectRequest) -> ConnectReply {
    match request {
        ConnectRequest::SyncCatalogs { dir, after } => {
            let read = catalog::fetch_read(srv, dir.as_deref());
            ConnectReply::CatalogsSynced { dir, read, after }
        }
        ConnectRequest::PickerRead { generation, list, read } => picker_reads::execute(srv, generation, list, read),
        ConnectRequest::LoadIntegration { session, provider, dir, attempt } => {
            let catalog = match srv.integration(&provider, dir.as_deref()) {
                Ok(i) => Catalog { methods: i.connect_methods(), integration: Some(i) },
                // One the server can't describe can still take an API key.
                Err(_) => Catalog { integration: None, methods: vec![key_method()] },
            };
            ConnectReply::DialogResult {
                session,
                provider,
                event: Answer::Loaded { attempt, catalog, manual: false },
                catalog: None,
            }
        }
        ConnectRequest::StartSignIn { session, provider, dir, attempt, method_id, kind, answer, window_id } => {
            let console = provider == CONSOLE;
            let result = match kind {
                MethodKind::Command => srv.command_connect(&provider, dir.as_deref(), &method_id).map(|id| Answer::CommandStarted { attempt, id }),
                MethodKind::External => srv.connect_external(&provider, dir.as_deref(), &method_id, answer.as_ref()).map(|()| Answer::ExternalSaved { attempt }),
                _ => srv.oauth_connect(&provider, dir.as_deref(), &method_id, answer.as_ref()).map(|mut auth| {
                    if console {
                        // The Console names the desktop app, and its return link would focus the window
                        // that started the sign-in. That link is on OpenCode Desktop's scheme
                        // (`opencode://console/authorized?window=`), which Rune doesn't take, so it opens
                        // OpenCode Desktop if installed; the sign-in itself completes either way.
                        let ret = if window_id.is_empty() { String::new() } else { format!("&return_window={}", encode_query(&window_id)) };
                        let join = if auth.url.contains('?') { '&' } else { '?' };
                        auth.url = format!("{}{join}client_id=opencode-desktop{ret}", auth.url);
                    }
                    Answer::OauthStarted { attempt, auth }
                }),
            };
            let event = result.unwrap_or_else(|e| start_failed_event(attempt, console, kind, e));
            ConnectReply::DialogResult { session, provider, event, catalog: None }
        }
        ConnectRequest::PollSignIn { session, provider, dir, attempt, signin } => {
            let console = provider == CONSOLE;
            let outcome = poll(srv, &provider, dir.as_deref(), &signin, console);
            ConnectReply::DialogResult {
                session,
                provider,
                event: Answer::Polled { attempt, outcome },
                catalog: None,
            }
        }
        ConnectRequest::PasteOauth { session, provider, dir, attempt, attempt_id, value } => {
            let result = srv.oauth_complete(&provider, &attempt_id, dir.as_deref(), &value);
            ConnectReply::DialogResult {
                session,
                provider,
                event: Answer::PasteResult { attempt, result },
                catalog: None,
            }
        }
        ConnectRequest::CancelSignIn { provider, dir, command, attempt_id } => {
            if command {
                drop(srv.command_cancel(&provider, &attempt_id, dir.as_deref()));
            } else {
                drop(srv.oauth_cancel(&provider, &attempt_id, dir.as_deref()));
            }
            ConnectReply::None
        }
        ConnectRequest::SubmitEntry { session, provider, dir, attempt, signin, value, answer } => {
            let code = matches!(signin, Signin::Code { .. });
            let result = match &signin {
                Signin::Code { auth, .. } => srv.oauth_complete(&provider, &auth.attempt, dir.as_deref(), &value),
                _ => srv.connect_key(&provider, dir.as_deref(), &value, answer.as_ref()),
            };
            let event = submit_result_event(attempt, code, result);
            ConnectReply::DialogResult { session, provider, event, catalog: None }
        }
        ConnectRequest::AccountActivate { session, attempt, provider_name, account_label, account } => {
            let result = srv.credential_activate(&account);
            ConnectReply::AccountActivated { session, attempt, provider_name, account_label, result }
        }
        ConnectRequest::AccountRemove { session, attempt, provider_name, account_label, account } => {
            let result = srv.credential_remove(&account);
            ConnectReply::AccountRemoved { session, attempt, provider_name, account_label, result }
        }
        ConnectRequest::AccountRename { dir, credential, label } => {
            let result = srv.credential_rename(&credential, &label);
            ConnectReply::AccountRenamed { dir, result }
        }
        ConnectRequest::CredentialRemove { account } => {
            drop(srv.credential_remove(&account));
            ConnectReply::None
        }
    }
}

/// Applies a completed [`ConnectReply`] back onto `host` on the UI thread.
pub fn apply_reply(host: &mut dyn ConnectHost, reply: ConnectReply, tx: &mut Tx) {
    match reply {
        ConnectReply::CatalogsSynced { dir, read, after } => {
            let ok = catalog::apply_read(host, dir.as_deref(), read, tx);
            match after {
                SyncAfter::None => {}
                SyncAfter::AccountSettled { session, attempt } => {
                    crate::connect_event(host, Event::AccountSettled { session, attempt }, tx);
                }
                SyncAfter::ClaudeConfirm { session, check } => {
                    let problem = crate::accounts_of(host, CLAUDE).problem;
                    let has_models = crate::has_models(host, CLAUDE);
                    crate::connect_event(host, Event::ClaudeConfirmed { session, check, problem, has_models }, tx);
                }
                SyncAfter::RefreshConnection { dir, provider, session, attempt, tries } => {
                    finish_refresh_connection(host, dir, provider, session, attempt, tries, ok, tx);
                }
            }
        }
        ConnectReply::PickerRead { generation, list, read, value, client_error } => {
            picker_reads::apply(host, generation, list, read, value, client_error, tx);
        }
        ConnectReply::DialogResult { session, provider, event, catalog } => {
            crate::connect_event(host, Event::Result { session, provider, event, catalog }, tx);
        }
        ConnectReply::AccountActivated { session, attempt, provider_name, account_label, result } => {
            if result.is_ok() {
                host.toast_succeed(
                    opencode_i18n::t!("settings.providers.account.switched.title", provider = provider_name),
                    opencode_i18n::t!("settings.providers.account.switched.description", account = account_label),
                );
            }
            settle_account(host, session, attempt);
        }
        ConnectReply::AccountRemoved { session, attempt, provider_name, account_label, result } => {
            if result.is_ok() {
                host.toast_succeed(
                    format!("{account_label} removed"),
                    format!("{provider_name} will no longer use this account."),
                );
            }
            settle_account(host, session, attempt);
        }
        ConnectReply::AccountRenamed { dir, result } => {
            if let Err(e) = result {
                host.toast_fail(opencode_i18n::t!("common.requestFailed").to_owned(), e);
            }
            catalog::sync_catalogs(host, dir);
        }
        ConnectReply::ClaudeTimedOut { session, check } => {
            crate::connect_event(host, Event::ClaudeTimedOut { session, check }, tx);
        }
        ConnectReply::RetryConsoleRefresh { dir, provider, session, attempt, tries } => {
            refresh_connection(host, dir, provider, session, attempt, Some(tries));
        }
        ConnectReply::None => {}
    }
}

pub(crate) fn run_effect(host: &mut dyn ConnectHost, effect: Effect, scope: Option<&str>, tx: &mut Tx) {
    match effect {
        Effect::Connect { session, provider, effect } => run_sign_in(host, session, provider, *effect, tx),
        Effect::AccountActivate { session, attempt, provider, account } => {
            let provider_name = crate::provider_name(host, &provider);
            let account_label = account_label(host, &provider, &account);
            if !host.can_write() {
                host.spawn_job(ConnectJob::Defer {
                    reply: ConnectReply::AccountActivated {
                        session,
                        attempt,
                        provider_name,
                        account_label,
                        result: Err(read_only()),
                    },
                });
                return;
            }
            host.spawn_job(ConnectJob::Server {
                delay: Duration::ZERO,
                request: ConnectRequest::AccountActivate { session, attempt, provider_name, account_label, account },
            });
        }
        Effect::AccountRemove { session, attempt, provider, account } => {
            let provider_name = crate::provider_name(host, &provider);
            let account_label = account_label(host, &provider, &account);
            if !host.can_write() {
                host.spawn_job(ConnectJob::Defer {
                    reply: ConnectReply::AccountRemoved {
                        session,
                        attempt,
                        provider_name,
                        account_label,
                        result: Err(read_only()),
                    },
                });
                return;
            }
            host.spawn_job(ConnectJob::Server {
                delay: Duration::ZERO,
                request: ConnectRequest::AccountRemove { session, attempt, provider_name, account_label, account },
            });
        }
        Effect::ClaudeReplace(ids) => {
            if !host.can_write() {
                return;
            }
            for account in ids {
                host.spawn_job(ConnectJob::Server {
                    delay: Duration::ZERO,
                    request: ConnectRequest::CredentialRemove { account },
                });
            }
        }
        Effect::ClaudeTimer { session, check } => {
            host.spawn_job(ConnectJob::Timer {
                delay: Duration::from_secs_f64(CLAUDE_CHECK_TIMEOUT_S),
                reply: ConnectReply::ClaudeTimedOut { session, check },
            });
        }
        Effect::ClaudeConfirm { session, check } => {
            let dir = crate::connect_dir(host).map(str::to_owned);
            catalog::sync_catalogs_with(host, dir, SyncAfter::ClaudeConfirm { session, check });
        }
        Effect::Reveal(provider) => {
            let keys: Vec<String> = crate::connection_models(host, &provider).into_iter().map(|m| m.key).collect();
            host.reveal_models(&keys);
        }
        Effect::Connected(_) => {}
        Effect::Toast(provider) => {
            let name = crate::provider_name(host, &provider);
            let description = if crate::connection_models(host, &provider).is_empty() {
                String::new()
            } else {
                format!("{name} models are now available to use.")
            };
            host.toast_succeed(format!("{name} connected"), description);
        }
        Effect::Apply(key) => host.apply_connected_model(&key, scope, tx),
        Effect::Close => crate::close(host, tx),
    }
}

fn settle_account(host: &mut dyn ConnectHost, session: u32, attempt: u32) {
    let dir = crate::connect_dir(host).map(str::to_owned);
    catalog::sync_catalogs_with(host, dir, SyncAfter::AccountSettled { session, attempt });
}

fn run_sign_in(host: &mut dyn ConnectHost, session: u32, provider: String, effect: SignIn, tx: &mut Tx) {
    let dir = crate::connect_dir(host).map(str::to_owned);
    let console = provider == CONSOLE;
    match effect {
        SignIn::Load { attempt } => host.spawn_job(ConnectJob::Server {
            delay: Duration::ZERO,
            request: ConnectRequest::LoadIntegration { session, provider, dir, attempt },
        }),
        SignIn::Start { attempt, method, answer } => {
            let answer = answer.map(Value::Object);
            let method_id = method.id.unwrap_or_default();
            let kind = method.kind;
            if !host.can_write() {
                let event = start_failed_event(attempt, console, kind, read_only());
                host.spawn_job(ConnectJob::Defer {
                    reply: ConnectReply::DialogResult { session, provider, event, catalog: None },
                });
                return;
            }
            let window_id = host.window_id().to_owned();
            host.spawn_job(ConnectJob::Server {
                delay: Duration::ZERO,
                request: ConnectRequest::StartSignIn { session, provider, dir, attempt, method_id, kind, answer, window_id },
            });
        }
        SignIn::OpenBrowser { attempt_id, url } => {
            let opened = !host.is_headless() && host.open_external(&url, tx);
            host.spawn_job(ConnectJob::Defer {
                reply: ConnectReply::DialogResult {
                    session,
                    provider,
                    event: Answer::BrowserOpened { attempt_id, opened },
                    catalog: None,
                },
            });
        }
        SignIn::FormOpenBrowser { attempt, browser, url } => {
            let opened = !host.is_headless() && host.open_external(&url, tx);
            host.spawn_job(ConnectJob::Defer {
                reply: ConnectReply::DialogResult {
                    session,
                    provider,
                    event: Answer::FormBrowserOpened { attempt, browser, opened },
                    catalog: None,
                },
            });
        }
        SignIn::Poll { attempt, signin, delay } => {
            let command = matches!(signin, Signin::Command { .. });
            let every = if command || console { 0.5 } else { 1.0 };
            host.spawn_job(ConnectJob::Server {
                delay: Duration::from_secs_f64(if delay { every } else { 0.0 }),
                request: ConnectRequest::PollSignIn { session, provider, dir, attempt, signin },
            });
        }
        SignIn::PollStop | SignIn::Complete { .. } => {}
        SignIn::Paste { attempt, attempt_id, value } => {
            if !host.can_write() {
                host.spawn_job(ConnectJob::Defer {
                    reply: ConnectReply::DialogResult {
                        session,
                        provider,
                        event: Answer::PasteResult { attempt, result: Err(read_only()) },
                        catalog: None,
                    },
                });
                return;
            }
            host.spawn_job(ConnectJob::Server {
                delay: Duration::ZERO,
                request: ConnectRequest::PasteOauth { session, provider, dir, attempt, attempt_id, value },
            });
        }
        SignIn::Cancel { command, attempt_id } => {
            if !host.can_write() {
                return;
            }
            host.spawn_job(ConnectJob::Server {
                delay: Duration::ZERO,
                request: ConnectRequest::CancelSignIn { provider, dir, command, attempt_id },
            });
        }
        SignIn::Submit { attempt, signin, value, answer } => {
            let code = matches!(signin, Signin::Code { .. });
            let answer = answer.map(Value::Object);
            if !host.can_write() {
                let event = submit_result_event(attempt, code, Err(read_only()));
                host.spawn_job(ConnectJob::Defer {
                    reply: ConnectReply::DialogResult { session, provider, event, catalog: None },
                });
                return;
            }
            host.spawn_job(ConnectJob::Server {
                delay: Duration::ZERO,
                request: ConnectRequest::SubmitEntry { session, provider, dir, attempt, signin, value, answer },
            });
        }
        SignIn::Refresh { attempt, method } => {
            let wait = console && method.kind != MethodKind::Key;
            refresh_connection(host, dir, provider, session, attempt, wait.then_some(CONSOLE_SETTLE_TRIES));
        }
        SignIn::Sync => catalog::sync_catalogs(host, dir),
    }
}

/// Reads the catalogs after a sign-in. The Console's workspace providers can land after its
/// grant, so its refresh (`tries`) reads again until they show or the tries run out.
fn refresh_connection(host: &mut dyn ConnectHost, dir: Option<String>, provider: String, session: u32, attempt: u32, tries: Option<u32>) {
    catalog::sync_catalogs_with(
        host,
        dir.clone(),
        SyncAfter::RefreshConnection { dir, provider, session, attempt, tries },
    );
}

#[allow(clippy::too_many_arguments)]
fn finish_refresh_connection(
    host: &mut dyn ConnectHost,
    dir: Option<String>,
    provider: String,
    session: u32,
    attempt: u32,
    tries: Option<u32>,
    ok: bool,
    tx: &mut Tx,
) {
    if provider == CONSOLE && !ok {
        let error = opencode_i18n::t!("provider.connect.console.refreshFailed").into();
        crate::connect_event(
            host,
            Event::Result { session, provider, event: Answer::RefreshFailed { attempt, error }, catalog: None },
            tx,
        );
        return;
    }
    if let Some(n) = tries.filter(|n| *n > 0)
        && !console_loaded(host)
    {
        host.spawn_job(ConnectJob::Timer {
            delay: Duration::from_millis(500),
            reply: ConnectReply::RetryConsoleRefresh { dir, provider, session, attempt, tries: n - 1 },
        });
        return;
    }
    let catalog = Snapshot { pending: tries == Some(0) && !console_loaded(host), has_models: crate::has_models(host, &provider) };
    crate::connect_event(
        host,
        Event::Result { session, provider, event: Answer::RefreshSucceeded { attempt }, catalog: Some(catalog) },
        tx,
    );
}

/// A signed-in Console workspace lists its own providers beside the free `opencode` one.
fn console_loaded(host: &dyn ConnectHost) -> bool {
    catalog::provider_infos(host, crate::connect_dir(host))
        .unwrap_or_default()
        .iter()
        .any(|p| p.integration.as_deref() == Some(CONSOLE) && p.id != CONSOLE)
}

fn account_label(host: &dyn ConnectHost, provider: &str, account: &str) -> String {
    catalog::integration_of(host, crate::connect_dir(host), provider)
        .and_then(|i| i.connections.iter().find(|c| c.id == account).map(|c| c.label.clone()))
        .unwrap_or_else(|| account.to_owned())
}

fn start_failed_event(attempt: u32, console: bool, kind: MethodKind, error: String) -> Answer {
    Answer::StartFailed {
        attempt,
        error: if console && kind == MethodKind::Oauth {
            opencode_i18n::t!("provider.connect.console.startFailed").into()
        } else {
            error
        },
    }
}

fn submit_result_event(attempt: u32, code: bool, result: Result<(), String>) -> Answer {
    match result {
        Ok(()) => Answer::SubmitSucceeded { attempt },
        Err(e) if e.is_empty() && code => Answer::SubmitFailed { attempt, error: opencode_i18n::t!("provider.connect.oauth.code.invalid").into() },
        Err(error) => Answer::SubmitFailed { attempt, error },
    }
}

fn key_method() -> Method {
    Method {
        kind: MethodKind::Key,
        id: None,
        label: Some(opencode_i18n::t!("provider.connect.method.apiKey").into()),
        form: vec![],
    }
}

/// Percent-encodes a query parameter's value.
fn encode_query(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// One status read of a browser sign-in or a command, in the words the dialog shows.
fn poll(srv: &Server, provider: &str, dir: Option<&str>, signin: &Signin, console: bool) -> Outcome {
    if let Signin::Command { attempt, .. } = signin {
        return match srv.command_status(provider, attempt, dir) {
            Ok(AttemptStatus::Pending(output)) => Outcome::Pending(output),
            Ok(AttemptStatus::Complete) => Outcome::Complete,
            Ok(AttemptStatus::Failed(message)) => Outcome::Failed(message),
            Ok(AttemptStatus::Expired) => Outcome::Failed(opencode_i18n::t!("provider.connect.command.expired").into()),
            Err(e) => Outcome::Failed(e),
        };
    }
    let attempt = match signin {
        Signin::Browser { auth, .. } | Signin::Code { auth, .. } => auth.attempt.clone(),
        _ => return Outcome::Failed(String::new()),
    };
    let expired = if console {
        opencode_i18n::t!("provider.connect.console.expired")
    } else {
        opencode_i18n::t!("provider.connect.oauth.expired")
    };
    match srv.oauth_status(provider, &attempt, dir) {
        Ok(AttemptStatus::Pending(_)) => Outcome::Pending(String::new()),
        Ok(AttemptStatus::Complete) => Outcome::Complete,
        Ok(AttemptStatus::Expired) => Outcome::Failed(expired.into()),
        Ok(AttemptStatus::Failed(m)) if console && m.contains("expired_token") => Outcome::Failed(expired.into()),
        Ok(AttemptStatus::Failed(m)) if console && m.contains("access_denied") => {
            Outcome::Failed(opencode_i18n::t!("provider.connect.console.denied").into())
        }
        Ok(AttemptStatus::Failed(m)) => Outcome::Failed(m),
        Err(_) if console => Outcome::Unreachable(opencode_i18n::t!("provider.connect.console.statusFailed").into()),
        Err(e) => Outcome::Unreachable(e),
    }
}
