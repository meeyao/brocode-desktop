//! A lost link after the app has connected: the event stream dropped, went silent past
//! [`STREAM_IDLE`], or a read or write failed on the way (a timeout, a refusal). Everything stays
//! on screen; once it has lasted [`QUIET`] a quiet session header label says so, and it leaves
//! the moment the server answers again (`Incoming::Reconnected` also resyncs). The stream thread
//! in `opencode_kit::client` retries on its own with backoff.
//!
//! One [`Link`] per server (`opencode_kit::servers::link`), many at once (`Links`), as OpenCode
//! Desktop keeps one event stream and one sync state per server
//! (`packages/app/src/runtime/server/server-sync/machine.ts`: `idle | disconnected |
//! awaiting-handshake | live`, and `packages/client/src/solid/connection.ts`). What the stream,
//! the other calls, the launch, the health probe and the OS say steps it; the header reads it.
//! Desktop shows nothing while a server is away; the header label is Kit's (`motion_gate` ALLOW).

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use opencode_kit::client::Request;
use opencode_kit::model::{self, Message, Part, ToolStatus};
use opencode_kit::servers::link::{Event, Link};
use rune::Tx;

use crate::{App, Route, Source};

/// A short drop says nothing; the stream thread retries after 1 s (official `reconnectDelay`).
pub const QUIET: f64 = opencode_kit::servers::link::QUIET;
/// The event stream is reopened after this long with nothing on it, matching official
/// `defaultIdleTimeout = 45_000` (`packages/client/src/solid/connection.ts:35`). The server
/// writes a keepalive every 15 s, so a healthy remote link tolerates two missed heartbeats.
pub const STREAM_IDLE: Duration = Duration::from_secs(45);
/// Backoff range for retrying a failed `GET /api/session/active` read (`ActiveSync`).
const ACTIVE_RETRY_MIN: f64 = 0.25;
const ACTIVE_RETRY_MAX: f64 = 4.0;

/// Sync state of `GET /api/session/active` for one server
/// (`packages/client/src/solid/data.ts:783-807`).
#[derive(Default)]
pub struct ActiveSync {
    /// Whether an authoritative `/api/session/active` snapshot has landed for this connection.
    /// False on cold open (until boot's `active` read or `Incoming::Active` succeeds) and reset
    /// to false on reconnect until the fresh snapshot lands.
    synced: bool,
    /// SSE execution lifecycle deltas (`true` = started, `false` = ended) received while a
    /// `Request::Active` read is in flight, overlaid onto the snapshot so live events win.
    in_flight: Option<HashMap<String, bool>>,
    /// Monotonic app-clock deadline for retrying a failed `Request::Active`.
    retry_at: Option<f64>,
    /// Consecutive failed `Request::Active` attempts since the last success or reconnect.
    attempts: u32,
}

impl ActiveSync {
    pub fn synced(&self) -> bool {
        self.synced
    }

    pub fn mark_boot_synced(&mut self) {
        self.synced = true;
        self.retry_at = None;
        self.attempts = 0;
    }

    pub fn absorb(&mut self, from: ActiveSync) {
        if !self.synced && from.synced {
            self.synced = true;
        }
    }

    fn start(&mut self, reset_synced: bool) {
        if reset_synced {
            self.synced = false;
            self.attempts = 0;
        }
        self.in_flight = Some(HashMap::new());
        self.retry_at = None;
    }

    pub fn note_execution(&mut self, sid: &str, started: bool) {
        if let Some(updates) = &mut self.in_flight {
            updates.insert(sid.to_owned(), started);
        }
    }

    pub fn landed(&mut self) -> HashMap<String, bool> {
        self.synced = true;
        self.retry_at = None;
        self.attempts = 0;
        self.in_flight.take().unwrap_or_default()
    }

    fn failed(&mut self, now: f64) -> f64 {
        self.in_flight = None;
        let delay = (ACTIVE_RETRY_MIN * f64::from(1u32 << self.attempts.min(4))).min(ACTIVE_RETRY_MAX);
        self.attempts = self.attempts.saturating_add(1);
        self.retry_at = Some(now + delay);
        delay
    }

    fn take_due(&mut self, now: f64) -> Option<f64> {
        let due = self.retry_at?;
        if due <= now + 1e-9 {
            self.retry_at = None;
            Some(0.0)
        } else {
            Some(due - now)
        }
    }
}

/// Requests `/api/session/active` and begins tracking concurrent SSE execution deltas so live
/// transitions arriving mid-request supersede the returned snapshot (`data.ts:783-807`).
pub fn request_active(s: &mut App, reset_synced: bool) {
    if let Source::Live { requests: Some(r), .. } = &*s.source {
        s.stops.active_sync.start(reset_synced);
        r.send(Request::Active).ok();
    } else if reset_synced {
        s.stops.active_sync.start(true);
    }
}

/// Records a transient `Request::Active` failure and schedules an exponential-backoff retry.
pub fn active_failed(s: &mut App, e: &opencode_kit::wire_error::WireError, tx: &mut Tx) {
    call_failed(s, e, tx);
    let delay = s.stops.active_sync.failed(*s.clock);
    tx.wake_after(Duration::from_secs_f64(delay));
}

/// A server's key (`opencode_kit::servers::ServerKey`'s text: `"sidecar"`, an address, `wsl:…`).
pub type ServerId = String;

/// After a reconnect, the subagents read again: their step logs give way to the fresh page when
/// it lands, and the next list of running sessions may name subagents that started meanwhile.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CatchUp {
    #[default]
    Idle,
    /// The next list of running sessions is read for subagents that started while away.
    AwaitingActive,
}

/// Every server's link (`opencode_kit::servers::link`), many at once, and the catch-up after a
/// reconnect.
#[derive(Default)]
pub struct Links {
    /// Each server's link that isn't online; one not listed is.
    states: HashMap<ServerId, Link>,
    /// What the launch and the health probe last said of each server, as the link last heard it.
    heard: HashMap<ServerId, Probe>,
    catch_up: HashMap<ServerId, CatchUp>,
    /// Subagents whose pages were asked for again after a reconnect, by server.
    refreshing: HashMap<ServerId, HashSet<String>>,
}

impl Links {
    /// Server `server`'s link as the header tells it.
    pub fn state(&self, server: &str) -> Link {
        self.states.get(server).cloned().unwrap_or(Link::Online)
    }

    /// A server that leaves this window forgets its link.
    pub fn forget(&mut self, server: &str) {
        self.states.remove(server);
        self.heard.remove(server);
        self.catch_up.remove(server);
        self.refreshing.remove(server);
    }
}

/// The server the window is checked out on (`servers::runtime`): the key the stream's lines and
/// replies it takes now come under.
fn server_of(s: &App) -> Option<ServerId> {
    crate::launch::window_server(s)
}

/// Steps server `server`'s link (`opencode_kit::servers::link`) and carries out what it asks.
pub fn step_link(s: &mut App, server: &str, event: opencode_kit::servers::link::Event, tx: &mut Tx) -> bool {
    use opencode_kit::servers::link::Effect;
    let mut state = s.services.server_links.state(server);
    let was = state.dropped();
    let opened = matches!(event, Event::StreamUp);
    let effects = state.step(event);
    let dropped = state.dropped();
    if state == Link::Online {
        s.services.server_links.states.remove(server);
    } else {
        s.services.server_links.states.insert(server.to_owned(), state);
    }
    // The stream opening says the server's alive for as long as it stays up (its heartbeat), so
    // the health poll leaves it be; its dropping is the first word that the server went, and the
    // poll asks at once and again every 10 s.
    if opened || was != dropped {
        crate::servers::health::streaming(s, &opencode_kit::servers::ServerKey::new(server), !dropped, tx);
    }
    let mut changed = false;
    for effect in effects {
        match effect {
            Effect::Repaint => {
                tx.instant();
                changed = true;
            }
            Effect::WakeAt(at) => tx.wake_after(Duration::from_secs_f64((at - *s.clock).max(0.0))),
            Effect::Reconnect => {
                opencode_kit::client::Server::wake_reconnects();
                crate::servers::health::nudge(s, &opencode_kit::servers::ServerKey::new(server), tx);
            }
        }
    }
    changed
}

/// Server `server`'s link as the header tells it.
pub fn link_of(s: &App, server: &str) -> Link {
    s.services.server_links.state(server)
}

/// The event stream dropped or failed to open (`Incoming::StreamDown`), with its typed error.
pub fn stream_down(s: &mut App, e: opencode_kit::wire_error::WireError, tx: &mut Tx) {
    let Some(server) = server_of(s) else { return };
    let now = *s.clock;
    // What was mid-text when it dropped grows again only from whole text (the barrier).
    if !s.transcripts.is_empty() {
        let gaps = &mut s.kernel.slots.get_mut(&crate::events::STREAM_GAPS).0;
        for (sid, t) in s.transcripts.iter() {
            gaps.entry(sid.clone()).or_default().extend(t.open_steps());
        }
    }
    *s.diag.status = e.to_string();
    step_link(s, &server, Event::StreamDown { now, error: Some(e) }, tx);
}

/// After a network change or a wake, the server didn't answer a check (`client::streams`):
/// offline now, said at once.
pub fn unanswered(s: &mut App, e: opencode_kit::wire_error::WireError, tx: &mut Tx) {
    let Some(server) = server_of(s) else { return };
    let now = *s.clock;
    *s.diag.status = e.to_string();
    step_link(s, &server, Event::Unanswered { now, error: Some(e) }, tx);
}

/// Another call failed: one that failed on the link itself (no connection, no answer in time,
/// dropped), not on the server's answer, makes the link suspect.
pub fn call_failed(s: &mut App, e: &opencode_kit::wire_error::WireError, tx: &mut Tx) {
    use opencode_kit::wire_error::WireKind;
    let Some(server) = server_of(s) else { return };
    if matches!(e.kind, WireKind::Unreachable(_) | WireKind::TimedOut | WireKind::Dropped(_)) {
        let now = *s.clock;
        step_link(s, &server, Event::CallFailed { now }, tx);
    }
}

/// The stream is back.
pub fn back(s: &mut App) {
    let Some(server) = server_of(s) else { return };
    step_link(s, &server, Event::StreamUp, &mut Tx::default());
}

/// The server answered something: a suspect link is fine. True when the header changed.
pub fn heard(s: &mut App) -> bool {
    let Some(server) = server_of(s) else { return false };
    if !matches!(s.services.server_links.state(&server), Link::Suspect { .. }) {
        return false;
    }
    step_link(s, &server, Event::Heard, &mut Tx::default())
}

/// The window's server's link is anything but online: nothing should wait on it to answer.
pub fn down(s: &App) -> bool {
    link_of(s, crate::servers::runtime::active(s).as_str()) != Link::Online
}

/// Marks the header once trouble has lasted [`QUIET`], for every server whose link waits on
/// the clock; and steps the window's server's link with what its launch and health probe say
/// when that changes.
pub fn poll(s: &mut App, tx: &mut Tx) -> bool {
    let now = *s.clock;
    let mut changed = probed(s, tx);
    let ticking: Vec<ServerId> = s.services.server_links.states.iter().filter(|(_, l)| l.ticks()).map(|(k, _)| k.clone()).collect();
    for server in ticking {
        changed |= step_link(s, &server, Event::Tick { now }, tx);
    }
    // A `&mut` borrow of `stops` is a write that rebuilds every region reading it (the composer
    // dock): borrow it only when a retry is set, not on every poll.
    if rune::Tracked::untracked(&s.stops).active_sync.retry_at.is_some()
        && let Some(remaining) = s.stops.active_sync.take_due(now)
    {
        if remaining <= 0.0 {
            request_active(s, false);
            crate::windows::peer::touch(s);
        } else {
            tx.wake_after(Duration::from_secs_f64(remaining));
        }
    }
    changed
}

/// What the launch and the health probe say of a server, for its link.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Probe {
    Unknown,
    Answering,
    SignedOut,
    Incompatible,
    Unreachable,
}

/// Every checked-out or parked server's launch and health answers, into its link when they
/// change. A probe that finds nothing doesn't outvote a stream that's up.
fn probed(s: &mut App, tx: &mut Tx) -> bool {
    use crate::view::server_unavailable::{self as unavailable, Cover};
    let now = *s.clock;
    let mut changed = false;
    let active_key = crate::servers::runtime::active(s).clone();
    for key in s.checkout.keys() {
        let server = key.as_str().to_owned();
        let healthy = crate::servers::health::get(&key).is_some_and(|h| opencode_servers::health::is_healthy(Some(&h)));
        // Only treat launch's Offline phase as authoritative over a healthy probe for the active
        // server (or when the server is Unlisted); a parked server's healthy probe clears it.
        let launch_offline = s.launch.phase(&server).is_some_and(|p| {
            matches!(p, crate::launch::Phase::Unlisted) || (key == active_key && p.offline())
        });
        let probe = match unavailable::cover(&key) {
            Some(Cover::SignedOut) => Probe::SignedOut,
            Some(Cover::Incompatible) => Probe::Incompatible,
            None if launch_offline || unavailable::unreachable(&key) => Probe::Unreachable,
            None if healthy => Probe::Answering,
            None => Probe::Unknown,
        };
        if s.services.server_links.heard.get(&server).copied().unwrap_or(Probe::Unknown) == probe {
            continue;
        }
        let event = match probe {
            Probe::SignedOut => Event::Unauthorized,
            Probe::Incompatible => Event::Healthy { compatible: false },
            Probe::Answering => Event::Healthy { compatible: true },
            // Heard only once the stream is down too, so asked again each poll until then.
            Probe::Unreachable if matches!(link_of(s, &server), Link::Online | Link::Suspect { .. }) => continue,
            Probe::Unreachable => Event::Unreachable { now },
            Probe::Unknown => {
                s.services.server_links.heard.insert(server, probe);
                continue;
            }
        };
        s.services.server_links.heard.insert(server.clone(), probe);
        changed |= step_link(s, &server, event, tx);
    }
    changed
}

/// `--stream-audit <file>` (the resilience harness's stream cuts): each poll where the open
/// session's newest reply shows other text than the last, one JSON line with it, so the harness
/// can check every text shown was a start of the final one.
pub fn audit(s: &App) {
    use std::io::Write;
    static FILE: std::sync::OnceLock<Option<std::sync::Mutex<(std::fs::File, String)>>> = std::sync::OnceLock::new();
    let Some(file) = FILE.get_or_init(|| crate::arg("--stream-audit").and_then(|p| std::fs::File::create(p).ok()).map(|f| std::sync::Mutex::new((f, String::new())))) else { return };
    let Route::Session { id, .. } = &*s.route else { return };
    let Some(Message::Assistant(a)) = s.transcripts.get(id).and_then(|t| t.messages.iter().rev().find(|m| matches!(m, Message::Assistant(_)))) else { return };
    let text: String = a.parts.iter().filter_map(|p| if let Part::Text { text } = p { Some(text.as_str()) } else { None }).collect();
    let mut f = file.lock().unwrap_or_else(|e| e.into_inner());
    if f.1 == text {
        return;
    }
    let line = serde_json::json!({ "t": *s.clock, "msg": a.id, "done": a.completed.is_some(), "text": text });
    writeln!(f.0, "{line}").ok();
    f.1 = text;
}

/// After a reconnect, reads again what the lost link may have hidden: the pages of the open
/// session's subagents that were running, so their rows say where they are now, not where they
/// were (or "Starting").
pub fn catch_up(s: &mut App) {
    let Some(server) = server_of(s) else { return };
    s.services.server_links.catch_up.insert(server.clone(), CatchUp::AwaitingActive);
    s.services.server_links.refreshing.remove(&server);
    let Route::Session { id: open, .. } = &*s.route else { return };
    let root = s.root_of(open);
    let mut kids: Vec<String> = s.running.iter().filter(|id| id.as_str() != root && s.descends(id, root)).cloned().collect();
    for id in [root, open.as_str()] {
        for m in s.transcripts.get(id).map_or(&[][..], |t| &t.messages[..]) {
            let Message::Assistant(a) = m else { continue };
            for p in &a.parts {
                if let Part::Tool { name, status: ToolStatus::Running | ToolStatus::Pending, input, metadata, .. } = p
                    && matches!(name.as_str(), "task" | "subagent")
                    && let Some(child) = model::subagent_session(input, metadata)
                {
                    kids.push(child.to_owned());
                }
            }
        }
    }
    for id in kids {
        refresh(s, &server, id);
    }
}

fn refresh(s: &mut App, server: &str, id: String) {
    let Source::Live { requests: Some(r), .. } = &*s.source else { return };
    if s.services.server_links.refreshing.entry(server.to_owned()).or_default().insert(id.clone()) {
        r.send(Request::Session(id)).ok();
    }
}

/// The server's running sessions: families we don't know are looked up, and right after a
/// reconnect each running subagent's page is read too.
pub fn active(s: &mut App, ids: &[String]) {
    let Some(server) = server_of(s) else { return };
    let catching_up = s.services.server_links.catch_up.remove(&server) == Some(CatchUp::AwaitingActive);
    let mut missing: Vec<String> = ids.iter().filter_map(|id| s.missing_ancestor(id)).collect();
    missing.sort();
    missing.dedup();
    for id in missing {
        s.fetch_info(&id);
    }
    if catching_up {
        let kids: Vec<String> = ids.iter().filter(|id| s.session(id).is_none_or(|i| i.parent.is_some())).cloned().collect();
        for id in kids {
            refresh(s, &server, id);
        }
    }
}

/// A page landed: a subagent read again after a reconnect shows its latest step from it.
pub fn page_in(s: &mut App, id: &str) {
    let Some(server) = server_of(s) else { return };
    if s.services.server_links.refreshing.get_mut(&server).is_some_and(|ids| ids.remove(id)) {
        s.forget_agent_steps(id);
    }
}

mod story {
    use crate::App;
    use crate::stories::{Story, Trigger, replay};

    use super::{Event, Link, step_link};

    /// The open session, its stream down long enough to say so.
    fn reconnecting(s: &mut App) {
        crate::open_scene(s);
        s.finish_stream();
        let server = super::server_of(s).unwrap_or_default();
        let now = *s.clock;
        step_link(s, &server, Event::StreamDown { now: now - super::QUIET, error: None }, &mut rune::Tx::default());
        step_link(s, &server, Event::Tick { now }, &mut rune::Tx::default());
        debug_assert_eq!(s.services.server_links.state(&server), Link::Reopening { since: now - super::QUIET, shown: true });
    }

    const KEYS: &[Trigger] = &[
        replay(|s, _| {
            let server = super::server_of(s).unwrap_or_default();
            let now = *s.clock;
            step_link(s, &server, Event::StreamDown { now, error: None }, &mut rune::Tx::default());
        }),
        Trigger { key: 'b', label: "Stream back", run: |s, _| super::back(s) },
    ];

    rune_register::register!(crate::stories::STORY_LIST: Story = [Story::session("Session/Reconnecting", KEYS)]);
    rune_register::register!(crate::scenes::SCENE_LIST: crate::scenes::Scene = [("reconnecting", reconnecting)]);
}

