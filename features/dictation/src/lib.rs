//! Dictation through Hex: the mic in the composer's toolbar, or ⇧⌘M, records and puts the words
//! at the caret, ready to edit. A press starts and a second press (or the stop) finishes;
//! holding ⇧⌘M past `HOLD` talks until it comes up. Esc throws the recording away. Send or ⏎
//! while it listens finishes and sends with the words, as the iOS app's Send does; otherwise
//! nothing is sent.
//!
//! It looks like the OpenCode iOS app's composer: while it listens the mic is a quiet stop beside
//! Send, which shows a spinner while the words come; "Preparing dictation…", the words so far,
//! and what went wrong sit under the toolbar. `hex_client::dictate` runs each dictation on its
//! own thread over whatever the running Hex offers (its capture API, on the microphone chosen in
//! Hex, with the words so far where its model streams; on a Windows Hex from before that API, a
//! live stream from the app's own microphone, which also sends the words so far). Hex pastes nothing,
//! so the composer keeps its focus. An installed Hex that isn't running starts on the press.
//!
//! The mic shows while Hex is there (on Windows always, so a press without Hex offers its
//! download). Headless runs, scenes, fixtures, and the storybook use a fake Hex unless `SEER_HEX`
//! names an endpoint file, so they never open the microphone.
//!
//! Where Hex is costs the UI thread nothing, as Hex's own SDK does it (`connect()` reads the
//! endpoint file when it connects): a press's thread finds Hex (`hex_client::dictate`). Only where
//! the mic hides without Hex (off Windows) does the window need to know sooner; there the
//! composer's first toolbar starts a watcher on Hex's folder, whose thread looks once and again
//! only when the folder changes.

#![forbid(unsafe_code)]

mod machine;
mod view;
mod watch;

use std::cell::{Cell, OnceCell};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc};

pub use hex_client::{Fix, Heard, Presence, Problem};
use hex_client::Cmd;
use rune::machine::{Attempt, Seq};
use rune::{Key, KeyEvent, TextEdit, Tx, Waker};

use machine::{Effect, Event, Phase, Rec, step};
pub use view::{status, toolbar};
use watch::Backend;

/// Who Hex says is recording.
const SOURCE: &str = "Brocode";
/// Holding the shortcut longer than this is push-to-talk: the recording ends when it comes up.
pub(crate) const HOLD: f64 = 0.3;
/// "Preparing dictation…" shows once Hex has taken this long to start, so a quick start shows
/// nothing but the stop.
pub(crate) const PREPARING_AFTER: f64 = 0.25;

/// The fake's transcription time.
const FAKE_TRANSCRIBE: f64 = 0.6;
/// How long the words may take after the stop. Hex transcribes on this computer, a long
/// dictation in a few seconds; past this it isn't coming: the words heard so far go into the
/// draft and the line says it didn't finish, rather than a spinner that never ends.
pub(crate) const FINISH_LIMIT: f64 = 30.0;
/// The fake's start when the storybook asks for a slow one.
const FAKE_SLOW_START: f64 = 1.2;
pub const FAKE_WORDS: &str = "keep the retry budget as it is and add a test for the reconnect";

/// Which submit shortcut ended a dictation recording.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SubmitKind {
    #[default]
    Default,
    Alternate,
    QueueFirst,
    InPlace,
}

/// Host state and callbacks required by Hex voice dictation in the composer.
pub trait DictationHost: opencode_kernel::AppEnv {
    fn dictation(&self) -> &Dictation;
    fn dictation_mut(&mut self) -> &mut Dictation;
    /// Mutable access to the composer text editor for inserting dictated words at the caret.
    fn composer_mut(&mut self) -> &mut TextEdit;
    /// Whether `ev` matches the host's configured Dictate shortcut (default ⇧⌘M).
    fn is_dictate_bind(&self, ev: &KeyEvent) -> bool;
    /// Keycap labels for the Dictate shortcut tooltip (e.g. `["⇧", "⌘", "M"]`).
    fn dictate_keycaps(&self) -> Vec<String> {
        Vec::new()
    }
    /// Hex's application support folder where `local-api.json` lives (`None` if unavailable).
    fn hex_support_dir(&self) -> Option<PathBuf>;
    /// Window waker used by the Hex presence watcher and dictation thread.
    fn dictation_waker(&self) -> Option<Waker>;
    /// Submits the composer prompt after dictation finishes when Send or Return ended recording.
    fn submit_prompt(&mut self, kind: SubmitKind, tx: &mut Tx);
}

pub(crate) fn resolve_backend(host: &dyn DictationHost) -> &Backend {
    host.dictation().backend.get_or_init(|| watch::detect(host))
}

fn ensure_backend(host: &mut dyn DictationHost) {
    if host.dictation().backend.get().is_none() {
        let backend = watch::detect(&*host);
        drop(host.dictation().backend.set(backend));
    }
    if matches!(host.dictation().backend.get(), Some(Backend::Fake)) && host.dictation().presence.is_none() {
        host.dictation_mut().presence = Some(Presence::Running);
    }
}

pub struct Dictation {
    pub(crate) backend: OnceCell<Backend>,
    /// Where Hex is, when known (`Backend::watch`).
    pub(crate) presence: Option<Presence>,
    /// This window's toolbar joined the watcher.
    pub(crate) watching: Cell<bool>,
    pub(crate) phase: Phase,
    /// What Hex's threads heard, tagged with their recording.
    heard: Arc<Mutex<Vec<(Attempt, Heard)>>>,
    pub(crate) seq: Seq,
    /// Seconds the fake takes to start.
    pub(crate) prepare: f64,
    /// The fake never sends its words (the storybook's "never finishes", `SEER_FAKE_STALL`).
    stall: bool,
}

impl Default for Dictation {
    fn default() -> Self {
        let stall = std::env::var_os("SEER_FAKE_STALL").is_some();
        Dictation {
            backend: OnceCell::new(),
            presence: None,
            watching: Cell::new(false),
            phase: Phase::Idle,
            heard: Arc::default(),
            seq: Seq::default(),
            prepare: 0.0,
            stall,
        }
    }
}

impl Dictation {
    fn with(backend: Backend) -> Dictation {
        let presence = matches!(backend, Backend::Fake).then_some(Presence::Running);
        let stall = std::env::var_os("SEER_FAKE_STALL").is_some();
        Dictation {
            backend: OnceCell::from(backend),
            presence,
            watching: Cell::new(false),
            phase: Phase::Idle,
            heard: Arc::default(),
            seq: Seq::default(),
            prepare: 0.0,
            stall,
        }
    }

    /// The storybook's Hex, whatever the app runs on.
    pub fn fake() -> Dictation {
        Self::with(Backend::Fake)
    }

    fn rec(&self) -> Option<&Rec> {
        match &self.phase {
            Phase::Starting { rec, .. } | Phase::Listening { rec, .. } | Phase::Finishing { rec, .. } => Some(rec),
            Phase::Idle | Phase::Failed(_) => None,
        }
    }

    /// The microphone is open (or opening): starting or listening.
    pub fn recording(&self) -> bool {
        matches!(self.phase, Phase::Starting { .. } | Phase::Listening { .. })
    }

    /// A recording is on: preparing, listening or finishing.
    pub fn active(&self) -> bool {
        self.rec().is_some()
    }

    pub fn error(&self) -> Option<&str> {
        match &self.phase {
            Phase::Failed(p) => Some(&p.says),
            _ => None,
        }
    }

    /// The mic shows while Hex is there, or always where a press offers its download.
    pub(crate) fn shows_mic(&self, backend: &Backend) -> bool {
        hex_client::MIC_WITHOUT_HEX
            || self.active()
            || matches!(backend, Backend::Fake)
            || self.presence.is_some_and(|p| p != Presence::Missing)
    }

    /// The fake starts slowly from now on, as a Hex loading its model would.
    pub fn slow_fake(&mut self) {
        self.prepare = FAKE_SLOW_START;
    }

    /// Configures how many seconds the fake backend takes to start (for storybook demos).
    pub fn set_fake_prepare(&mut self, secs: f64) {
        self.prepare = secs;
    }

    /// Configures whether the fake backend stalls instead of delivering final words (for storybook demos).
    pub fn set_fake_stall(&mut self, stall: bool) {
        self.stall = stall;
    }

    /// Enqueues fake `Heard` events for the current recording attempt (for storybook demos).
    pub fn push_fake_heard(&mut self, items: impl IntoIterator<Item = Heard>) {
        let attempt = self.seq.current();
        self.heard.lock().unwrap_or_else(|e| e.into_inner()).extend(items.into_iter().map(|h| (attempt, h)));
    }

    /// Cancels any active recording thread and transitions into `Failed(why)`.
    pub fn fail(&mut self, why: Problem) {
        if let (_, Some(Effect::Cancel(cmd))) = step(self, Event::Fail(why)) {
            let _ = cmd.send(Cmd::Cancel).ok();
        }
    }
}

pub fn start(host: &mut dyn DictationHost) {
    if host.dictation().active() {
        return;
    }
    let now = host.clock();
    run(host, Event::Toggle { now, held: false }, None);
}

/// Stops the microphone; the words land at the caret when Hex has them.
pub fn stop(host: &mut dyn DictationHost) {
    if host.dictation().recording() {
        let now = host.clock();
        run(host, Event::Toggle { now, held: false }, None);
    }
}

pub fn cancel(host: &mut dyn DictationHost) {
    run(host, Event::Escape, None);
}

/// Toggles dictation from a toolbar press (non-generic handler for the mic/stop button).
pub fn toggle(host: &mut dyn DictationHost, tx: &mut Tx) {
    tx.instant();
    let now = tx.clock();
    run(host, Event::Toggle { now, held: false }, Some(tx));
}

/// Clears a failed dictation error back to idle; true when an error was present.
pub fn dismiss_error(host: &mut dyn DictationHost) -> bool {
    if !matches!(host.dictation().phase, Phase::Failed(_)) {
        return false;
    }
    step(host.dictation_mut(), Event::DismissError).0
}

/// Runs the fix action offered by a failed dictation status line.
pub fn apply_fix(host: &mut dyn DictationHost, fix: Fix, tx: &mut Tx) {
    tx.instant();
    // The fake (storybook, headless) opens nothing.
    let support = match resolve_backend(&*host) {
        Backend::Hex { support, .. } => support.clone(),
        Backend::Fake => return,
    };
    match fix {
        Fix::Install => {
            harness_log::open_url(tx, hex_client::DOWNLOAD);
        }
        Fix::OpenHex => {
            if let Err(why) = hex_client::launch(&support, true) {
                host.dictation_mut().fail(Problem { says: why, fix: None });
            }
        }
        Fix::MicSettings => hex_client::open_mic_settings(),
    }
}

/// Steps the machine and carries out its effect; whether it took the event.
fn run(host: &mut dyn DictationHost, event: Event, tx: Option<&mut Tx>) -> bool {
    ensure_backend(host);
    let (took, effect) = step(host.dictation_mut(), event);
    match effect {
        Some(Effect::Start(attempt)) => {
            let wake = host.dictation_waker();
            let d = host.dictation_mut();
            if let Some(Backend::Hex { file, support }) = d.backend.get() {
                let (cmd, rx) = mpsc::channel();
                let (file, support, heard) = (file.clone(), support.clone(), d.heard.clone());
                let wakes = wake.is_some();
                std::thread::spawn(move || {
                    hex_client::dictate(&file, &support, SOURCE, &rx, &mut |h| {
                        heard.lock().unwrap_or_else(|e| e.into_inner()).push((attempt, h));
                        if let Some(wake) = &wake {
                            wake();
                        }
                    });
                });
                step(d, Event::ThreadStarted { attempt, cmd, wakes });
            }
        }
        Some(Effect::Stop) => {
            if let Some(cmd) = host.dictation().rec().and_then(Rec::cmd) {
                let _ = cmd.send(Cmd::Stop).ok();
            }
        }
        Some(Effect::Cancel(cmd)) => {
            let _ = cmd.send(Cmd::Cancel).ok();
        }
        Some(Effect::Words { words, send }) => {
            insert(host, &words);
            if let (Some(kind), Some(tx)) = (send, tx) {
                host.submit_prompt(kind, tx);
            }
        }
        Some(Effect::TimedOut { words, cancel }) => {
            // Hex's thread is told to stop; a late answer is fenced off by the recording's attempt.
            if let Some(cmd) = cancel {
                let _ = cmd.send(Cmd::Cancel).ok();
            }
            insert(host, &words);
        }
        None => {}
    }
    took
}

/// Send or ⏎ (`kind` for ⌘⏎ / Alt+⏎): while it listens, finishes and sends with the words, as the
/// iOS app's Send does; while the words come, waits for them. True when dictation took it.
pub fn submit(host: &mut dyn DictationHost, kind: SubmitKind) -> bool {
    if dismiss_error(host) {
        return false;
    }
    let now = host.clock();
    run(host, Event::Submit { now, kind }, None)
}

/// ⇧⌘M starts or finishes; while a recording is on, Esc throws it away.
pub fn key(host: &mut dyn DictationHost, ev: &KeyEvent, tx: &mut Tx) -> bool {
    if host.is_dictate_bind(ev) {
        if !tx.key_repeat() {
            tx.instant();
            let now = tx.clock();
            run(host, Event::Toggle { now, held: true }, Some(tx));
        }
        return true;
    }
    if ev.key == Key::Escape && host.dictation().active() {
        tx.instant();
        cancel(host);
        return true;
    }
    false
}

/// The shortcut's M coming up (with ⌘ still down, or after it): a hold past `HOLD` was
/// push-to-talk, so the recording finishes.
pub fn key_up(host: &mut dyn DictationHost, ev: &KeyEvent, tx: &mut Tx) -> bool {
    let m = matches!(&ev.key, Key::Command('m')) || matches!(&ev.key, Key::Text(t) if t.eq_ignore_ascii_case("m"));
    let held = matches!(
        host.dictation().phase,
        Phase::Starting { hold: Some(_), .. } | Phase::Listening { hold: Some(_), .. }
    );
    if !m || !held {
        return false;
    }
    let now = tx.clock();
    let was = host.dictation().recording();
    run(host, Event::Release { now }, Some(tx));
    if was && !host.dictation().recording() {
        tx.instant();
    }
    true
}

/// Each frame: looks for Hex, follows Hex's thread, and puts Hex's words at the caret.
pub fn poll(host: &mut dyn DictationHost, now: f64, tx: &mut Tx) -> bool {
    let mut changed = false;
    // The watcher's look, once the toolbar has started it: one atomic load, no file. Written
    // only when it moved, and nothing else is until a recording runs.
    if host.dictation().watching.get() {
        let presence = resolve_backend(&*host).watch(None);
        if presence != host.dictation().presence {
            host.dictation_mut().presence = presence;
            changed = true;
        }
    }
    if host.dictation().rec().is_none() {
        return changed;
    }
    let (attempt, since, fake, starting, prepare, wakes, words_due, gives_up, mut heard) = {
        let d = host.dictation();
        let Some(rec) = d.rec() else { return changed };
        let (attempt, since, fake, wakes) = (rec.attempt, rec.since, rec.is_fake(), rec.wakes());
        let heard: Vec<Heard> = d
            .heard
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .drain(..)
            .filter(|(a, _)| *a == attempt)
            .map(|(_, h)| h)
            .collect();
        let starting = matches!(d.phase, Phase::Starting { .. });
        // The fake answers on its own clock: ready after `prepare`, words `FAKE_TRANSCRIBE` after
        // the stop.
        let words_due = match &d.phase {
            Phase::Finishing { stopped, .. } if fake && !d.stall => Some(stopped + FAKE_TRANSCRIBE),
            _ => None,
        };
        let gives_up = match &d.phase {
            Phase::Finishing { stopped, .. } => Some(stopped + FINISH_LIMIT),
            _ => None,
        };
        (attempt, since, fake, starting, d.prepare, wakes, words_due, gives_up, heard)
    };
    if fake && starting && now - since >= prepare {
        heard.push(Heard::Ready);
    }
    if words_due.is_some_and(|t| now >= t) {
        let words = std::env::var_os("SEER_FAKE_WORDS")
            .and_then(|v| v.into_string().ok())
            .unwrap_or_else(|| FAKE_WORDS.into());
        heard.push(Heard::Words(words));
    }
    // On time for the slow-start line and the fake's next step; a Hex thread that can't wake
    // the window (a press before its first frame) is looked at again soon.
    let mut next = if wakes { f64::INFINITY } else { now + 0.1 };
    if starting {
        next = next.min(since + PREPARING_AFTER);
        if fake {
            next = next.min(since + prepare);
        }
    }
    if let Some(t) = words_due {
        next = next.min(t);
    }
    if let Some(t) = gives_up {
        next = next.min(t);
    }
    if next.is_finite() {
        tx.wake_at(next.max(now + 0.001));
    }
    if machine::tick_due(host.dictation(), now, heard.is_empty()) {
        changed |= run(host, Event::Tick { now, heard_empty: heard.is_empty() }, Some(tx));
    }
    for h in heard {
        changed = true;
        run(host, Event::Heard { attempt, heard: h }, Some(tx));
    }
    changed
}

/// The words at the caret, replacing a selection, with a space from the text either side.
fn insert(host: &mut dyn DictationHost, words: &str) {
    let words = words.trim();
    if words.is_empty() {
        return;
    }
    let c = host.composer_mut();
    let (start, end) = (c.anchor.min(c.focus), c.anchor.max(c.focus));
    let before = c.text[..start.min(c.text.len())].chars().next_back().is_some_and(|ch| !ch.is_whitespace());
    let after = c.text.get(end..).and_then(|t| t.chars().next()).is_some_and(|ch| !ch.is_whitespace());
    let mut put = String::new();
    if before {
        put.push(' ');
    }
    put.push_str(words);
    if after {
        put.push(' ');
    }
    c.insert(&put);
}
