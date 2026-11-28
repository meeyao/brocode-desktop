//! Home across servers (`home/model.ts`, `home/projects/controller.tsx`,
//! `home/sessions/load-machine.ts`):
//! - **Selection:** which server Home focuses and which of its used projects is selected, run by
//!   MS-1's [`opencode_kit::servers::home`] machine against the servers Home lists. The window
//!   works against the selected server while Home shows (`servers::runtime::desired`).
//! - **Servers:** each listed server's name, badge, health and used projects, read once a poll
//!   ([`Home::servers`]) for the projects column, and each one's collapse state and whether it's
//!   hidden from Home, kept for every window in the app's database (Desktop's global `home.servers`
//!   and `server.hidden`), as is the selection (`layout.home.selection`).
//! - **Session list:** the focused server's list phase, run by the [`home_load`] machine: its
//!   placeholder rows while the server connects, its Unreachable, Signed out and Incompatible
//!   states, and a list already shown staying while its server is away.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::hash::{DefaultHasher, Hash, Hasher};

use opencode_kit::model::Project;
use opencode_kit::servers::ServerKey;
use opencode_servers::ServerConn;
use opencode_servers::health::{self as server_health, Health, ProbeResult};
use opencode_servers::home::{self as machine, Selection};
use opencode_servers::home_load::{self, Connection, Target, Timer, Unavailable};
use opencode_platform::storage;
use rune::machine::{Attempt, Effects};
use rune::{TextEdit, Tx};
use serde_json::{Map, Value, json};

use crate::host::{ListedServer, ProjectsHost, on_server, storage_update_host};
use crate::{open_project_dir, organized_projects, project_for_session, remote_picker, used_projects};

/// One server as Home's projects column shows it.
#[derive(Clone, Debug)]
pub struct ServerView {
    pub key: ServerKey,
    pub conn: ServerConn,
    /// `servers.label`.
    pub label: String,
    pub health: Option<Health>,
    /// Its used projects, most recently active first.
    pub projects: Vec<Project>,
    /// Visible remote project rows, read on their own checkout.
    pub sessions: BTreeMap<String, Vec<(String, String)>>,
    /// Its first reads landed.
    pub loaded: bool,
}

impl ServerView {
    pub fn healthy(&self) -> bool {
        server_health::is_healthy(self.health.as_ref())
    }

    pub fn failed(&self) -> bool {
        server_health::is_failed(self.health.as_ref())
    }

    pub fn incompatible(&self) -> bool {
        server_health::is_incompatible(self.health.as_ref())
    }

    /// Its plugin asks for a sign-in, or an added server rejects its saved one (`signInRequired`).
    pub fn sign_in_required(&self) -> bool {
        match &self.conn {
            ServerConn::Plugin { status, .. } => status.http().is_none() && self.health == Some(Health::Auth),
            ServerConn::Http { .. } => server_health::is_unauthorized(self.health.as_ref()),
            ServerConn::Sidecar { .. } => false,
        }
    }

    /// Its plugin is starting it (`connecting`).
    pub fn connecting(&self) -> bool {
        matches!(&self.conn, ServerConn::Plugin { .. }) && self.health == Some(Health::Starting)
    }
}

/// Project-navigation browser modal state (`home/projects/{landing,start,browse-view}.tsx`).
#[derive(Default)]
pub enum Browser {
    #[default]
    Closed,
    Projects {
        search: TextEdit,
        selected: usize,
        restore_focus: Option<String>,
        restore_composer: bool,
    },
    History {
        search: TextEdit,
        server: ServerKey,
        project: Project,
        sessions: Vec<(String, String)>,
        selected: usize,
        restore_focus: Option<String>,
        restore_composer: bool,
    },
}

/// An armed `home_load` timer and the event it steps when it fires.
#[derive(Clone, Debug, Default, PartialEq)]
enum Armed {
    #[default]
    Idle,
    At { due: f64, event: home_load::Event },
}

/// The project add Home asked a server for, until its project shows among the server's used ones
/// (`project.add` → `project.added`).
#[derive(Clone, Debug, Default, PartialEq)]
enum Adding {
    #[default]
    Idle,
    Waiting { attempt: Attempt, server: ServerKey, directory: String },
}

/// `home/projects/landing.tsx` restores once per visit. A slow or failed session open
/// must not enqueue the same navigation on every poll.
#[derive(Default, PartialEq)]
enum Landing {
    #[default]
    Away,
    Visited,
}

enum Visit {
    Left,
    Arrived(Option<String>),
}

/// A window's Home.
#[derive(Default)]
pub struct Home {
    pub last_project: Option<(ServerKey, String)>,
    pub browser: Browser,
    landing: Landing,
    machine: machine::State,
    load: home_load::State,
    timers: [Armed; 3],
    adding: Adding,
    /// The servers Home lists, as of the last poll.
    pub servers: Vec<ServerView>,
    /// The store's version the machine last read.
    seen: u64,
    /// Stamp of every host and store input as of the last [`sync`]; `None` before the first sync.
    sync_stamp: Option<u64>,
}

/// What every window shares, saved in Desktop's global keys.
#[derive(Default)]
struct Store {
    hydrated: bool,
    selection: Selection,
    collapsed: BTreeMap<ServerKey, bool>,
    hidden: BTreeSet<ServerKey>,
    version: u64,
}

thread_local! {
    static STORE: RefCell<Store> = RefCell::new(Store::default());
}

/// Desktop's global keys: `layout.home.selection`, `home.servers.collapsed` and `server.hidden`.
pub const GLOBAL: &str = "opencode.global.dat";

fn parse_store(read: impl Fn(&str, &str) -> Option<Value>) -> Store {
    let json = |key: &str| read(GLOBAL, key).unwrap_or(Value::Null);
    let (layout, servers, server) = (json("layout"), json("home.servers"), json("server"));
    let key = |v: &Value| v.as_str().filter(|k| !k.is_empty()).map(ServerKey::new);
    let sel = &layout["home"]["selection"];
    let selection = Selection::new(key(&sel["server"]), sel["directory"].as_str().map(str::to_owned));
    let collapsed = servers["collapsed"]
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(k, b)| Some((ServerKey::new(k), b.as_bool()?)))
        .collect();
    let hidden = server["hidden"]
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(_, b)| b.as_bool() == Some(true))
        .map(|(k, _)| ServerKey::new(k))
        .collect();
    Store { hydrated: true, selection, collapsed, hidden, version: 0 }
}

fn ensure_hydrated_ops() {
    if !storage::is_configured() || STORE.with_borrow(|st| st.hydrated) {
        return;
    }
    let loaded = parse_store(storage::read_json);
    STORE.with_borrow_mut(|st| {
        if !st.hydrated {
            let version = st.version.wrapping_add(1);
            *st = Store { version, ..loaded };
        }
    });
}

fn ensure_hydrated_host(host: &dyn ProjectsHost) {
    if STORE.with_borrow(|st| st.hydrated) {
        return;
    }
    let loaded = if storage::is_configured() {
        parse_store(storage::read_json)
    } else {
        parse_store(|ns, k| host.storage_read_json(ns, k))
    };
    STORE.with_borrow_mut(|st| {
        if !st.hydrated {
            let version = st.version.wrapping_add(1);
            *st = Store { version, ..loaded };
        }
    });
}

/// Clears the hydration flag so the next read re-loads Home's global store from storage.
pub fn reload_store() {
    STORE.with_borrow_mut(|st| {
        st.hydrated = false;
        st.version = st.version.wrapping_add(1);
    });
}

type SerializedStore = (Value, Map<String, Value>, Map<String, Value>);

fn serialize_parts(store: &Store) -> SerializedStore {
    let selection = json!({
        "server": store.selection.server.as_ref().map(ServerKey::as_str),
        "directory": store.selection.directory,
    });
    let collapsed: Map<String, Value> = store.collapsed.iter().map(|(k, b)| (k.as_str().to_owned(), json!(b))).collect();
    let hidden: Map<String, Value> = store.hidden.iter().map(|k| (k.as_str().to_owned(), json!(true))).collect();
    (selection, collapsed, hidden)
}

/// Each part goes to its own key; a part that didn't change writes nothing.
fn save_ops((selection, collapsed, hidden): SerializedStore) {
    if !storage::attended() {
        return;
    }
    storage::update_json(GLOBAL, "layout", |m| {
        let home = m.entry("home").or_insert_with(|| json!({}));
        if let Some(home) = home.as_object_mut() {
            home.insert("selection".into(), selection);
        }
    });
    storage::update_json(GLOBAL, "home.servers", |m| {
        m.insert("collapsed".into(), Value::Object(collapsed));
    });
    storage::update_json(GLOBAL, "server", |m| {
        m.insert("hidden".into(), Value::Object(hidden));
    });
}

fn save_host(host: &mut dyn ProjectsHost, (selection, collapsed, hidden): SerializedStore) {
    if !host.attended() {
        return;
    }
    storage_update_host(host, GLOBAL, "layout", |m| {
        let home = m.entry("home").or_insert_with(|| json!({}));
        if let Some(home) = home.as_object_mut() {
            home.insert("selection".into(), selection);
        }
    });
    storage_update_host(host, GLOBAL, "home.servers", |m| {
        m.insert("collapsed".into(), Value::Object(collapsed));
    });
    storage_update_host(host, GLOBAL, "server", |m| {
        m.insert("hidden".into(), Value::Object(hidden));
    });
}

fn write_ops(f: impl FnOnce(&mut Store)) {
    ensure_hydrated_ops();
    let parts = STORE.with_borrow_mut(|s| {
        f(s);
        s.version += 1;
        serialize_parts(s)
    });
    save_ops(parts);
}

fn write_host(host: &mut dyn ProjectsHost, f: impl FnOnce(&mut Store)) {
    ensure_hydrated_host(host);
    let parts = STORE.with_borrow_mut(|s| {
        f(s);
        s.version += 1;
        serialize_parts(s)
    });
    save_host(host, parts);
}

/// The servers hidden from Home ("Hide from project list").
pub fn hidden() -> BTreeSet<ServerKey> {
    ensure_hydrated_ops();
    STORE.with_borrow(|s| s.hidden.clone())
}

pub fn set_hidden(key: &ServerKey, hide: bool) {
    write_ops(|s| {
        if hide {
            s.hidden.insert(key.clone());
        } else {
            s.hidden.remove(key);
        }
    });
}

/// Same as [`set_hidden`], persisting through `host` directly even when
/// [`opencode_platform::storage::configure`] was not called.
pub fn set_hidden_on(host: &mut dyn ProjectsHost, key: &ServerKey, hide: bool) {
    write_host(host, |s| {
        if hide {
            s.hidden.insert(key.clone());
        } else {
            s.hidden.remove(key);
        }
    });
}

/// Server `key`'s projects are collapsed on Home (`home.servers.collapsed`).
pub fn collapsed(key: &ServerKey) -> bool {
    ensure_hydrated_ops();
    STORE.with_borrow(|s| s.collapsed.get(key).copied().unwrap_or(false))
}

/// The collapse chevron: only a healthy server with projects toggles.
pub fn toggle_collapsed(key: &ServerKey, tx: &mut Tx) {
    tx.instant();
    write_ops(|s| {
        let next = !s.collapsed.get(key).copied().unwrap_or(false);
        s.collapsed.insert(key.clone(), next);
    });
}

/// Same as [`toggle_collapsed`], persisting through `host` directly.
pub fn toggle_collapsed_on(host: &mut dyn ProjectsHost, key: &ServerKey, tx: &mut Tx) {
    tx.instant();
    write_host(host, |s| {
        let next = !s.collapsed.get(key).copied().unwrap_or(false);
        s.collapsed.insert(key.clone(), next);
    });
}

/// The servers Home lists: those not hidden from it, and when every one is, the app's own anyway
/// (`resolveVisibleServers`).
pub fn visible_servers(host: &dyn ProjectsHost) -> Vec<ListedServer> {
    ensure_hydrated_host(host);
    let all = host.listed_servers();
    let hidden = STORE.with_borrow(|s| s.hidden.clone());
    let shown: Vec<ListedServer> = all.iter().filter(|e| !hidden.contains(&e.key())).cloned().collect();
    if !shown.is_empty() || all.is_empty() {
        return shown;
    }
    all.into_iter().take(1).collect()
}

/// What Home shows as selected (`selectHomeSelection`).
pub fn selection(host: &dyn ProjectsHost) -> Selection {
    host.home_state().machine.selection()
}

/// The server Home shows: the selection's, else the app's own.
pub fn server(host: &dyn ProjectsHost) -> ServerKey {
    if host.project_navigation()
        && let Some((key, _)) = &host.home_state().last_project
        && host.home_state().servers.iter().any(|v| v.key == *key)
    {
        return key.clone();
    }
    selection(host).server.unwrap_or_else(ServerKey::sidecar)
}

/// Home's selected project, on the server the window works against (`packages/app/src/home/model.ts:132-135`).
pub fn selected(host: &dyn ProjectsHost) -> Option<&Project> {
    let sel = selection(host);
    if sel.server.as_ref() != Some(&host.active_server()) {
        return None;
    }
    let dir = machine::same_directory_key(sel.directory.as_deref()?);
    used_projects(host).into_iter().find(|p| machine::same_directory_key(&p.directory) == dir)
}

/// Its id.
pub fn selected_id(host: &dyn ProjectsHost) -> Option<String> {
    selected(host).map(|p| p.id.clone())
}

/// Home focuses server `key`, all of its projects (`focusServer`).
pub fn focus(host: &mut dyn ProjectsHost, key: &ServerKey) {
    send(host, machine::Event::ServerFocused(key.clone()));
}

/// A project row toggles its project on `server` (`project.toggled`).
pub fn toggle(host: &mut dyn ProjectsHost, server: &ServerKey, directory: &str) {
    send(host, machine::Event::ProjectToggled { server: server.clone(), directory: directory.to_owned() });
}

/// Another part of the app selects `directory` on `server` (a session's "Show in Home"): Home
/// takes it as an outside write.
pub fn show(host: &mut dyn ProjectsHost, server: &ServerKey, directory: Option<&str>) {
    let sel = Selection::new(Some(server.clone()), directory.map(str::to_owned));
    write_host(host, |st| st.selection = sel);
    sync(host);
}

/// Adds `directories` on `server` as projects; the first one's project is selected once it shows
/// (`home.project.add`).
pub fn add(host: &mut dyn ProjectsHost, server: &ServerKey, directories: Vec<String>, tx: &mut Tx) {
    let fx = step(host, machine::Event::ProjectAdd { server: server.clone(), directories });
    run(host, fx, tx);
}

/// Steps `event` after the servers' latest state, as Desktop's `send` does: a user action can
/// follow a server change in the same update.
fn send(host: &mut dyn ProjectsHost, event: machine::Event) {
    sync(host);
    let fx = step(host, event);
    run(host, fx, &mut Tx::default());
}

fn step(host: &mut dyn ProjectsHost, event: machine::Event) -> Effects<machine::Effect> {
    let fx = machine::step(&mut host.home_state_mut().machine, event);
    follow(host);
    fx
}

/// Keep the window checkout aligned with Home's focused server (`server(host)`, matching
/// `servers::runtime::desired`) while Home shows.
fn follow(host: &mut dyn ProjectsHost) {
    if !host.is_home_route() || host.settings_open() {
        return;
    }
    let want = server(host);
    if want != host.active_server() {
        host.switch_server(&want);
    }
}

/// Stamp of every host and Home-store input that [`sync`] reads.
fn sync_stamp(host: &dyn ProjectsHost) -> u64 {
    let mut h = DefaultHasher::new();
    host.home_sync_stamp().hash(&mut h);
    STORE.with_borrow(|st| (st.hydrated, st.version).hash(&mut h));
    match &host.home_state().adding {
        Adding::Idle => 0u64.hash(&mut h),
        Adding::Waiting { attempt, .. } => (1u64, attempt.0).hash(&mut h),
    }
    h.finish()
}

/// The servers Home lists, read from each (`env`), and the machine told.
fn sync(host: &mut dyn ProjectsHost) {
    ensure_hydrated_host(host);
    let entries = visible_servers(host);
    let mut servers = Vec::with_capacity(entries.len());
    for e in entries {
        let key = e.key();
        let read = |h: &mut dyn ProjectsHost| {
            let hr: &dyn ProjectsHost = h;
            let mut sessions = BTreeMap::new();
            let projects: Vec<Project> = organized_projects(hr).into_iter().cloned().collect();
            for p in &projects {
                if !hr.project_state().organize_expanded.contains(&format!("{key}:{}", p.id)) {
                    continue;
                }
                let mut rows: Vec<(i64, String, String)> = hr
                    .sessions()
                    .iter()
                    .filter(|i| i.parent.is_none() && i.archived.is_none() && project_for_session(hr, i).is_some_and(|proj| proj.id == p.id))
                    .map(|i| (hr.session_active_ms(i), i.id.clone(), i.title.clone()))
                    .collect();
                rows.sort_by_key(|(ms, ..)| std::cmp::Reverse(*ms));
                sessions.insert(p.id.clone(), rows.into_iter().map(|(_, id, title)| (id, title)).collect());
            }
            (used_projects(hr).into_iter().cloned().collect::<Vec<_>>(), sessions, loaded(hr, &key))
        };
        let (projects, sessions, is_loaded) = on_server(host, &key, read).unwrap_or_default();
        servers.push(ServerView {
            label: host.server_label(&e.conn),
            health: host.server_health(&key),
            key,
            conn: e.conn,
            projects,
            sessions,
            loaded: is_loaded,
        });
    }
    let (stored, version) = STORE.with_borrow(|st| (st.selection.clone(), st.version));
    let env = machine::Env {
        hydrated: !host.is_live() || host.server_sources_hydrated(),
        stored,
        servers: servers
            .iter()
            .map(|v| machine::ServerSnapshot {
                key: v.key.clone(),
                failed: v.failed(),
                connected: v.loaded && !v.failed(),
                projects_ready: v.loaded,
                used_projects: v.projects.iter().map(|p| machine::Project { id: p.id.clone(), worktree: p.directory.clone() }).collect(),
            })
            .collect(),
    };
    {
        let st = host.home_state_mut();
        st.servers = servers;
        st.seen = version;
    }
    let fx = step(host, machine::Event::EnvSynced(env));
    run(host, fx, &mut Tx::default());
    added(host);
    let stamp = sync_stamp(host);
    host.home_state_mut().sync_stamp = Some(stamp);
}

/// The server's first reads landed (or there's no server to read: a fixture).
fn loaded(host: &dyn ProjectsHost, key: &ServerKey) -> bool {
    !host.is_live() || host.is_server_connected(key)
}

fn run(host: &mut dyn ProjectsHost, fx: Effects<machine::Effect>, tx: &mut Tx) {
    for effect in fx {
        match effect {
            machine::Effect::SelectionPersist(sel) => {
                write_host(host, |st| st.selection = sel);
                host.home_state_mut().seen = STORE.with_borrow(|st| st.version);
            }
            machine::Effect::ProjectAdd { attempt, server, directories } => {
                let Some(first) = directories.first().cloned() else { continue };
                host.home_state_mut().adding = Adding::Waiting { attempt, server: server.clone(), directory: first };
                let _ = on_server(host, &server, |h| {
                    for dir in &directories {
                        open_project_dir(h, dir, false, tx);
                    }
                });
            }
            machine::Effect::WorktreesList { attempt, server, project_id } => {
                let _ = on_server(host, &server, |h| h.list_worktrees(&project_id));
                let fx = machine::step(&mut host.home_state_mut().machine, machine::Event::WorktreesListed { attempt });
                run(host, fx, tx);
            }
            machine::Effect::WorktreesRefresh { .. } => {}
        }
    }
}

/// A project add Home waits for landed: its project is selected.
fn added(host: &mut dyn ProjectsHost) {
    let Adding::Waiting { attempt, server, directory } = host.home_state().adding.clone() else { return };
    let dir = machine::comparable_path(&directory);
    let Some(project) = host
        .home_state()
        .servers
        .iter()
        .find(|v| v.key == server)
        .and_then(|v| v.projects.iter().find(|p| machine::comparable_path(&p.directory) == dir))
        .map(|p| machine::Project { id: p.id.clone(), worktree: p.directory.clone() })
    else {
        return;
    };
    host.home_state_mut().adding = Adding::Idle;
    let fx = step(host, machine::Event::ProjectAdded { attempt, project: Some(project) });
    run(host, fx, &mut Tx::default());
}

/// Each poll: the servers' latest state reaches the selection whenever any Home input moved, and
/// while Home shows, its list's phase follows the focused server. True when what Home shows changed.
pub fn poll(host: &mut dyn ProjectsHost, tx: &mut Tx) -> bool {
    let landed = landing(host, tx);
    let snapshot = |h: &dyn ProjectsHost| {
        (
            selection(h),
            h.home_state()
                .servers
                .iter()
                .map(|v| (v.key.clone(), v.health.clone(), v.projects.len(), v.loaded))
                .collect::<Vec<_>>(),
            list_view(h),
        )
    };
    let before = snapshot(host);
    let closed = remote_picker::poll(host, tx);
    if host.home_state().sync_stamp != Some(sync_stamp(host)) || closed {
        sync(host);
    }
    let shown = host.is_home_route() && !host.settings_open();
    let next_target = shown.then(|| target(host));
    // Every poll comes here: off Home with the load at rest, and with no timer due, the home
    // state isn't written.
    if next_target.is_some() || !matches!(host.home_state().load, home_load::State::Idle { .. }) {
        let fx = home_load::step(&mut host.home_state_mut().load, home_load::Event::ContextChanged(next_target));
        run_load(host, fx, tx);
    }
    let now = host.clock();
    let any_due = host.home_state().timers.iter().any(|t| matches!(t, Armed::At { due, .. } if *due <= now));
    let due: Vec<home_load::Event> = if !any_due {
        Vec::new()
    } else {
        host.home_state_mut()
            .timers
            .iter_mut()
            .filter_map(|t| match t {
                Armed::At { due, event } if *due <= now => {
                    let event = event.clone();
                    *t = Armed::Idle;
                    Some(event)
                }
                _ => None,
            })
            .collect()
    };
    for event in due {
        let fx = home_load::step(&mut host.home_state_mut().load, event);
        run_load(host, fx, tx);
    }
    for t in &host.home_state().timers {
        if let Armed::At { due, .. } = t {
            tx.wake_at(*due);
        }
    }
    let after = snapshot(host);
    landed || closed || before != after
}

fn landing_step(state: &mut Landing, event: Visit) -> Option<String> {
    match event {
        Visit::Left => *state = Landing::Away,
        Visit::Arrived(tab) if *state == Landing::Away => {
            *state = Landing::Visited;
            return tab;
        }
        Visit::Arrived(_) => {}
    }
    None
}

fn landing(host: &mut dyn ProjectsHost, tx: &mut Tx) -> bool {
    let project_nav = host.project_navigation();
    if project_nav
        && !host.settings_open()
        && let Some(tab) = host.current_tab()
    {
        let h: &dyn ProjectsHost = host;
        let directory = h
            .session(&tab.id)
            .and_then(|ses| project_for_session(h, ses))
            .map_or_else(|| tab.directory.clone(), |p| p.directory.clone());
        let next = (tab.server, directory);
        if host.home_state().last_project.as_ref() != Some(&next) {
            host.home_state_mut().last_project = Some(next);
        }
    }
    let visit = if project_nav && host.is_home_route() && !host.settings_open() {
        if host.home_state().landing == Landing::Visited {
            return false;
        }
        Visit::Arrived(host.restore_project_tab_uid())
    } else {
        if host.home_state().landing == Landing::Away {
            return false;
        }
        Visit::Left
    };
    let Some(uid) = landing_step(&mut host.home_state_mut().landing, visit) else { return false };
    host.select_tab(&uid, tx);
    true
}

/// The focused server as the list sees it (`HomeSessionsTarget`).
fn target(host: &dyn ProjectsHost) -> Target {
    let key = server(host);
    let view = host.home_state().servers.iter().find(|v| v.key == key);
    let health = view.and_then(|v| v.health.clone());
    let offline = host.is_live() && host.is_offline();
    // A list kept from the last run counts as the server's cached one: it shows at once, as a
    // list already shown stays while its server is away, and the launch's own reads replace it.
    let cached = (view.is_some_and(|v| v.loaded) || host.lists_saved_sessions(&key)) && !offline;
    let connection = match health {
        Some(Health::Probe(ProbeResult::Unauthorized) | Health::Auth) => Connection::Unavailable(Unavailable::Auth),
        Some(Health::Incompatible) => Connection::Unavailable(Unavailable::Incompatible),
        _ if offline => Connection::Unavailable(Unavailable::Offline),
        Some(Health::Probe(ProbeResult::Unreachable)) if !cached => Connection::Unavailable(Unavailable::Offline),
        _ if cached => Connection::Connected,
        _ => Connection::Connecting,
    };
    Target { server: key, connection, cached, stale: false }
}

fn run_load(host: &mut dyn ProjectsHost, fx: Effects<home_load::Effect>, tx: &mut Tx) {
    for effect in fx {
        match effect {
            // The window's own launch reads the list (`servers::runtime`): once it's there the
            // fetch has its answer.
            home_load::Effect::FetchStart { attempt, .. } => {
                let reduced_motion = host.reduced_motion();
                let fx = home_load::step(&mut host.home_state_mut().load, home_load::Event::FetchSucceeded { attempt, reduced_motion });
                run_load(host, fx, tx);
            }
            home_load::Effect::FetchCancel(_) => {}
            home_load::Effect::ProbeServers => host.retry_servers(tx),
            home_load::Effect::TimerStart { timer, delay_ms, event } => {
                let due = host.clock() + delay_ms as f64 / 1000.0;
                host.home_state_mut().timers[slot(timer)] = Armed::At { due, event };
                tx.wake_at(due);
            }
            home_load::Effect::TimerStop(timer) => host.home_state_mut().timers[slot(timer)] = Armed::Idle,
        }
    }
}

fn slot(timer: Timer) -> usize {
    match timer {
        Timer::Placeholder => 0,
        Timer::Deadline => 1,
        Timer::Reveal => 2,
    }
}

/// What the session list shows (`selectHomeSessionsListView`).
pub fn list_view(host: &dyn ProjectsHost) -> home_load::ListView {
    host.home_state().load.list_view(!host.sessions().is_empty())
}

/// The list's Retry: probe the servers, and read the list again.
pub fn retry(host: &mut dyn ProjectsHost, tx: &mut Tx) {
    if server(host).is_sidecar() && matches!(host.home_state().load, home_load::State::Unavailable { reason: Unavailable::Auth, .. }) {
        host.retry_servers(tx);
        tx.instant();
        return;
    }
    let fx = home_load::step(&mut host.home_state_mut().load, home_load::Event::Retry);
    run_load(host, fx, tx);
    tx.instant();
}
