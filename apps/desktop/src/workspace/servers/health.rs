//! Every listed server's health, app-wide, as Desktop's health checks run
//! (`runtime/server/health-machine.ts`, `health.ts`): one `/api/info` probe per server when it is
//! listed or its endpoint changes, then every 10 s, and a `retry` that probes all of them now.
//! The machine is MS-1's [`opencode_kit::servers::health`]; this runs its effects: each probe on a
//! worker thread, its answer back through a channel any window's poll drains, the poll's timer
//! on the app's clock. A probe that answers 401 tells the server's controller, which stops its
//! connection until a probe with the same credentials answers again (`machine.ts:203-242`).
//!
//! Home's status marks, its Authenticate button, the cross-server searches and the location
//! dialog read it ([`get`], [`indicator`]).

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{Receiver, Sender, channel};

use opencode_kit::servers::ServerKey;
use opencode_servers::ServerConn;
use opencode_servers::health::{self, Effect, Event, Health, Indicator, Input, ProbeResult};
use rune::Tx;
use rune::machine::Attempt;

use crate::App;

/// The poll's timer (`TimerStart` / `TimerStop`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Timer {
    #[default]
    Stopped,
    /// The next poll is due at this app clock time.
    Every { due: f64, interval: f64 },
}

enum Answer {
    Probe { key: ServerKey, attempt: Attempt, result: ProbeResult, discovered: Option<opencode_kit::client::Server> },
    /// Its home folder, asked for once it answers (`sync.data.path.home`), after the probe's
    /// answer: a server that just started takes seconds over it.
    Home { key: ServerKey, home: String },
}

struct Runtime {
    state: health::State,
    timer: Timer,
    inputs: Vec<Input>,
    /// The connection each probe uses, by server.
    conns: HashMap<ServerKey, ServerConn>,
    /// What each server's `/api/info` last called its machine (`store.hostnames`).
    hostnames: HashMap<ServerKey, String>,
    /// Each server's home folder, inferred from where it runs (`loadPathQuery`), for `~`.
    homes: HashMap<ServerKey, String>,
    answers: (Sender<Answer>, Receiver<Answer>),
    waiters: u64,
    /// `retry` callers whose every probe answered, until they ask ([`resolved`]).
    resolved: HashSet<u64>,
    version: u64,
}

impl Default for Runtime {
    fn default() -> Self {
        Runtime { state: health::State::default(), timer: Timer::Stopped, inputs: Vec::new(), conns: HashMap::new(), hostnames: HashMap::new(), homes: HashMap::new(), answers: channel(), waiters: 0, resolved: HashSet::new(), version: 0 }
    }
}

thread_local! {
    static HEALTH: RefCell<Runtime> = RefCell::default();
}

/// Server `key`'s health; None while nothing is known.
pub fn get(key: &ServerKey) -> Option<Health> {
    HEALTH.with_borrow(|h| h.state.health(key))
}

/// The status mark a row shows for server `key` (`selectServerIndicator`).
pub fn indicator(key: &ServerKey) -> Indicator {
    health::indicator(get(key).as_ref())
}

/// Whether server `key`'s latest health probe answered HTTP 401 (`isServerUnauthorized`).
pub fn is_unauthorized(key: &ServerKey) -> bool {
    health::is_unauthorized(get(key).as_ref())
}

/// The hostname server `key` last reported.
pub fn hostname(key: &ServerKey) -> Option<String> {
    HEALTH.with_borrow(|h| h.hostnames.get(key).cloned())
}

/// Server `key`'s home folder, as Desktop infers it from where the server runs
/// (`inferHomeDirectory(location.directory) || inferHomeDirectory(location.project.directory)`);
/// None until it has answered, or when it runs outside a home.
pub fn home(key: &ServerKey) -> Option<String> {
    HEALTH.with_borrow(|h| h.homes.get(key).cloned()).filter(|h| !h.is_empty())
}

/// Changes whenever any server's health does.
pub fn version() -> u64 {
    HEALTH.with_borrow(|h| h.version)
}

/// Probes every server now (`retry`), as Home's and Settings' Retry do; [`resolved`] says when
/// each answered. None without a server.
pub fn retry(s: &App, tx: &mut Tx) -> Option<u64> {
    if !s.live() {
        return None;
    }
    let (id, before, fx) = HEALTH.with_borrow_mut(|h| {
        let before = shown(h);
        h.waiters += 1;
        (h.waiters, before, health::step(&mut h.state, Event::RetryRequested(h.waiters)))
    });
    run(s, before, fx, tx);
    Some(id)
}

/// Probes every server now without registering a waiter ID (for fire-and-forget callers such as
/// network-change recovery, avoiding leaking unconsumed IDs in `resolved`).
pub fn retry_quiet(s: &App, tx: &mut Tx) {
    if !s.live() {
        return;
    }
    let keys: Vec<ServerKey> = HEALTH.with_borrow(|h| h.conns.keys().cloned().collect());
    for key in keys {
        nudge(s, &key, tx);
    }
}

/// Probes server `key` now: its event stream dropped or came back, so Home's mark and the
/// server's tabs follow at once rather than at the next 10 s poll.
pub fn nudge(s: &App, key: &ServerKey, tx: &mut Tx) {
    if !s.live() {
        return;
    }
    dispatch(s, Event::Nudged(key.clone()), tx);
}

/// Server `key`'s event stream opened (`up`) or dropped: nudges an immediate probe on drop (or
/// on open while health was not yet Healthy); the 10 s poll keeps running per official `health-machine.ts`.
pub fn streaming(s: &App, key: &ServerKey, up: bool, tx: &mut Tx) {
    if !s.live() {
        return;
    }
    dispatch(s, Event::Streaming { key: key.clone(), up }, tx);
}

/// Whether `retry` caller `id`'s probes have all answered (it's forgotten once asked).
pub fn resolved(id: u64) -> bool {
    HEALTH.with_borrow_mut(|h| h.resolved.remove(&id))
}

/// Each poll: the registry's changes reach the machine, answers that came in land, and a due
/// poll probes again. True when a server's health changed.
pub fn poll(s: &App, tx: &mut Tx) -> bool {
    if !s.live() {
        return false;
    }
    let before = version();
    let entries = super::registry::list(s);
    let inputs: Vec<Input> = entries.iter().map(|e| Input::of(&e.conn)).collect();
    let synced = HEALTH.with_borrow_mut(|h| {
        h.conns = entries.into_iter().map(|e| (e.key(), e.conn)).collect();
        if h.inputs == inputs {
            return None;
        }
        let prev = shown(h);
        h.inputs = inputs.clone();
        Some((prev, health::step(&mut h.state, Event::ServersSynced(inputs))))
    });
    if let Some((prev, fx)) = synced {
        run(s, prev, fx, tx);
    }
    let answers: Vec<Answer> = HEALTH.with_borrow(|h| h.answers.1.try_iter().collect());
    for a in answers {
        let (key, attempt, result, discovered) = match a {
            Answer::Home { key, home } => {
                HEALTH.with_borrow_mut(|h| {
                    h.homes.insert(key, home);
                    h.version += 1;
                });
                continue;
            }
            Answer::Probe { key, attempt, result, discovered } => (key, attempt, result, discovered),
        };
        if let Some(actual) = discovered
            && crate::servers::runtime::own_server()
                .as_ref()
                .is_none_or(|o| o.base != actual.base || o.auth_header() != actual.auth_header())
        {
            crate::servers::runtime::set_sidecar(&actual);
            let entries = super::registry::list(s);
            let inputs: Vec<Input> = entries.iter().map(|e| Input::of(&e.conn)).collect();
            let (prev, fx) = HEALTH.with_borrow_mut(|h| {
                let prev = shown(h);
                h.conns = entries.into_iter().map(|e| (e.key(), e.conn)).collect();
                h.inputs = inputs.clone();
                (prev, health::step(&mut h.state, Event::ServersSynced(inputs)))
            });
            run(s, prev, fx, tx);
            continue;
        }
        dispatch(s, Event::ProbeSettled { key, attempt, result }, tx);
    }
    let due = HEALTH.with_borrow(|h| match h.timer {
        Timer::Every { due, .. } => Some(due),
        Timer::Stopped => None,
    });
    if let Some(due) = due {
        if due <= *s.clock {
            let (prev, fx) = HEALTH.with_borrow_mut(|h| {
                let prev = shown(h);
                if let Timer::Every { interval, .. } = h.timer {
                    h.timer = Timer::Every { due: *s.clock + interval, interval };
                }
                (prev, health::step(&mut h.state, Event::PollDue))
            });
            run(s, prev, fx, tx);
        }
        if let Timer::Every { due, .. } = HEALTH.with_borrow(|h| h.timer) {
            tx.wake_at(due);
        }
    }
    version() != before
}

type Shown = Vec<(ServerKey, Health, Option<String>)>;

fn dispatch(s: &App, event: Event, tx: &mut Tx) {
    let (before, fx) = HEALTH.with_borrow_mut(|h| {
        let before = shown(h);
        (before, health::step(&mut h.state, event))
    });
    run(s, before, fx, tx);
}

fn run(s: &App, before: Shown, fx: rune::machine::Effects<Effect>, tx: &mut Tx) {
    // Only what rows show moves the version and asks for a frame: an answer that says what the
    // last one did (an unreachable server's every 10 s) changes nothing on screen.
    let mut changed = false;
    let mut known = Vec::new();
    for effect in fx {
        match effect {
            Effect::Probe { key, attempt, .. } => {
                if let Some(result) = probe(s, &key, attempt) {
                    known.push((key, attempt, result));
                }
            }
            Effect::NotifyProbe(probe) => {
                if let ProbeResult::Healthy { hostname: Some(name), .. } = &probe.result {
                    HEALTH.with_borrow_mut(|h| h.hostnames.insert(probe.key.clone(), name.clone()));
                }
                crate::servers::runtime::reported(&probe);
            }
            Effect::TimerStart { interval_ms } => {
                let interval = interval_ms as f64 / 1000.0;
                HEALTH.with_borrow_mut(|h| h.timer = Timer::Every { due: *s.clock + interval, interval });
                tx.wake_at(*s.clock + interval);
            }
            Effect::TimerStop => HEALTH.with_borrow_mut(|h| h.timer = Timer::Stopped),
            Effect::ResolveWaiters(ids) => {
                HEALTH.with_borrow_mut(|h| h.resolved.extend(ids));
                changed = true;
            }
        }
    }
    changed |= HEALTH.with_borrow(shown) != before;
    if changed {
        HEALTH.with_borrow_mut(|h| h.version += 1);
        tx.instant();
    }
    for (key, attempt, result) in known {
        dispatch(s, Event::ProbeSettled { key, attempt, result }, tx);
    }
}

/// What rows show of every server's health.
fn shown(h: &Runtime) -> Shown {
    h.state.entries.iter().map(|(k, slot)| (k.clone(), slot.health(), h.hostnames.get(k).cloned())).collect()
}

/// `checkServerHealth` (`packages/app/src/runtime/server/health.ts:26-143`): 30 s timeout, 2
/// transport retries (100 ms, 200 ms), 401 -> Unauthorized, validated `ServerInfo` JSON.
fn probe_http(server: &opencode_kit::client::Server, conn: &ServerConn) -> ProbeResult {
    if let Some(http) = conn.http().filter(|_| !conn.builtin()) {
        return opencode_kit::servers::probe::probe(http);
    }
    // Sidecar carries its resolved Basic auth header on `server`, so probe with the same
    // 2-retry / 100 ms linear backoff policy on transport errors as `checkServerHealth`.
    let deadline = rune::time::Instant::now() + opencode_kit::servers::probe::PROBE_TIMEOUT;
    let mut count = 0u32;
    loop {
        match server.get("/api/info") {
            Ok(v) => match opencode_kit::servers::probe::ServerInfo::from_json(&v) {
                Some(info) => return ProbeResult::Healthy {
                    version: Some(info.version),
                    hostname: info.hostname.filter(|h| !h.is_empty()),
                },
                None => return ProbeResult::Unreachable,
            },
            Err(e) if e.status() == Some(401) => return ProbeResult::Unauthorized,
            Err(e) if count < opencode_kit::servers::probe::PROBE_RETRIES
                && matches!(e.kind, opencode_kit::wire_error::WireKind::Unreachable(_) | opencode_kit::wire_error::WireKind::Dropped(_) | opencode_kit::wire_error::WireKind::Tls(_)) =>
            {
                count += 1;
                let wait = opencode_kit::servers::probe::PROBE_RETRY_DELAY * count;
                if rune::time::Instant::now() + wait >= deadline {
                    return ProbeResult::Unreachable;
                }
                rune::task::sleep(wait);
            }
            Err(_) => return ProbeResult::Unreachable,
        }
    }
}

/// Runs `checkServerHealth` on a worker thread and fetches `/api/location` once on first healthy
/// answer to infer the server's home folder (`loadPathQuery`).
fn probe(s: &App, key: &ServerKey, attempt: Attempt) -> Option<ProbeResult> {
    let conn = HEALTH.with_borrow(|h| h.conns.get(key).cloned())?;
    let server = super::registry::server_for(&conn)?;
    let discover_sidecar = key.is_sidecar() && crate::arg("--server").is_none();
    let wake = s.launch.inbox_wake.clone().or_else(|| (*s.waker).clone());
    let ask_home = HEALTH.with_borrow(|h| !h.homes.contains_key(key));
    let send = HEALTH.with_borrow(|h| h.answers.0.clone());
    let key = key.clone();
    crate::plugin_host::tasks::run(move || {
        let current = if discover_sidecar {
            opencode_kit::local_service::discover_local_server(&server)
        } else {
            server.clone()
        };
        let (server, result) = (current.clone(), probe_http(&current, &conn));
        #[cfg(not(target_arch = "wasm32"))]
        let (server, result) = if result == ProbeResult::Unreachable && discover_sidecar && super::sidecar::managed() {
            match super::sidecar::reconnect() {
                Ok(restarted) => {
                    let res = probe_http(&restarted, &conn);
                    (restarted, res)
                }
                Err(_) => (server, result),
            }
        } else {
            (server, result)
        };
        let healthy = matches!(result, ProbeResult::Healthy { .. });
        let discovered = discover_sidecar.then(|| server.clone());
        send.send(Answer::Probe { key: key.clone(), attempt, result, discovered }).ok();
        if let Some(w) = &wake {
            w();
        }
        let Some(v) = (ask_home && healthy).then(|| server.get("/api/location").ok()).flatten() else { return };
        let dir = |v: &serde_json::Value| v["directory"].as_str().unwrap_or_default().to_owned();
        let home = Some(opencode_kit::path::infer_home(&dir(&v))).filter(|h| !h.is_empty()).unwrap_or_else(|| opencode_kit::path::infer_home(&dir(&v["project"])));
        send.send(Answer::Home { key, home }).ok();
        if let Some(w) = &wake {
            w();
        }
    });
    None
}
