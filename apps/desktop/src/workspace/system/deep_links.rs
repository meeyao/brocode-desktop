//! Links opened with the app (`App::on_open_url`), on the app's own scheme (`brocode://`,
//! see `launch::product`). OpenCode Desktop's `opencode://` stays OpenCode Desktop's: Rune never
//! registers it, so such a link opens OpenCode Desktop, or nothing when only Rune is installed.
//!
//! - `open-project?directory=<folder>` (and the older `open?project=<folder>`) asks the user to
//!   confirm opening `<folder>` before adding it as a project (`opencode#65803`,
//!   `packages/app/src/shell/deep-links/{machine.ts,deep-links.tsx}`);
//!   `new-session?directory=<folder>&prompt=<text>` does the same with the prompt typed in, not
//!   sent. Links are asked about one at a time in arrival order and only local absolute folders
//!   (no UNC/network shares, no `.`/`..`, no control/bidi characters) are accepted.
//! - `notification/<id>?…` is a clicked notification (`rune_notify::click_from_url`): its
//!   session opens, the app coming forward, also when the click started it.
//! - `console/authorized?window=<id>` is the OpenCode Console's return link after a sign-in:
//!   the window that started it comes forward (Desktop's `consoleReturnWindow`).
//! - `new-window`, `new-session` and `session?id=<id>` are the jump list's (`jump_list`): a new
//!   window, a new session in the window in front, a session on the app's own server.

use opencode_i18n::t;
use opencode_ui::palette::{Elevate as _, Pal, kit};
use opencode_ui::widgets::{BODY, DialogSize, Fit, button, dialog_window, sized};
use rune::machine::Effects;
use rune::{Cx, Direction, El, Key, KeyEvent, TextStyle, Tx, col, el, row, spacer, text};
use rune_kit::ButtonVariant;

use crate::reconnect::ServerId;
use crate::{App, Source};

/// Overlay stack identifier for the deep-link folder confirmation dialog.
pub const DEEP_LINK_OVERLAY_ID: &str = "dialog.deep_link_open";

const TITLE: TextStyle = TextStyle::new(15.0, 530.0).lh(20.0).tracking(-0.13);
const FOLDER_LABEL: TextStyle = TextStyle::new(11.0, 440.0).lh(16.0);
const FOLDER_CODE: TextStyle = TextStyle::new(12.0, 500.0).mono().lh(16.0);

/// A link waiting for confirmation or for its server: the folder an `open-project` / `new-session`
/// link names, on the server it opens on (the app's own: a folder on this computer).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Queued {
    pub server: ServerId,
    pub dir: String,
    /// `new-session`'s prompt, typed into the new session's composer, not sent.
    pub prompt: Option<String>,
}

/// Explicit confirmation phase for incoming project deep links (`deep-links/machine.ts:11-18`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeepLinkConfirm {
    Idle { attempt: u64 },
    Asking { attempt: u64, link: Queued, queued: Vec<Queued> },
}

impl Default for DeepLinkConfirm {
    fn default() -> Self {
        Self::Idle { attempt: 0 }
    }
}

/// Links the system handed the app: confirmed one at a time, then held until their server can
/// open them (`packages/app/src/shell/deep-links/machine.ts`, `opencode#65803`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DeepLinks {
    confirm: DeepLinkConfirm,
    /// Accepted links whose server isn't connected yet, oldest first.
    waiting: Vec<Queued>,
    /// A `new-session` link's prompt, until the session it opened shows its composer.
    prompt: Option<Pending>,
}

/// A `new-session` link's prompt on its way to the session the link opens.
#[derive(Clone, Debug, PartialEq)]
struct Pending {
    dir: String,
    prompt: String,
    /// When the link opened its project (`App::clock`).
    at: f64,
    /// The session on screen when it did: never the one the link opens, even in the same folder.
    before: Option<String>,
}

pub enum DeepLinkEvent {
    Received(Queued),
    Accepted { attempt: u64 },
    Declined { attempt: u64 },
    /// `server` is connected: its accepted links can open.
    Ready { server: ServerId },
}

#[derive(Debug, PartialEq, Eq)]
pub enum DeepLinkEffect {
    /// The window comes forward, as Desktop's `window.focus` command.
    Focus,
    /// Opens `dir` as a project with a new session on `server`, `prompt` typed into it.
    Open { server: ServerId, dir: String, prompt: Option<String> },
}

impl DeepLinks {
    pub fn step(&mut self, event: DeepLinkEvent, ready: impl Fn(&str) -> bool) -> Effects<DeepLinkEffect> {
        let mut effects = Effects::none();
        match event {
            DeepLinkEvent::Received(link) => {
                effects.push(DeepLinkEffect::Focus);
                match &mut self.confirm {
                    DeepLinkConfirm::Asking { queued, .. } => queued.push(link),
                    DeepLinkConfirm::Idle { attempt } => {
                        let next_attempt = attempt.saturating_add(1);
                        self.confirm = DeepLinkConfirm::Asking { attempt: next_attempt, link, queued: Vec::new() };
                    }
                }
                return effects;
            }
            DeepLinkEvent::Accepted { attempt } => {
                let current = std::mem::take(&mut self.confirm);
                match current {
                    DeepLinkConfirm::Asking { attempt: cur, link, queued } if cur == attempt => {
                        self.confirm = advance_confirm(attempt, queued);
                        self.waiting.push(link);
                    }
                    other => {
                        self.confirm = other;
                        return effects;
                    }
                }
            }
            DeepLinkEvent::Declined { attempt } => {
                let current = std::mem::take(&mut self.confirm);
                match current {
                    DeepLinkConfirm::Asking { attempt: cur, queued, .. } if cur == attempt => {
                        self.confirm = advance_confirm(attempt, queued);
                    }
                    other => self.confirm = other,
                }
                return effects;
            }
            DeepLinkEvent::Ready { .. } => {}
        }
        // In order, and one `new-session` prompt at a time: while one waits for its session, the
        // links behind it wait too, or the session the next one opens would take that prompt and
        // the next prompt would replace it.
        let queue = std::mem::take(&mut self.waiting);
        let mut held = self.prompt.is_some();
        let mut later = Vec::new();
        for q in queue {
            if held || !ready(&q.server) {
                later.push(q);
                continue;
            }
            held = q.prompt.is_some();
            effects.push(DeepLinkEffect::Open { server: q.server, dir: q.dir, prompt: q.prompt });
        }
        self.waiting = later;
        effects
    }

    pub fn is_waiting(&self) -> bool { !self.waiting.is_empty() }

    pub fn asking(&self) -> Option<(u64, &Queued)> {
        match &self.confirm {
            DeepLinkConfirm::Asking { attempt, link, .. } => Some((*attempt, link)),
            DeepLinkConfirm::Idle { .. } => None,
        }
    }
}

fn advance_confirm(attempt: u64, mut queued: Vec<Queued>) -> DeepLinkConfirm {
    if queued.is_empty() {
        DeepLinkConfirm::Idle { attempt }
    } else {
        let link = queued.remove(0);
        DeepLinkConfirm::Asking { attempt: attempt.saturating_add(1), link, queued }
    }
}

/// Whether the deep-link project confirmation dialog is currently shown.
pub(crate) fn asking(s: &App) -> bool {
    s.services.deep_links.asking().is_some()
}

/// Accepts the active deep-link confirmation attempt.
pub(crate) fn accept(s: &mut App, attempt: u64, tx: &mut Tx) {
    tx.instant();
    let ready = ready_servers(s);
    let effects = s
        .services
        .deep_links
        .step(DeepLinkEvent::Accepted { attempt }, |server| ready.iter().any(|r| r == server));
    let _ = run(s, effects, tx);
}

/// Declines the active deep-link confirmation attempt.
pub(crate) fn decline(s: &mut App, attempt: u64, tx: &mut Tx) {
    tx.instant();
    let effects = s.services.deep_links.step(DeepLinkEvent::Declined { attempt }, |_| false);
    let _ = run(s, effects, tx);
}

/// Declines whichever deep-link confirmation dialog is open, if any.
pub(crate) fn decline_current(s: &mut App, tx: &mut Tx) {
    if let Some((attempt, _)) = s.services.deep_links.asking() {
        decline(s, attempt, tx);
    }
}

/// Handles Escape while the deep-link confirmation dialog is open.
pub(crate) fn key(s: &mut App, ev: &KeyEvent, tx: &mut Tx) -> bool {
    if ev.key == Key::Escape && asking(s) {
        decline_current(s, tx);
        return true;
    }
    false
}

/// `DialogOpenDeepLink` (`packages/app/src/shell/deep-links/deep-links.tsx:96-139`, `opencode#65803`):
/// asks before opening a folder from an `brocode://` link, with Cancel focused by default so
/// a stray Enter can never open an untrusted folder.
pub(crate) fn dialog(s: &App, p: &Pal, cx: &Cx<App>) -> Vec<El> {
    let Some((attempt, link)) = s.services.deep_links.asking() else {
        return vec![];
    };
    let dir = link.dir.clone();
    let name = folder_name(&dir);
    let k = kit(p);
    // Cancel has autofocus so a link cannot open a folder on a stray Enter (`deep-links.tsx:129`).
    let cancel = button(t!("common.cancel"), ButtonVariant::Neutral)
        .on_press_tx(move |s: &mut App, tx: &mut Tx| decline(s, attempt, tx))
        .build(k)
        .key("deep-link-cancel")
        .autofocus();
    let open_btn = button(t!("deepLink.open.button"), ButtonVariant::Contrast)
        .on_press_tx(move |s: &mut App, tx: &mut Tx| accept(s, attempt, tx))
        .build(k)
        .key("deep-link-open");
    let title = t!("deepLink.open.title", name = name);
    let folder_badge = row()
        .key("deep-link-folder-code")
        .radius(4.0)
        .px(4.0)
        .py(2.0)
        .fill(p.text.alpha(0.08))
        .child(text(dir).font(FOLDER_CODE).color(p.text).dir(Direction::Ltr).wrap());
    let card = col()
        .key("deep-link-card")
        .role("dialog")
        .label(title.clone())
        .radius(6.0)
        .fill(p.layer1)
        .elevate(p.overlay)
        .clip_soft(0.0)
        .modal_tx(true, move |s: &mut App, tx: &mut Tx| decline(s, attempt, tx))
        .child(
            col()
                .key("deep-link-header")
                .w_full()
                .padding(16.0)
                .gap(8.0)
                .child(text(title).font(TITLE).color(p.text).wrap().w_full())
                .child(text(t!("deepLink.open.description")).font(BODY).color(p.muted).wrap().w_full())
                .child(
                    col()
                        .key("deep-link-folder")
                        .w_full()
                        .gap(4.0)
                        .child(text(t!("deepLink.open.folder")).font(FOLDER_LABEL).color(p.faint))
                        .child(folder_badge),
                )
                .child(text(t!("deepLink.open.warning")).font(BODY).color(p.muted).wrap().w_full()),
        )
        .child(row().key("deep-link-footer").w_full().padding(16.0).gap(8.0).child(spacer()).child(cancel).child(open_btn));
    let card = sized(card, DialogSize::Normal, Fit::Content, dialog_window(cx, *s.zoom));
    vec![
        el().key("deep-link-scrim").absolute(0.0, 0.0).w_full().h_full().fill(p.scrim),
        el().key("deep-link-modal").absolute(0.0, 0.0).w_full().h_full().center().child(card).raise(),
    ]
}

fn folder_name(dir: &str) -> String {
    let trimmed = dir.trim_end_matches(['/', '\\']);
    trimmed
        .rsplit(['/', '\\'])
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(dir)
        .to_owned()
}

/// `App::on_open_url`.
#[cfg(not(target_arch = "wasm32"))]
pub fn open(s: &mut App, url: &str, tx: &mut Tx) {
    if let Some(click) = rune_notify::click_from_url(url) {
        crate::notifications::clicked(s, &click);
        return;
    }
    if let Some(id) = console_return(url, crate::launch::product().scheme()) {
        match crate::windows::handle(&id) {
            Some(w) => w.focus(),
            None => rune_notify::bring_to_front(),
        }
        return;
    }
    match jump(url, crate::launch::product().scheme()) {
        Some(Jump::NewWindow) => {
            crate::windows::new_window(s);
            return;
        }
        Some(Jump::NewSession) => {
            rune_notify::bring_to_front();
            s.close_settings();
            s.new_session(tx);
            return;
        }
        Some(Jump::Session(id)) => {
            crate::notifications::clicked(s, &rune_notify::Click { id: String::new(), info: vec![("session".into(), id), ("server".into(), String::new())] });
            return;
        }
        None => {}
    }
    match (project(url, crate::launch::product().scheme()), server_of(s)) {
        (Some((dir, prompt)), Some(server)) => {
            let ready = ready_servers(s);
            let effects = s.services.deep_links.step(DeepLinkEvent::Received(Queued { server, dir, prompt }), |server| ready.iter().any(|r| r == server));
            run(s, effects, tx);
        }
        _ => eprintln!("brocode: ignored a link it doesn't open: {url}"),
    }
}

/// `App::on_open_url`: browsers deliver query parameters via `apps/web`, never OS URL schemes.
#[cfg(target_arch = "wasm32")]
pub fn open(_: &mut App, _: &str, _: &mut Tx) {}

/// Opens accepted links waiting for a server once it connects (from `poll`).
pub fn drain(s: &mut App, tx: &mut Tx) -> bool {
    let typed = type_prompt(s);
    if !s.services.deep_links.is_waiting() {
        return typed;
    }
    if let Some(p) = &s.services.deep_links.prompt {
        tx.wake_after(std::time::Duration::from_secs_f64((p.at + 30.0 - *s.clock).max(0.0) + 0.05));
    }
    let ready = ready_servers(s);
    let Some(server) = ready.first().cloned() else { return typed };
    let effects = s.services.deep_links.step(DeepLinkEvent::Ready { server }, |server| ready.iter().any(|r| r == server));
    typed | run(s, effects, tx)
}

fn run(s: &mut App, effects: Effects<DeepLinkEffect>, tx: &mut Tx) -> bool {
    let mut changed = false;
    for effect in effects {
        match effect {
            #[cfg(not(target_arch = "wasm32"))]
            DeepLinkEffect::Focus if s.live() => rune_notify::bring_to_front(),
            DeepLinkEffect::Focus => {}
            DeepLinkEffect::Open { server, dir, prompt } => {
                s.close_settings();
                crate::servers::runtime::switch(s, &opencode_kit::servers::ServerKey::new(&server));
                let before = match &*s.route {
                    crate::Route::Session { id, .. } => Some(id.clone()),
                    _ => None,
                };
                s.open_project_dir(&dir, true, tx);
                if let Some(prompt) = prompt {
                    s.services.deep_links.prompt = Some(Pending { dir, prompt, at: *s.clock, before });
                }
                changed = true;
            }
        }
    }
    changed
}

/// The server a link opens on: the app's own (a folder on this machine).
#[cfg(not(target_arch = "wasm32"))]
fn server_of(_: &App) -> Option<ServerId> {
    Some(opencode_kit::servers::ServerKey::SIDECAR.to_owned())
}

/// The servers the window has a half of that can take writes: a link waits only for its own
/// server, not for the one the window shows.
fn ready_servers(s: &mut App) -> Vec<ServerId> {
    let can_write = |s: &mut App| matches!(&*s.source, Source::Live { writes: Some(_), .. } | Source::Fixture);
    s.checkout.keys().into_iter().filter(|k| crate::servers::runtime::with_server(s, k, can_write).unwrap_or(false)).map(|k| k.as_str().to_owned()).collect()
}

/// A `new-session` link's prompt goes into the composer of the session it opened, once that
/// session shows (the project opens through the server first). Dropped after 30 s, or when the
/// composer already holds something.
fn type_prompt(s: &mut App) -> bool {
    let Some(Pending { dir, prompt, at, before }) = s.services.deep_links.prompt.clone() else { return false };
    if *s.clock - at > 30.0 {
        s.services.deep_links.prompt = None;
        return false;
    }
    let crate::Route::Session { id, .. } = &*s.route else { return false };
    let same = |a: &str, b: &str| a.trim_end_matches(['/', '\\']).eq_ignore_ascii_case(b.trim_end_matches(['/', '\\']));
    if before.as_deref() == Some(id.as_str()) || !s.session(id).is_some_and(|i| same(&i.directory, &dir)) {
        return false;
    }
    s.services.deep_links.prompt = None;
    if !s.composer.text.is_empty() {
        return false;
    }
    *s.composer = rune::TextEdit::new(prompt);
    true
}

/// An absolute folder on this computer: `/…` or `C:\…`. Rejects relative paths, `.`/`..`
/// segments, UNC/network shares (`\\host\share`, `//host/share`, `/net/<host>`), and control or
/// bidirectional characters (`packages/app/src/shell/deep-links/machine.ts:117-131`).
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn is_local_directory(directory: &str) -> bool {
    if directory.is_empty() {
        return false;
    }
    if directory.chars().any(|c| {
        c <= '\u{001f}'
            || c == '\u{007f}'
            || matches!(c, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
    }) {
        return false;
    }
    let bytes = directory.as_bytes();
    if bytes.len() >= 2 && matches!(bytes[0], b'/' | b'\\') && matches!(bytes[1], b'/' | b'\\') {
        return false;
    }
    let unix_abs = bytes[0] == b'/';
    let win_abs = bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && matches!(bytes[2], b'/' | b'\\');
    if !unix_abs && !win_abs {
        return false;
    }
    let mut segments = directory.split(['/', '\\']);
    let first = segments.next();
    let second = segments.next();
    if first.is_some_and(|s| s == "." || s == "..") || second.is_some_and(|s| s == "." || s == "..") {
        return false;
    }
    if segments.any(|s| s == "." || s == "..") {
        return false;
    }
    if unix_abs && second.is_some_and(|s| s.eq_ignore_ascii_case("net")) {
        return false;
    }
    true
}

/// The folder (and `new-session`'s prompt) a project link on the app's scheme `own` names:
/// `open-project?directory=`, `new-session?directory=&prompt=`, or the older `open?project=`.
#[cfg(not(target_arch = "wasm32"))]
fn project(url: &str, own: &str) -> Option<(String, Option<String>)> {
    let (scheme, rest) = url.trim().split_once("://")?;
    if !scheme.eq_ignore_ascii_case(own) {
        return None;
    }
    let (host, query) = rest.split_once('?')?;
    let param = |name: &str| query.split('&').find_map(|pair| pair.strip_prefix(name).and_then(|v| v.strip_prefix('='))).map(unpercent);
    let (dir, prompt) = match host.trim_end_matches('/') {
        "open" => (param("project")?, None),
        "open-project" => (param("directory")?, None),
        "new-session" => (param("directory")?, param("prompt").filter(|p| !p.is_empty())),
        _ => return None,
    };
    is_local_directory(&dir).then_some((dir, prompt))
}

/// The jump list's entries (`jump_list`), besides `open?project=`.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, PartialEq, Eq)]
enum Jump {
    NewWindow,
    NewSession,
    /// `session?id=<id>`: a session on the app's own server.
    Session(String),
}

#[cfg(not(target_arch = "wasm32"))]
fn jump(url: &str, own: &str) -> Option<Jump> {
    let (scheme, rest) = url.trim().split_once("://")?;
    if !scheme.eq_ignore_ascii_case(own) {
        return None;
    }
    let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
    match path.trim_end_matches('/') {
        "new-window" => Some(Jump::NewWindow),
        "new-session" if !query.split('&').any(|pair| pair.starts_with("directory=")) => Some(Jump::NewSession),
        "session" => {
            let id = query.split('&').find_map(|pair| pair.strip_prefix("id=")).map(unpercent)?;
            (!id.is_empty() && id.len() <= 256 && !id.chars().any(char::is_control)).then_some(Jump::Session(id))
        }
        _ => None,
    }
}

/// The window a `<own>://console/authorized?window=<id>` link names.
#[cfg(not(target_arch = "wasm32"))]
fn console_return(url: &str, own: &str) -> Option<String> {
    let (scheme, rest) = url.trim().split_once("://")?;
    if !scheme.eq_ignore_ascii_case(own) {
        return None;
    }
    let (path, query) = rest.split_once('?')?;
    if path != "console/authorized" {
        return None;
    }
    let id = query.split('&').find_map(|pair| pair.strip_prefix("window=")).map(unpercent)?;
    (!id.is_empty() && id.len() <= 256 && !id.chars().any(char::is_control)).then_some(id)
}

/// A query value: `%XX` escapes and `+` for a space, as `URLSearchParams` reads it.
#[cfg(not(target_arch = "wasm32"))]
fn unpercent(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = (b[i] == b'%' && i + 2 < b.len())
            .then(|| std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()))
            .flatten();
        match (b[i], hex) {
            (_, Some(v)) => {
                out.push(v);
                i += 3;
            }
            (b'+', None) => {
                out.push(b' ');
                i += 1;
            }
            (c, None) => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

mod scenes {
    use super::{DeepLinkEvent, Queued};
    use crate::App;

    fn open_deep_link_confirm(s: &mut App) {
        crate::scenes::open_scene(s);
        let server = opencode_kit::servers::ServerKey::SIDECAR.to_owned();
        let _ = s.services.deep_links.step(
            DeepLinkEvent::Received(Queued {
                server: server.clone(),
                dir: "/Users/dev/untrusted-repo".into(),
                prompt: None,
            }),
            |_| true,
        );
        let _ = s.services.deep_links.step(
            DeepLinkEvent::Received(Queued {
                server,
                dir: "/Users/dev/second-repo".into(),
                prompt: None,
            }),
            |_| true,
        );
    }

    rune_register::register!(crate::scenes::SCENE_LIST: crate::scenes::Scene = [("deep-link-confirm", open_deep_link_confirm)]);
}
