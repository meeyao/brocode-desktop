//! Every listed server's health (`runtime/server/health-machine.ts`, ported transition for
//! transition): one `/api/info` probe per server when it is listed or its endpoint changes, then
//! every 10 s while any server has an endpoint, and a `retry` that probes all of them now and
//! answers once each has. Every probe carries its slot's attempt; an answer for another is stale.

use opencode_kit::servers::key::ServerKey;
pub use opencode_kit::servers::probe::ProbeResult;
use rune::machine::{Attempt, Effects};

use crate::plugin_server::{PluginStage, PluginStatus};
use crate::registry::{HttpBase, ServerConn};

/// `SERVER_HEALTH_POLL_MS` (`health-machine.ts:5`).
pub const SERVER_HEALTH_POLL_MS: u64 = 10_000;

/// What the app knows of a server's health (`ServerHealth`, `:13-23`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Health {
    Probe(ProbeResult),
    /// Waiting for the first answer from the current endpoint.
    Checking { managed: bool },
    Incompatible,
    Starting,
    Auth,
    Stopped,
}

impl Health {
    pub const HEALTHY: Health = Health::Probe(ProbeResult::Healthy { version: None, hostname: None });
    pub const UNREACHABLE: Health = Health::Probe(ProbeResult::Unreachable);
    pub const UNAUTHORIZED: Health = Health::Probe(ProbeResult::Unauthorized);
}

/// A listed server as the health checks see it (`ServerHealthInput`, `:27-36`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Input {
    Staged { key: ServerKey, stage: PluginStage },
    Endpoint { key: ServerKey, http: HttpBase, managed: bool },
}

impl Input {
    /// `healthInput` (`health.ts:227-236`).
    pub fn of(conn: &ServerConn) -> Input {
        let key = conn.key();
        match (conn, conn.http()) {
            (ServerConn::Plugin { status: PluginStatus::Stage(stage), .. }, _) => Input::Staged { key, stage: *stage },
            (_, Some(http)) => Input::Endpoint { key, http: http.clone(), managed: conn.managed() },
            (_, None) => Input::Staged { key, stage: PluginStage::Failed },
        }
    }

    fn key(&self) -> &ServerKey {
        match self {
            Input::Staged { key, .. } | Input::Endpoint { key, .. } => key,
        }
    }
}

/// The endpoint a slot probes, and its credentials string (`Endpoint`, `:38`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    pub http: HttpBase,
    pub auth: String,
    pub managed: bool,
}

/// One server (`ServerHealthSlot`, `:40-58`). `Checking` and `Refreshing` have a probe in
/// flight; `Settled` waits for the next poll.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Slot {
    Staged { stage: PluginStage, attempt: Attempt },
    Checking { endpoint: Endpoint, attempt: Attempt },
    Settled { endpoint: Endpoint, result: ProbeResult, attempt: Attempt },
    Refreshing { endpoint: Endpoint, result: ProbeResult, attempt: Attempt },
}

impl Slot {
    pub fn attempt(&self) -> Attempt {
        match self {
            Slot::Staged { attempt, .. } | Slot::Checking { attempt, .. } | Slot::Settled { attempt, .. } | Slot::Refreshing { attempt, .. } => *attempt,
        }
    }

    /// `selectServerHealth` (`:137-144`).
    pub fn health(&self) -> Health {
        match self {
            Slot::Staged { stage: PluginStage::Failed, .. } => Health::UNREACHABLE,
            Slot::Staged { stage, .. } => match stage {
                PluginStage::Stopped => Health::Stopped,
                PluginStage::Starting => Health::Starting,
                PluginStage::Auth => Health::Auth,
                PluginStage::Incompatible | PluginStage::Failed => Health::Incompatible,
            },
            Slot::Checking { endpoint, .. } => Health::Checking { managed: endpoint.managed },
            Slot::Settled { result, .. } | Slot::Refreshing { result, .. } => Health::Probe(result.clone()),
        }
    }
}

/// A `retry` caller's id, which `ResolveWaiters` hands back.
pub type WaiterId = u64;

/// A `retry` caller, waiting for each server's probe from at least the given attempt (`:60-64`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Waiter {
    pub id: WaiterId,
    pub targets: Vec<(ServerKey, Attempt)>,
}

/// `ServerHealthState` (`:66-69`): the slots in list order, and the `retry` callers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct State {
    pub entries: Vec<(ServerKey, Slot)>,
    pub waiters: Vec<Waiter>,
    /// Servers whose event stream is currently up, used to deduplicate immediate drop/open nudges.
    pub streaming: Vec<ServerKey>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    ServersSynced(Vec<Input>),
    PollDue,
    RetryRequested(WaiterId),
    /// Something other than a probe says server `key` went away or came back (its event stream
    /// dropped or reopened): probe it now rather than at the next poll.
    Nudged(ServerKey),
    /// Server `key`'s event stream opened (`true`) or dropped (`false`).
    Streaming { key: ServerKey, up: bool },
    ProbeSettled { key: ServerKey, attempt: Attempt, result: ProbeResult },
    Disposed,
}

/// An accepted probe answer with the credentials it used (`ServerProbe`, `:71-76`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Probe {
    pub key: ServerKey,
    pub auth: String,
    pub result: ProbeResult,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    Probe { key: ServerKey, attempt: Attempt, http: HttpBase },
    NotifyProbe(Probe),
    TimerStart { interval_ms: u64 },
    TimerStop,
    ResolveWaiters(Vec<WaiterId>),
}

impl State {
    pub fn get(&self, key: &ServerKey) -> Option<&Slot> {
        self.entries.iter().find(|(k, _)| k == key).map(|(_, s)| s)
    }

    /// `selectServerHealthRecord` (`:132-135`).
    pub fn health(&self, key: &ServerKey) -> Option<Health> {
        self.get(key).map(Slot::health)
    }

    /// `polling` (`packages/app/src/runtime/server/health-machine.ts:347-349`): any server has an
    /// endpoint (polled every `SERVER_HEALTH_POLL_MS` = 10 s, matching official).
    pub fn polling(&self) -> bool {
        self.entries.iter().any(|(_, s)| !matches!(s, Slot::Staged { .. }))
    }
}

/// `transitionServerHealth` (`health-machine.ts:111-130`).
pub fn step(state: &mut State, event: Event) -> Effects<Effect> {
    match event {
        Event::ServersSynced(inputs) => synced(state, inputs),
        Event::PollDue => polled(state),
        Event::RetryRequested(id) => retried(state, id),
        Event::Nudged(key) => nudged(state, &key),
        Event::Streaming { key, up } => streaming(state, key, up),
        Event::ProbeSettled { key, attempt, result } => answered(state, key, attempt, result),
        // Stop polling and release every `retry` caller.
        Event::Disposed => {
            if state.entries.is_empty() && state.waiters.is_empty() {
                return Effects::none();
            }
            let mut fx = Effects::none();
            if state.polling() {
                fx.push(Effect::TimerStop);
            }
            let ids: Vec<WaiterId> = state.waiters.iter().map(|w| w.id).collect();
            *state = State::default();
            resolved(&mut fx, ids);
            fx
        }
    }
}

/// `synced` (`:200-219`).
fn synced(state: &mut State, inputs: Vec<Input>) -> Effects<Effect> {
    let mut probes = Effects::none();
    let entries: Vec<(ServerKey, Slot)> = inputs
        .into_iter()
        .map(|input| {
            let prev = state.get(input.key());
            let next = listed(prev, &input);
            if let Slot::Checking { endpoint, attempt } = &next
                && Some(*attempt) != prev.map(Slot::attempt)
            {
                probes.push(Effect::Probe { key: input.key().clone(), attempt: *attempt, http: endpoint.http.clone() });
            }
            (input.key().clone(), next)
        })
        .collect();
    settle(state, entries, probes)
}

/// `listed` (`:221-242`).
fn listed(slot: Option<&Slot>, input: &Input) -> Slot {
    let attempt = slot.map_or(Attempt(0), Slot::attempt);
    match input {
        Input::Staged { stage, .. } => match slot {
            Some(s @ Slot::Staged { stage: current, .. }) if current == stage => s.clone(),
            // The probe in flight, if any, keeps its attempt and so can never land.
            _ => Slot::Staged { stage: *stage, attempt },
        },
        Input::Endpoint { http, managed, .. } => {
            let auth = http.credentials();
            let current = match slot {
                Some(Slot::Checking { endpoint, .. } | Slot::Settled { endpoint, .. } | Slot::Refreshing { endpoint, .. }) if endpoint.auth == auth => endpoint,
                // Another endpoint is another target: its previous answer says nothing of the new one.
                _ => return Slot::Checking { endpoint: Endpoint { http: http.clone(), auth, managed: *managed }, attempt: Attempt(attempt.0 + 1) },
            };
            let mut next = slot.cloned().unwrap_or(Slot::Staged { stage: PluginStage::Stopped, attempt });
            if current.managed != *managed {
                match &mut next {
                    Slot::Checking { endpoint, .. } | Slot::Settled { endpoint, .. } | Slot::Refreshing { endpoint, .. } => endpoint.managed = *managed,
                    Slot::Staged { .. } => {}
                }
            }
            next
        }
    }
}

/// A probe still in flight keeps running: a server slower than the poll still answers
/// (`packages/app/src/runtime/server/health-machine.ts:244-256`).
fn polled(state: &mut State) -> Effects<Effect> {
    let mut fx = Effects::none();
    for (key, slot) in &mut state.entries {
        if let Slot::Settled { endpoint, result, attempt } = slot {
            let attempt = Attempt(attempt.0 + 1);
            fx.push(Effect::Probe { key: key.clone(), attempt, http: endpoint.http.clone() });
            *slot = Slot::Refreshing { endpoint: endpoint.clone(), result: result.clone(), attempt };
        }
    }
    fx
}

/// A retry replaces any probe in flight, so one that hangs can't hold the caller (`:258-289`).
fn retried(state: &mut State, id: WaiterId) -> Effects<Effect> {
    let mut fx = Effects::none();
    let mut targets = Vec::new();
    for (key, slot) in &mut state.entries {
        let next = match slot {
            Slot::Staged { .. } => continue,
            Slot::Checking { endpoint, attempt } => Slot::Checking { endpoint: endpoint.clone(), attempt: Attempt(attempt.0 + 1) },
            Slot::Settled { endpoint, result, attempt } | Slot::Refreshing { endpoint, result, attempt } => {
                Slot::Refreshing { endpoint: endpoint.clone(), result: result.clone(), attempt: Attempt(attempt.0 + 1) }
            }
        };
        let (Slot::Checking { endpoint, attempt } | Slot::Refreshing { endpoint, attempt, .. }) = &next else { continue };
        fx.push(Effect::Probe { key: key.clone(), attempt: *attempt, http: endpoint.http.clone() });
        targets.push((key.clone(), *attempt));
        *slot = next;
    }
    if targets.is_empty() {
        return Effects::one(Effect::ResolveWaiters(vec![id]));
    }
    state.waiters.push(Waiter { id, targets });
    fx.push(Effect::TimerStart { interval_ms: SERVER_HEALTH_POLL_MS });
    fx
}

/// A probe of server `key` now, superseding one in flight; no caller waits on it, and the poll
/// keeps its time.
fn nudged(state: &mut State, key: &ServerKey) -> Effects<Effect> {
    let Some(slot) = state.entries.iter_mut().find_map(|(k, s)| (k == key).then_some(s)) else { return Effects::none() };
    let next = match slot {
        Slot::Staged { .. } => return Effects::none(),
        Slot::Checking { endpoint, attempt } => Slot::Checking { endpoint: endpoint.clone(), attempt: Attempt(attempt.0 + 1) },
        Slot::Settled { endpoint, result, attempt } | Slot::Refreshing { endpoint, result, attempt } => {
            Slot::Refreshing { endpoint: endpoint.clone(), result: result.clone(), attempt: Attempt(attempt.0 + 1) }
        }
    };
    let (Slot::Checking { endpoint, attempt } | Slot::Refreshing { endpoint, attempt, .. }) = &next else { return Effects::none() };
    let fx = Effects::one(Effect::Probe { key: key.clone(), attempt: *attempt, http: endpoint.http.clone() });
    *slot = next;
    fx
}

/// A server's event stream opened or dropped: nudge an immediate probe on drop, or on open when
/// cached health was not Healthy. The 10 s poll stays active for all endpoints (`health-machine.ts`).
fn streaming(state: &mut State, key: ServerKey, up: bool) -> Effects<Effect> {
    if state.streaming.contains(&key) == up {
        return Effects::none();
    }
    if up {
        state.streaming.push(key.clone());
    } else {
        state.streaming.retain(|k| *k != key);
    }
    let stale = !matches!(state.health(&key), Some(Health::Probe(ProbeResult::Healthy { .. })));
    if up && !stale { Effects::none() } else { nudged(state, &key) }
}

/// `answered` (`:291-311`): a superseded, unlisted or disposed probe changes nothing.
fn answered(state: &mut State, key: ServerKey, attempt: Attempt, result: ProbeResult) -> Effects<Effect> {
    let Some(Slot::Checking { endpoint, attempt: current } | Slot::Refreshing { endpoint, attempt: current, .. }) = state.get(&key) else { return Effects::none() };
    if *current != attempt {
        return Effects::none();
    }
    let endpoint = endpoint.clone();
    let probe = Probe { key: key.clone(), auth: endpoint.auth.clone(), result: result.clone() };
    let entries = state.entries.iter().map(|(k, s)| (k.clone(), if *k == key { Slot::Settled { endpoint: endpoint.clone(), result: result.clone(), attempt } } else { s.clone() })).collect();
    settle(state, entries, Effects::one(Effect::NotifyProbe(probe)))
}

/// `settle` (`:313-340`): applies new entries, starts or stops the poll, and resolves the `retry`
/// callers whose probes all answered.
fn settle(state: &mut State, entries: Vec<(ServerKey, Slot)>, mut fx: Effects<Effect>) -> Effects<Effect> {
    if entries == state.entries {
        return fx;
    }
    let was = state.polling();
    state.entries = entries;
    let mut done = Vec::new();
    let entries = &state.entries;
    state.waiters.retain_mut(|w| {
        w.targets.retain(|(key, attempt)| !answered_since(entries.iter().find(|(k, _)| k == key).map(|(_, s)| s), *attempt));
        if w.targets.is_empty() {
            done.push(w.id);
        }
        !w.targets.is_empty()
    });
    let now = state.polling();
    if !was && now {
        fx.push(Effect::TimerStart { interval_ms: SERVER_HEALTH_POLL_MS });
    }
    if was && !now {
        fx.push(Effect::TimerStop);
    }
    resolved(&mut fx, done);
    fx
}

/// A server that left the list or has no endpoint has nothing left to answer (`:342-345`).
fn answered_since(slot: Option<&Slot>, attempt: Attempt) -> bool {
    match slot {
        None | Some(Slot::Staged { .. }) => true,
        Some(Slot::Settled { attempt: a, .. }) => *a >= attempt,
        _ => false,
    }
}

fn resolved(fx: &mut Effects<Effect>, ids: Vec<WaiterId>) {
    if !ids.is_empty() {
        fx.push(Effect::ResolveWaiters(ids));
    }
}

/// `isServerHealthy` (`:146-148`). A server that answers `/api/info` is usable whatever version
/// it reports.
pub fn is_healthy(health: Option<&Health>) -> bool {
    matches!(health, Some(Health::Probe(ProbeResult::Healthy { .. })))
}

/// `canUseServer` (`:150-153`): it answered, or nothing says it can't yet.
pub fn can_use(health: Option<&Health>) -> bool {
    matches!(health, None | Some(Health::Probe(ProbeResult::Healthy { .. }) | Health::Checking { .. }))
}

/// `isServerFailed` (`:155-158`): its last answer, or its plugin, says it can't be used.
pub fn is_failed(health: Option<&Health>) -> bool {
    matches!(health, Some(Health::Probe(ProbeResult::Unreachable | ProbeResult::Unauthorized) | Health::Incompatible))
}

pub fn is_unauthorized(health: Option<&Health>) -> bool {
    matches!(health, Some(Health::Probe(ProbeResult::Unauthorized)))
}

/// Its plugin says it can't run.
pub fn is_incompatible(health: Option<&Health>) -> bool {
    matches!(health, Some(Health::Incompatible))
}

/// `selectServerVersion` (`:168-170`).
pub fn version(health: Option<&Health>) -> Option<&str> {
    match health {
        Some(Health::Probe(ProbeResult::Healthy { version, .. })) => version.as_deref(),
        _ => None,
    }
}

/// `selectServerHostname` (`:172-174`).
pub fn hostname(health: Option<&Health>) -> Option<&str> {
    match health {
        Some(Health::Probe(ProbeResult::Healthy { hostname, .. })) => hostname.as_deref(),
        _ => None,
    }
}

/// Sort rank: healthy, then settling, then failed (`selectServerHealthRank`, `:176-183`).
pub fn rank(health: Option<&Health>) -> u8 {
    if is_healthy(health) {
        return 0;
    }
    if is_failed(health) { 2 } else { 1 }
}

/// The status mark a server row shows (`selectServerIndicator`, `:185-196`;
/// `servers/registry/row.tsx` draws each).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Indicator {
    /// 6×6 dot, `icon-success-base`.
    Healthy,
    /// 6×6 dot, `icon-critical-base`.
    Unreachable,
    /// 6×6 dot, `v2-icon-icon-faint`.
    Unknown,
    /// 12 px `lock`, `v2-icon-icon-muted`.
    Auth,
    /// 12 px spinner.
    Connecting,
    /// `warning`, `icon-warning-base`.
    Incompatible,
}

pub fn indicator(health: Option<&Health>) -> Indicator {
    match health {
        None | Some(Health::Stopped) => Indicator::Unknown,
        Some(Health::Auth | Health::Probe(ProbeResult::Unauthorized)) => Indicator::Auth,
        Some(Health::Starting) => Indicator::Connecting,
        Some(Health::Checking { managed: true }) => Indicator::Connecting,
        Some(Health::Checking { managed: false }) => Indicator::Unknown,
        Some(Health::Probe(ProbeResult::Healthy { .. })) => Indicator::Healthy,
        Some(Health::Probe(ProbeResult::Unreachable)) => Indicator::Unreachable,
        Some(Health::Incompatible) => Indicator::Incompatible,
    }
}

