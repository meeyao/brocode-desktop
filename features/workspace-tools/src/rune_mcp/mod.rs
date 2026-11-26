//! Per-folder `rune` client MCP lifecycle, multi-window connection sharing, and session routing.
//!
//! Brocode attaches one WebSocket per open folder (`name=rune`) on the connected server.
//! Multiple windows open on the same folder share one [`ClientMcp`] connection through
//! [`RuneMcpHub`], and every incoming `tools/call` is routed by `_meta["ai.opencode/sessionID"]`
//! (and its parent chain) to the window and session tab showing it.

pub mod tools;

use std::collections::{BTreeSet, HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::ControlFlow;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::Duration;

use opencode_kit::client::{DEFAULT_SERVER, Server};
use opencode_kit::client_mcp::{
    ClientMcp, ClientMcpOptions, McpCall, McpResult, RECONNECT_MAX, RECONNECT_MIN,
    UNSUPPORTED_RETRY,
};
use opencode_kit::model::{Project, SessionInfo};
use opencode_kit::path;
use opencode_kit::servers::ServerKey;
use rune::{Tx, Waker};

pub const MCP_NAME: &str = "rune";
pub const SESSION_NOT_OPEN_ERROR: &str = "This session isn't open in Brocode";
pub const REMOTE_BROWSER_MCP_ERROR: &str =
    "Local browser MCP tools are refused on remote servers to prevent access to the local machine network.";
pub const RPC_ATTACHED_BROWSER_MCP_ERROR: &str =
    "This session is attached to the built-in experimental.browser tools; local browser_* MCP tools are refused.";
/// Sent in `initialize.result.instructions` for forward compatibility when OpenCode surfaces
/// instructions from attached client MCP sockets alongside configured MCP servers.
pub const MCP_INSTRUCTIONS: &str = "Brocode UI tools for sessions open in Brocode. Load the `rune-ui` skill before writing ```ui outlines.";

/// Whether an MCP tool name belongs to the legacy local `browser_*` / `browser_cdp` tool family.
pub fn is_browser_tool(name: &str) -> bool {
    name.starts_with("browser_")
}

/// Announces whether `window_id` currently suppresses local `browser_*` tools on `server`
/// (because `experimental.browser` v4 is attached or in sticky transient reconnect), updating
/// `ClientMcp::set_tools` (`notifications/tools/list_changed`) in place without affecting other
/// windows attached to the same endpoint.
pub fn set_browser_rpc_attached(window_id: u64, server: &Server, suppressed: Option<bool>) {
    global_hub().set_browser_rpc_attached(window_id, server, suppressed);
}

static NEXT_WINDOW_ID: AtomicU64 = AtomicU64::new(1);
static GLOBAL_HUB: OnceLock<Arc<RuneMcpHub>> = OnceLock::new();

type EndpointKey = (String, u64);
type FolderKey = (EndpointKey, String);

fn global_hub() -> Arc<RuneMcpHub> {
    GLOBAL_HUB.get_or_init(RuneMcpHub::new).clone()
}

fn clean_dir(raw: &str) -> String {
    path::trim(raw.trim()).to_owned()
}

/// Borrowed view of the active window route for MCP folder attachment and routing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum McpRoute<'a> {
    Home,
    Draft(&'a str),
    Session { server: &'a ServerKey, id: &'a str },
}

/// Borrowed view of one open tab for zero-allocation MCP sync fingerprinting and folder discovery.
#[derive(Clone, Copy, Debug)]
pub struct McpTab<'a> {
    pub id: &'a str,
    pub directory: &'a str,
    pub server: &'a ServerKey,
}

/// Object-safe host boundary for per-window `rune` client MCP registration, polling, and tool dispatch.
pub trait RuneMcpHost: opencode_kernel::AppEnv {
    fn rune_mcp(&self) -> &WindowMcp;
    fn rune_mcp_mut(&mut self) -> &mut WindowMcp;

    fn is_launch_waiting(&self) -> bool { false }
    fn is_stage(&self) -> bool { false }
    /// Whether `--client-mcp` was passed (headless mode only attaches against non-default servers).
    fn client_mcp_flag(&self) -> bool { false }
    /// Active live server and whether write/send operations are allowed (`allow_send`).
    fn live_server(&self) -> Option<(&Server, bool)>;

    fn route(&self) -> McpRoute<'_>;
    fn draft_dir(&self) -> &str;
    fn used_project_ids(&self) -> &[String];
    fn projects(&self) -> &[Project];
    fn used_projects(&self) -> Vec<&Project>;
    fn tab_count(&self) -> usize;
    fn tab_at(&self, index: usize) -> Option<McpTab<'_>>;
    fn sessions(&self) -> &[SessionInfo];
    fn session(&self, id: &str) -> Option<&SessionInfo>;
    fn sessions_version(&self) -> (u32, u64);
    fn root_of<'a>(&'a self, id: &'a str) -> &'a str { id }
    fn active_server(&self) -> &ServerKey;

    /// Returns `Some(reason)` when local `browser_*` / `browser_cdp` MCP tools must be refused for
    /// `session_id` (remote server without tunnel confinement, or session/server attached via
    /// `experimental.browser` v4 RPC).
    fn browser_mcp_refusal(&self, _session_id: &str) -> Option<&'static str> { None }
    /// Dispatches an incoming `tools/call` to the plugin owning it; returns `Some(call)` when no
    /// plugin handles the tool so `tools::execute` responds with `"Unknown tool: ..."`.
    fn dispatch_mcp_call(&mut self, _session_id: &str, call: McpCall, _tx: &mut Tx) -> Option<McpCall> { Some(call) }
    /// Expires timed-out in-flight plugin MCP calls; returns `true` when state changed.
    fn expire_plugin_calls(&mut self, _tx: &mut Tx) -> bool { false }
}

pub struct RoutedCall {
    pub target_session: String,
    pub call: McpCall,
}

/// What one window serves on one server. A window holds one for each server it has tabs on: the
/// one it shows routes calls, the others keep their folders' connections open (`update_window`).
struct WindowEntry {
    /// The server as the window's runtime names it (its tabs' `server`).
    runtime: ServerKey,
    server: Option<Server>,
    folders: BTreeSet<String>,
    routed_exact: Option<String>,
    routed_root: Option<String>,
    open_roots: HashSet<String>,
    known_parents: HashMap<String, Option<String>>,
    tx: Sender<RoutedCall>,
    waker: Waker,
}

struct HubInner {
    /// Per window, per server it has tabs on.
    windows: HashMap<(u64, EndpointKey), WindowEntry>,
    /// Open connections and whether `browser_*` tools were suppressed in the last pushed catalog.
    connections: HashMap<FolderKey, (ClientMcp, bool)>,
    routers: HashMap<EndpointKey, Sender<McpCall>>,
    /// `(window_id, EndpointKey)` pairs actively suppressing legacy `browser_*` tools.
    browser_rpc_attached: HashSet<(u64, EndpointKey)>,
    /// `(window_id, EndpointKey)` pairs where `experimental.browser` was explicitly proven
    /// `Unsupported` or `Unavailable` (legacy `browser_*` fails closed until present here).
    legacy_browser_allowed: HashSet<(u64, EndpointKey)>,
    /// The plugin tools generation the open connections listed (`tools::generation`).
    tools_generation: u64,
    reconnect_min: Duration,
    reconnect_max: Duration,
    unsupported_retry: Duration,
}

/// Coordinates per-folder `rune` client MCP WebSockets across all open windows in the process.
pub struct RuneMcpHub {
    inner: Mutex<HubInner>,
}

impl RuneMcpHub {
    pub fn new() -> Arc<Self> {
        Self::with_timings(RECONNECT_MIN, RECONNECT_MAX, UNSUPPORTED_RETRY)
    }

    pub fn with_timings(reconnect_min: Duration, reconnect_max: Duration, unsupported_retry: Duration) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(HubInner {
                windows: HashMap::new(),
                connections: HashMap::new(),
                routers: HashMap::new(),
                browser_rpc_attached: HashSet::new(),
                legacy_browser_allowed: HashSet::new(),
                tools_generation: tools::generation(),
                reconnect_min,
                reconnect_max,
                unsupported_retry,
            }),
        })
    }

    /// Updates `(window_id, server)` suppression vote: `Some(true)` suppresses legacy `browser_*`,
    /// `Some(false)` marks `server` proven `Unsupported`/`Unavailable`, and `None` clears the
    /// window's vote (reverting to fail-closed if no other window proved legacy fallback).
    pub fn set_browser_rpc_attached(self: &Arc<Self>, window_id: u64, server: &Server, suppressed: Option<bool>) {
        let ep = server.endpoint_key();
        let to_drop = {
            let mut inner = lock(&self.inner);
            let changed = match suppressed {
                Some(true) => {
                    let a = inner.browser_rpc_attached.insert((window_id, ep.clone()));
                    let b = inner.legacy_browser_allowed.remove(&(window_id, ep));
                    a || b
                }
                Some(false) => {
                    let a = inner.browser_rpc_attached.remove(&(window_id, ep.clone()));
                    let b = inner.legacy_browser_allowed.insert((window_id, ep));
                    a || b
                }
                None => {
                    let a = inner.browser_rpc_attached.remove(&(window_id, ep.clone()));
                    let b = inner.legacy_browser_allowed.remove(&(window_id, ep));
                    a || b
                }
            };
            if !changed {
                return;
            }
            Self::reconcile_locked(self, &mut inner)
        };
        close_later(to_drop);
    }

    fn browser_tools_refusal(&self, server: &Server) -> Option<&'static str> {
        if !server.is_local() {
            return Some(REMOTE_BROWSER_MCP_ERROR);
        }
        let inner = lock(&self.inner);
        let ep = server.endpoint_key();
        let any_remote = inner
            .windows
            .iter()
            .any(|((_, k), win)| *k == ep && !win.runtime.is_sidecar());
        if any_remote {
            return Some(REMOTE_BROWSER_MCP_ERROR);
        }
        let suppressed = inner.browser_rpc_attached.iter().any(|(_, k)| *k == ep)
            || !inner.legacy_browser_allowed.iter().any(|(_, k)| *k == ep);
        suppressed.then_some(RPC_ATTACHED_BROWSER_MCP_ERROR)
    }

    /// The window shows `entry`'s server now. Its entries for the other servers it still has tabs on
    /// (`keep`) stay, routing nothing, so a tab switch between servers keeps every server's
    /// connections open (Desktop's TUI holds its one connection until what it serves changes); an
    /// entry for a server the window no longer has tabs on goes, and its connections with it.
    fn update_window(self: &Arc<Self>, window_id: u64, entry: WindowEntry, keep: &HashSet<ServerKey>) {
        let to_drop = {
            let mut inner = lock(&self.inner);
            let Some(key) = entry.server.as_ref().map(Server::endpoint_key) else { return };
            inner.windows.retain(|(w, _), win| *w != window_id || keep.contains(&win.runtime));
            for win in inner.windows.iter_mut().filter_map(|((w, k), win)| (*w == window_id && *k != key).then_some(win)) {
                win.routed_exact = None;
                win.routed_root = None;
                win.open_roots.clear();
            }
            inner.windows.insert((window_id, key), entry);
            Self::reconcile_locked(self, &mut inner)
        };
        close_later(to_drop);
    }

    fn remove_window(self: &Arc<Self>, window_id: u64) {
        let to_drop = {
            let mut inner = lock(&self.inner);
            let before_w = inner.windows.len();
            let before_s = inner.browser_rpc_attached.len();
            let before_l = inner.legacy_browser_allowed.len();
            inner.windows.retain(|(w, _), _| *w != window_id);
            inner.browser_rpc_attached.retain(|(w, _)| *w != window_id);
            inner.legacy_browser_allowed.retain(|(w, _)| *w != window_id);
            if inner.windows.len() == before_w
                && inner.browser_rpc_attached.len() == before_s
                && inner.legacy_browser_allowed.len() == before_l
            {
                return;
            }
            Self::reconcile_locked(self, &mut inner)
        };
        close_later(to_drop);
    }

    fn reconcile_locked(hub: &Arc<Self>, inner: &mut HubInner) -> Vec<ClientMcp> {
        let mut wanted: HashMap<FolderKey, Server> = HashMap::new();
        let mut remote_endpoints: HashSet<EndpointKey> = HashSet::new();
        for win in inner.windows.values() {
            let Some(srv) = &win.server else { continue };
            let ep = srv.endpoint_key();
            if !win.runtime.is_sidecar() || !srv.is_local() {
                remote_endpoints.insert(ep.clone());
            }
            for dir in &win.folders {
                if !dir.is_empty() {
                    wanted.entry((ep.clone(), dir.clone())).or_insert_with(|| srv.clone());
                }
            }
        }

        let mut removed = Vec::new();
        let generation = tools::generation();
        let retool = std::mem::replace(&mut inner.tools_generation, generation) != generation;
        let all_tools = tools::catalog();
        let filtered_tools: Vec<_> = all_tools
            .iter()
            .filter(|t| !is_browser_tool(&t.name))
            .cloned()
            .collect();
        let suppress_for = |ep: &EndpointKey| -> bool {
            remote_endpoints.contains(ep)
                || inner.browser_rpc_attached.iter().any(|(_, k)| k == ep)
                || !inner.legacy_browser_allowed.iter().any(|(_, k)| k == ep)
        };

        // OpenCode's attached MCP client refreshes on `notifications/tools/list_changed`
        // (`packages/core/src/mcp/index.ts` `attach`), so open connections announce the new list
        // in place and keep calls in flight; reconnect only when a client lacks `list_changed`.
        let mut stale: Vec<FolderKey> = Vec::new();
        for (k, (conn, was_suppressed)) in &mut inner.connections {
            if !wanted.contains_key(k) {
                stale.push(k.clone());
                continue;
            }
            let want_suppressed = suppress_for(&k.0);
            if retool || *was_suppressed != want_suppressed {
                let next_tools = if want_suppressed {
                    filtered_tools.clone()
                } else {
                    all_tools.clone()
                };
                if conn.set_tools(next_tools) {
                    *was_suppressed = want_suppressed;
                } else {
                    stale.push(k.clone());
                }
            }
        }
        for k in stale {
            if let Some((conn, _)) = inner.connections.remove(&k) {
                removed.push(conn);
            }
        }

        inner
            .routers
            .retain(|srv_key, _| wanted.keys().any(|(ep, _)| ep == srv_key));

        for (key, srv) in wanted {
            if inner.connections.contains_key(&key) {
                continue;
            }
            let srv_key = key.0.clone();
            let call_tx = match inner.routers.get(&srv_key) {
                Some(tx) => tx.clone(),
                None => {
                    let (tx, rx) = channel();
                    let weak = Arc::downgrade(hub);
                    let router_srv = srv.clone();
                    // A thread natively; drained between a page's events in a browser (`rune::task::serve`).
                    let mut parent_cache = HashMap::new();
                    drop(rune::task::serve("desktop-rune-mcp-router", rx, move |call| route(&weak, &router_srv, &mut parent_cache, call)));
                    inner.routers.insert(srv_key, tx.clone());
                    tx
                }
            };

            let suppress = suppress_for(&key.0);
            let dir = key.1.clone();
            let catalog = if suppress { filtered_tools.clone() } else { all_tools.clone() };
            let mut opts = ClientMcpOptions::new(MCP_NAME, dir, catalog).with_instructions(MCP_INSTRUCTIONS);
            opts.reconnect_min = inner.reconnect_min;
            opts.reconnect_max = inner.reconnect_max;
            opts.unsupported_retry = inner.unsupported_retry;

            let weak_for_wake = Arc::downgrade(hub);
            let wake_srv = srv.clone();
            let waker: Waker = Arc::new(move || {
                if let Some(h) = weak_for_wake.upgrade() {
                    let wakers: Vec<Waker> = lock(&h.inner)
                        .windows
                        .values()
                        .filter(|w| w.server.as_ref().is_some_and(|s| s.same_target(&wake_srv)))
                        .map(|w| w.waker.clone())
                        .collect();
                    for w in wakers {
                        w();
                    }
                }
            });

            let conn = srv.connect_client_mcp_with(opts, call_tx, waker);
            inner.connections.insert(key, (conn, suppress));
        }

        removed
    }
}

/// Hands one `tools/call` to the window whose session (or an ancestor of it) asked; breaks once
/// the hub is gone.
fn route(weak_hub: &Weak<RuneMcpHub>, server: &Server, parent_cache: &mut HashMap<(String, String), Option<String>>, call: McpCall) -> ControlFlow<()> {
    let Some(hub) = weak_hub.upgrade() else {
        call.respond(McpResult::error(SESSION_NOT_OPEN_ERROR));
        return ControlFlow::Break(());
    };
    // Hard enforcement at execution time (not merely hiding from `tools/list`): refuse `browser_*`
    // and raw `browser_cdp` on remote servers and on servers attached via `experimental.browser` v4.
    if is_browser_tool(&call.name)
        && let Some(reason) = hub.browser_tools_refusal(server)
    {
        call.respond(McpResult::error(reason));
        return ControlFlow::Continue(());
    }
    let Some(sid) = call.session_id.clone().filter(|s| !s.is_empty()) else {
        call.respond(McpResult::error(SESSION_NOT_OPEN_ERROR));
        return ControlFlow::Continue(());
    };

    if let Some((tx, waker, target_session)) = match_window(&hub, server, &sid, parent_cache) {
        if tx.send(RoutedCall { target_session, call }).is_ok() {
            waker();
        }
        return ControlFlow::Continue(());
    }

    // Subagent sessions may call a `rune` tool milliseconds before their `SessionInfo` arrives on
    // SSE. Walk `/api/session/<id>` off-thread to discover ancestors if any window has open tabs.
    if should_probe_ancestors(&hub, server, &sid, parent_cache) {
        fetch_parent_chain(server, &sid, parent_cache);
        if let Some((tx, waker, target_session)) = match_window(&hub, server, &sid, parent_cache) {
            if tx.send(RoutedCall { target_session, call }).is_ok() {
                waker();
            }
            return ControlFlow::Continue(());
        }
    }

    call.respond(McpResult::error(SESSION_NOT_OPEN_ERROR));
    ControlFlow::Continue(())
}

fn match_window(
    hub: &RuneMcpHub,
    server: &Server,
    sid: &str,
    extra_parents: &HashMap<(String, String), Option<String>>,
) -> Option<(Sender<RoutedCall>, Waker, String)> {
    let inner = lock(&hub.inner);
    let mut merged_parents: HashMap<&str, Option<&str>> = HashMap::new();
    for ((base, k), v) in extra_parents {
        if base == &server.base {
            merged_parents.insert(k.as_str(), v.as_deref());
        }
    }
    for win in inner.windows.values() {
        if win.server.as_ref().is_some_and(|s| s.same_target(server)) {
            for (k, v) in &win.known_parents {
                merged_parents.insert(k.as_str(), v.as_deref());
            }
        }
    }

    let root = resolve_root(sid, &merged_parents);
    let mut best: Option<(u8, u64, Sender<RoutedCall>, Waker, String)> = None;

    for (&(wid, _), win) in &inner.windows {
        if !win.server.as_ref().is_some_and(|s| s.same_target(server)) {
            continue;
        }
        let score = if win.routed_exact.as_deref() == Some(sid) {
            4
        } else if win.routed_root.as_deref() == Some(root) || win.routed_exact.as_deref() == Some(root) {
            3
        } else if win.open_roots.contains(sid) {
            2
        } else if win.open_roots.contains(root) {
            1
        } else {
            0
        };
        if score == 0 {
            continue;
        }
        let target = if win.routed_exact.as_deref() == Some(sid) || win.open_roots.contains(sid) {
            sid.to_owned()
        } else if let Some(routed) = &win.routed_exact
            && resolve_root(routed, &merged_parents) == root
        {
            routed.clone()
        } else {
            root.to_owned()
        };
        let replace = match &best {
            None => true,
            Some((best_score, best_wid, ..)) => score > *best_score || (score == *best_score && wid < *best_wid),
        };
        if replace {
            best = Some((score, wid, win.tx.clone(), win.waker.clone(), target));
        }
    }

    best.map(|(_, _, tx, waker, target)| (tx, waker, target))
}

fn resolve_root<'a>(sid: &'a str, parents: &HashMap<&'a str, Option<&'a str>>) -> &'a str {
    let mut cur = sid;
    for _ in 0..32 {
        match parents.get(cur).copied().flatten() {
            Some(p) if !p.is_empty() && p != cur => cur = p,
            _ => break,
        }
    }
    cur
}

fn should_probe_ancestors(
    hub: &RuneMcpHub,
    server: &Server,
    sid: &str,
    cache: &HashMap<(String, String), Option<String>>,
) -> bool {
    if cache.contains_key(&(server.base.clone(), sid.to_owned())) {
        return false;
    }
    let inner = lock(&hub.inner);
    let mut has_open = false;
    for win in inner.windows.values() {
        if !win.server.as_ref().is_some_and(|s| s.same_target(server)) {
            continue;
        }
        if win.known_parents.contains_key(sid) {
            return false;
        }
        if !win.open_roots.is_empty() {
            has_open = true;
        }
    }
    has_open
}

fn fetch_parent_chain(server: &Server, sid: &str, cache: &mut HashMap<(String, String), Option<String>>) {
    if cache.len() > 512 {
        cache.clear();
    }
    let mut cur = sid.to_owned();
    for _ in 0..8 {
        let key = (server.base.clone(), cur.clone());
        if let Some(cached) = cache.get(&key) {
            match cached {
                Some(next) => {
                    cur = next.clone();
                    continue;
                }
                None => break,
            }
        }
        let Ok(v) = server.get_timeout(&format!("/api/session/{cur}"), Duration::from_millis(750)) else {
            break;
        };
        let data = v.get("data").unwrap_or(&v);
        let parent = data["parentID"].as_str().filter(|s| !s.is_empty()).map(str::to_owned);
        cache.insert(key, parent.clone());
        match parent {
            Some(next) if next != cur => cur = next,
            _ => break,
        }
    }
}

/// Per-window client MCP state stored on the window host.
pub struct WindowMcp {
    window_id: u64,
    hub: Arc<RuneMcpHub>,
    waker: Waker,
    tx: Sender<RoutedCall>,
    rx: Receiver<RoutedCall>,
    registration: Registration,
    force_enabled: bool,
}

impl WindowMcp {
    pub fn window_id(&self) -> u64 {
        self.window_id
    }

    pub fn force_enabled(&self) -> bool {
        self.force_enabled
    }

    pub fn set_force_enabled(&mut self, enabled: bool) {
        self.force_enabled = enabled;
    }
}

impl Default for WindowMcp {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self {
            window_id: NEXT_WINDOW_ID.fetch_add(1, Ordering::Relaxed),
            hub: global_hub(),
            waker: Arc::new(|| {}),
            tx,
            rx,
            registration: Registration::Absent,
            force_enabled: false,
        }
    }
}

impl Drop for WindowMcp {
    fn drop(&mut self) {
        self.hub.remove_window(self.window_id);
    }
}

#[derive(Default)]
enum Registration { #[default] Absent, Published(u64) }

enum Publish { Keep, Remove, Update }

impl Registration {
    fn step(&mut self, stamp: Option<u64>) -> Publish {
        match (&*self, stamp) {
            (Self::Absent, None) => Publish::Keep,
            (Self::Published(was), Some(now)) if *was == now => Publish::Keep,
            (_, None) => { *self = Self::Absent; Publish::Remove }
            (_, Some(now)) => { *self = Self::Published(now); Publish::Update }
        }
    }
}

pub fn setup_rune_mcp(host: &mut dyn RuneMcpHost, waker: Waker) {
    host.rune_mcp_mut().waker = waker;
    close_rune_mcp(host);
    sync_window(host);
}

pub fn close_rune_mcp(host: &mut dyn RuneMcpHost) {
    let mcp = host.rune_mcp_mut();
    mcp.hub.remove_window(mcp.window_id);
    mcp.registration = Registration::Absent;
}

pub fn poll(host: &mut dyn RuneMcpHost, tx: &mut Tx) -> bool {
    sync_window(host);
    let mut changed = false;
    loop {
        let Ok(routed) = host.rune_mcp().rx.try_recv() else { break };
        tools::execute(host, &routed.target_session, routed.call, tx);
        changed = true;
    }
    if host.expire_plugin_calls(tx) {
        changed = true;
    }
    changed
}

fn mcp_active(host: &dyn RuneMcpHost) -> Option<&Server> {
    if host.is_launch_waiting() || host.is_stage() {
        return None;
    }
    let (server, allow_send) = host.live_server()?;
    if host.rune_mcp().force_enabled {
        return Some(server);
    }
    // Headless `--client-mcp` is strictly for local mock servers, never the default `:4096` server.
    if host.is_headless() {
        return (host.client_mcp_flag() && server.base != DEFAULT_SERVER).then_some(server);
    }
    allow_send.then_some(server)
}

fn sync_stamp(host: &dyn RuneMcpHost, server: &Server) -> u64 {
    let mut h = DefaultHasher::new();
    server.endpoint_key().hash(&mut h);
    tools::generation().hash(&mut h);
    host.route().hash(&mut h);
    host.draft_dir().hash(&mut h);
    host.used_project_ids().hash(&mut h);
    let projects = host.projects();
    projects.len().hash(&mut h);
    for p in projects {
        p.id.hash(&mut h);
        p.directory.hash(&mut h);
    }
    let tab_count = host.tab_count();
    tab_count.hash(&mut h);
    for i in 0..tab_count {
        if let Some(t) = host.tab_at(i) {
            t.id.hash(&mut h);
            t.directory.hash(&mut h);
        }
    }
    host.sessions_version().hash(&mut h);
    h.finish()
}

fn sync_window(host: &mut dyn RuneMcpHost) {
    let active_srv = mcp_active(host).cloned();
    let stamp = active_srv.as_ref().map(|server| sync_stamp(host, server));
    match host.rune_mcp_mut().registration.step(stamp) {
        Publish::Keep => return,
        Publish::Remove => {
            let mcp = host.rune_mcp();
            mcp.hub.remove_window(mcp.window_id);
            return;
        }
        Publish::Update => (),
    }
    let Some(server) = active_srv else { return };

    let mut folders = BTreeSet::new();
    let mut add = |raw: &str| {
        let d = clean_dir(raw);
        if !d.is_empty() {
            folders.insert(d);
        }
    };
    for p in host.used_projects() {
        add(&p.directory);
    }
    let tab_count = host.tab_count();
    for i in 0..tab_count {
        if let Some(t) = host.tab_at(i) {
            add(host.session(t.id).map_or(t.directory, |info| info.directory.as_str()));
        }
    }
    match host.route() {
        McpRoute::Session { id: sid, .. } => {
            if let Some(info) = host.session(sid) {
                add(&info.directory);
            }
        }
        McpRoute::Draft(_) => add(host.draft_dir()),
        McpRoute::Home => {}
    }

    let mut open_roots: HashSet<String> = (0..tab_count).filter_map(|i| host.tab_at(i).map(|t| t.id.to_owned())).collect();
    let (routed_exact, routed_root) = match host.route() {
        McpRoute::Session { id: sid, .. } => {
            let root = host.root_of(sid).to_owned();
            open_roots.insert(sid.to_owned());
            open_roots.insert(root.clone());
            (Some(sid.to_owned()), Some(root))
        }
        _ => (None, None),
    };

    let known_parents: HashMap<String, Option<String>> =
        host.sessions().iter().map(|i| (i.id.clone(), i.parent.clone())).collect();

    let active = host.active_server().clone();
    // The servers the window has tabs on keep their entries (and connections) while it shows another.
    let mut keep: HashSet<ServerKey> = (0..tab_count).filter_map(|i| host.tab_at(i).map(|t| t.server.clone())).collect();
    keep.insert(active.clone());
    let mcp = host.rune_mcp();
    let entry = WindowEntry {
        runtime: active,
        server: Some(server),
        folders,
        routed_exact,
        routed_root,
        open_roots,
        known_parents,
        tx: mcp.tx.clone(),
        waker: mcp.waker.clone(),
    };
    mcp.hub.update_window(mcp.window_id, entry, &keep);
}

/// Closes connections a window no longer wants on a thread of their own: a close joins the
/// connection's thread after its WebSocket close, which took 330-820 ms on the UI thread for every
/// tab switch to another server (the window's folders move to that server's connections).
fn close_later(conns: Vec<ClientMcp>) {
    if conns.is_empty() {
        return;
    }
    #[cfg(not(target_arch = "wasm32"))]
    drop(std::thread::Builder::new().name("desktop-rune-mcp-close".into()).spawn(move || drop(conns)));
    #[cfg(target_arch = "wasm32")]
    drop(conns);
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}
