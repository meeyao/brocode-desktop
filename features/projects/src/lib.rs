//! Opening and tracking projects (`⌘O`, `/api/location`, `/api/project`), Home across servers
//! ([`home`]), session JSON import ([`import`]), and the remote directory picker dialog
//! ([`remote_picker`]).
//!
//! All state transitions, project queries, Home polling, import flows, and remote directory
//! picker operations run on non-generic `&dyn ProjectsHost` / `&mut dyn ProjectsHost`. Thin
//! `H: ProjectsHost + 'static` generic wrappers exist strictly on `rune::El` view callbacks.

#![forbid(unsafe_code)]

pub mod home;
pub mod host;
pub mod import;
pub mod jobs;
pub mod remote_picker;
pub mod sidebar;

use std::collections::{BTreeMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};

use opencode_kit::client::{DEFAULT_SERVER, Write};
use opencode_kit::model::{LocationInfo, Project, ProjectIcon, SessionInfo};
use opencode_kit::path;
use opencode_kit::servers::ServerKey;
use rune::{FileDialogOptions, Tx};
use serde_json::{Map, Value, json};

pub use host::{
    DefaultSelection, FileDialogDone, FolderDialogDone, ListedServer, ProjectsHost, TabSummary, on_server,
};
pub use jobs::{ProjectsJob, ProjectsReply, RemoteRequest, apply_reply, execute_import, execute_job, execute_remote};

use host::storage_update_host;

/// Overlay stack identifier for the remote directory picker dialog (`opencode_kernel::OverlayStack`).
pub const REMOTE_PICKER_OVERLAY_ID: &str = "dialog.remote_picker";

#[derive(Default)]
pub struct ProjectsState {
    /// Projects this client has opened or used, most recent first.
    pub used: Vec<String>,
    /// Whether Home's compact project filter popover is open.
    pub home_menu_open: bool,
    /// The narrow Home's projects dropdown's width: its popover takes it (`sameWidth`).
    pub trigger_w: f32,
    /// Import session (`import`).
    pub import: import::Import,
    /// Counter for sessions opened in fixture mode.
    opened: u32,
    /// The retired Projects sidebar's saved state (`sidebar.organize`): kept so Home's lists
    /// keep project order and older builds round-trip cleanly.
    pub organize_collapsed: bool,
    pub organize_expanded: HashSet<String>,
    pub organize_order: BTreeMap<String, Vec<String>>,
    /// The Customize sidebar's view, filters and Show more counts (`sidebar-customized.tsx`).
    pub customize: sidebar::Customize,
    /// The Recent sidebar's pins and Show more count (`recent-sessions.tsx`).
    pub recent: sidebar::Recent,
}

impl ProjectsState {
    /// Hash of the fields read by [`home::sync`] (`used`, `organize_order`, `organize_expanded`),
    /// so Home's sync stamp does not churn when per-server swaps restore identical values.
    pub fn home_sync_stamp(&self) -> u64 {
        let mut h = DefaultHasher::new();
        self.used.hash(&mut h);
        self.organize_order.hash(&mut h);
        self.organize_expanded.len().hash(&mut h);
        let mut expanded_xor = 0u64;
        for k in &self.organize_expanded {
            let mut eh = DefaultHasher::new();
            k.hash(&mut eh);
            expanded_xor ^= eh.finish();
        }
        expanded_xor.hash(&mut h);
        h.finish()
    }

    /// The organized sidebar's open projects, sorted, so saved JSON and stamps do not depend on
    /// `HashSet` iteration order.
    pub fn sorted_organize_expanded(&self) -> Vec<&String> {
        let mut ids: Vec<&String> = self.organize_expanded.iter().collect();
        ids.sort();
        ids
    }

    /// Serializes the organized sidebar's state in Desktop's `sidebar.organize` format.
    pub fn organize_json(&self) -> Value {
        let expanded: Map<String, Value> = self
            .sorted_organize_expanded()
            .into_iter()
            .map(|k| (k.clone(), Value::Bool(true)))
            .collect();
        json!({ "projectsCollapsed": self.organize_collapsed, "expanded": expanded, "order": self.organize_order })
    }

    /// Restores the organized sidebar's state from Desktop's `sidebar.organize` JSON.
    pub fn restore_organize(&mut self, v: &Value) {
        let strings = |v: &Value| {
            v.as_array()
                .into_iter()
                .flatten()
                .filter_map(|s| Some(s.as_str()?.to_owned()))
                .collect::<Vec<_>>()
        };
        // Saved before servers mixed: plain lists, of the app's own server's projects.
        let own = ServerKey::SIDECAR;
        self.organize_collapsed = v["projectsCollapsed"].as_bool().unwrap_or(false);
        self.organize_expanded = match &v["expanded"] {
            Value::Object(m) => m.iter().filter(|(_, on)| on.as_bool() == Some(true)).map(|(k, _)| k.clone()).collect(),
            list => strings(list).into_iter().map(|id| format!("{own}:{id}")).collect(),
        };
        self.organize_order = match &v["order"] {
            Value::Object(m) => m.iter().map(|(k, ids)| (k.clone(), strings(ids))).collect(),
            Value::Array(_) => std::iter::once((own.to_owned(), strings(&v["order"]))).collect(),
            _ => BTreeMap::new(),
        };
    }
}

pub fn clean_dir(raw: &str) -> String {
    path::trim(raw.trim()).to_owned()
}

pub fn folder_name(dir: &str) -> String {
    Some(path::basename(dir)).filter(|n| !n.is_empty()).unwrap_or(dir).to_owned()
}

fn synthetic_project_id(dir: &str) -> String {
    let mut h = DefaultHasher::new();
    dir.hash(&mut h);
    format!("local_{:016x}", h.finish())
}

/// Computes Desktop's `server.usedProjectIDs` scope key (`packages/app/src/runtime/server/scope.ts:25-30`).
fn used_projects_scope(host: &dyn ProjectsHost, server_base: &str) -> String {
    let active = host.active_server();
    if !active.is_sidecar() {
        return active.scope().to_string();
    }
    let own = host.own_server_base();
    let key = if own.as_deref() == Some(server_base) || server_base == DEFAULT_SERVER {
        ServerKey::sidecar()
    } else {
        ServerKey::new(server_base)
    };
    key.scope().to_string()
}

/// Reads saved used project IDs for `server_base` from `opencode.global.dat` (`server.usedProjectIDs`).
pub fn load_used_projects(host: &dyn ProjectsHost, server_base: &str) -> Option<Vec<String>> {
    let v = host.storage_read_json(home::GLOBAL, "server")?;
    let list = v["usedProjectIDs"].get(used_projects_scope(host, server_base))?.as_array()?;
    Some(list.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect())
}

/// Seeds `project_state.used` from saved state, or from open tabs and loaded sessions.
pub fn init_used_projects(host: &mut dyn ProjectsHost, saved: Option<Vec<String>>) {
    let mut seen = HashSet::new();
    let mut used = Vec::new();
    fn push(seen: &mut HashSet<String>, used: &mut Vec<String>, id: &str) {
        if !id.is_empty() && seen.insert(id.to_owned()) {
            used.push(id.to_owned());
        }
    }
    if let Some(ids) = saved {
        for id in ids {
            push(&mut seen, &mut used, &id);
        }
    } else {
        let h: &dyn ProjectsHost = host;
        let tab_projects: Vec<String> = h
            .tabs()
            .iter()
            .filter_map(|t| {
                h.session(&t.id)
                    .and_then(|s| project_for_session(h, s))
                    .or_else(|| project_at(h, &t.directory))
                    .map(|p| p.id.clone())
            })
            .collect();
        for id in tab_projects {
            push(&mut seen, &mut used, &id);
        }
        let session_projects: Vec<String> = h
            .sessions()
            .iter()
            .filter_map(|s| project_for_session(h, s).map(|p| p.id.clone()))
            .collect();
        for id in session_projects {
            push(&mut seen, &mut used, &id);
        }
        if used.is_empty() {
            let all: Vec<String> = h.projects().iter().map(|p| p.id.clone()).collect();
            for id in all {
                push(&mut seen, &mut used, &id);
            }
        }
    }
    host.project_state_mut().used = used;
}

/// Marks project `id` as used most recently by this client and saves the list in live mode.
pub fn touch_project(host: &mut dyn ProjectsHost, id: &str) {
    if id.is_empty() {
        return;
    }
    let now = host.now_ms();
    if let Some(p) = host.projects_mut().iter_mut().find(|p| p.id == id) {
        p.active = p.active.max(now);
    }
    let used = &mut host.project_state_mut().used;
    used.retain(|x| x != id);
    used.insert(0, id.to_owned());
    save_used_projects(host);
}

/// Persists `project_state.used` to `opencode.global.dat` (`server.usedProjectIDs`) when live and
/// persistence is enabled.
pub fn save_used_projects(host: &mut dyn ProjectsHost) {
    let Some(base) = host.live_server().map(|s| s.base.clone()) else { return };
    if !host.persists() {
        return;
    }
    let scope = used_projects_scope(host, &base);
    let ids = host.project_state().used.clone();
    storage_update_host(host, home::GLOBAL, "server", |m| {
        let used = m.entry("usedProjectIDs").or_insert_with(|| Value::Object(Map::new()));
        if let Some(used) = used.as_object_mut() {
            used.insert(scope, json!(ids));
        }
    });
}

/// Projects this client has used, in the server's order (most recently active first), one per
/// directory: using any project of a folder lists that folder's current one
/// (`projects.used()` in `runtime/server/runtime.tsx`).
pub fn used_projects(host: &dyn ProjectsHost) -> Vec<&Project> {
    let projects = host.projects();
    let used_dirs: HashSet<String> = host
        .project_state()
        .used
        .iter()
        .filter_map(|id| projects.iter().find(|p| &p.id == id))
        .map(|p| opencode_servers::home::same_directory_key(&p.directory))
        .collect();
    let mut by_activity: Vec<&Project> = projects.iter().collect();
    by_activity.sort_by_key(|p| std::cmp::Reverse(p.active));
    let mut seen_dirs = HashSet::new();
    by_activity
        .into_iter()
        .filter(|p| {
            let dir = opencode_servers::home::same_directory_key(&p.directory);
            used_dirs.contains(&dir) && seen_dirs.insert(dir)
        })
        .collect()
}

/// The project this client used last (`projects.last()`, the `lastProjectID` each touch sets),
/// whatever the server's activity order says since.
pub fn last_used_project(host: &dyn ProjectsHost) -> Option<&Project> {
    let projects = host.projects();
    host.project_state().used.iter().find_map(|id| projects.iter().find(|p| &p.id == id))
}

/// The used projects: those in the retired Projects sidebar's saved order first, then the
/// others, most recently active first.
pub fn organized_projects(host: &dyn ProjectsHost) -> Vec<&Project> {
    let used = used_projects(host);
    let active = host.active_server();
    let order = host.project_state().organize_order.get(active.as_str()).map_or(&[][..], Vec::as_slice);
    let mut list: Vec<&Project> = order.iter().filter_map(|id| used.iter().find(|p| &p.id == id).copied()).collect();
    list.extend(used.iter().filter(|p| !order.contains(&p.id)).copied());
    list
}

/// Server projects this client hasn't added or has hidden from its used list.
pub fn other_projects(host: &dyn ProjectsHost) -> Vec<&Project> {
    let used_dirs: HashSet<String> = used_projects(host).into_iter().map(|p| opencode_servers::home::same_directory_key(&p.directory)).collect();
    let mut seen = HashSet::new();
    host.projects()
        .iter()
        .filter(|p| {
            let d = opencode_servers::home::same_directory_key(&p.directory);
            !used_dirs.contains(&d) && seen.insert(d)
        })
        .collect()
}

/// Removes project `id` from this client's used list (`HomeProjects` Close); the server keeps it.
pub fn hide_project(host: &mut dyn ProjectsHost, id: &str, tx: &mut Tx) {
    tx.instant();
    let dir = host.project(id).map(|p| opencode_servers::home::same_directory_key(&p.directory));
    let drop_ids: HashSet<String> = host
        .projects()
        .iter()
        .filter(|p| p.id == id || dir.as_deref().is_some_and(|d| opencode_servers::home::same_directory_key(&p.directory) == d))
        .map(|p| p.id.clone())
        .collect();
    {
        let st = host.project_state_mut();
        st.used.retain(|x| !drop_ids.contains(x) && x != id);
        st.home_menu_open = false;
    }
    save_used_projects(host);
}

/// A Home project row: selects project `id` of the server the window works against, or, when
/// it's selected, goes back to all of the server's projects (`project.toggled`).
pub fn toggle_home_project(host: &mut dyn ProjectsHost, id: &str, tx: &mut Tx) {
    tx.instant();
    let server = host.active_server();
    if let Some(dir) = host.project(id).map(|p| p.directory.clone()) {
        home::toggle(host, &server, &dir);
    }
    host.project_state_mut().home_menu_open = false;
}

/// All projects: Home focuses the server it shows (`server.focused`).
pub fn clear_home_project(host: &mut dyn ProjectsHost, tx: &mut Tx) {
    tx.instant();
    let server = home::server(host);
    home::focus(host, &server);
    host.project_state_mut().home_menu_open = false;
}

/// Resolves the project that owns `dir`: exact root or worktree sandbox first, then longest
/// containing project root.
pub fn project_at<'a>(host: &'a dyn ProjectsHost, dir: &str) -> Option<&'a Project> {
    project_exactly_at(host, dir, "").or_else(|| project_containing(host, dir))
}

/// Resolves a session's project (`projectForSession` in Desktop): a project rooted exactly at
/// its directory first (so `"global"` folders and moved repos whose sessions keep an old
/// project ID resolve as the server does now), then `session.project`, and only then a
/// project whose root merely contains the directory, so a parent folder like `~/code` that
/// was once opened never claims every repo under it.
pub fn project_for_session<'a>(host: &'a dyn ProjectsHost, s: &SessionInfo) -> Option<&'a Project> {
    let by_id = || (!s.project.is_empty() && s.project != "global").then(|| host.project(&s.project)).flatten();
    project_exactly_at(host, &s.directory, &s.project)
        .or_else(by_id)
        .or_else(|| project_containing(host, &s.directory))
}

/// The project rooted at `dir` or with it as a sandbox, preferring `id` when several are.
fn project_exactly_at<'a>(host: &'a dyn ProjectsHost, dir: &str, id: &str) -> Option<&'a Project> {
    let dir = path::trim(dir.trim());
    if dir.is_empty() {
        return None;
    }
    // Its checkout, or one of its worktrees: a sandbox, or one the server's inventory lists.
    let at = |p: &&Project| path::same(path::trim(&p.directory), dir) || host.has_worktree_at(p, dir);
    host.project(id).filter(at).or_else(|| host.projects().iter().find(at))
}

fn project_containing<'a>(host: &'a dyn ProjectsHost, dir: &str) -> Option<&'a Project> {
    let dir = clean_dir(dir);
    host.projects().iter().filter(|p| path::contains(&p.directory, &dir)).max_by_key(|p| p.directory.len())
}

/// Opens the native folder chooser (`⌘O`, "Open project") and opens the chosen folder.
pub fn open_project_dialog(host: &mut dyn ProjectsHost, tx: &mut Tx) {
    tx.instant();
    let restore_composer = host.before_open_project_dialog();
    host.project_state_mut().home_menu_open = false;
    let options = FileDialogOptions { title: opencode_i18n::t!("command.project.open").into(), ..Default::default() };
    host.open_folder_dialog(
        options,
        tx,
        Box::new(move |h, result, tx| match result {
            Ok(Some(path)) => open_project_dir(h, &path.to_string_lossy(), true, tx),
            Ok(None) => {
                if restore_composer {
                    h.restore_composer_focus();
                }
            }
            Err(e) => h.toast_fail(opencode_i18n::t!("rune.status.folderPickerFailed"), &e),
        }),
    );
}

/// Adds `raw_dir` as a project and, when `start_session` is true, opens a new session in it.
pub fn open_project_dir(host: &mut dyn ProjectsHost, raw_dir: &str, start_session: bool, tx: &mut Tx) {
    let dir = clean_dir(raw_dir);
    if dir.is_empty() {
        return;
    }
    host.set_draft_dir(dir.clone());
    if host.is_live() && !host.is_stage() {
        if host.write_server(Write::OpenProject { directory: dir, start_session }) {
            host.set_status("Opening project…".into());
        }
        return;
    }
    tx.instant();
    let id = ensure_local_project(host, &dir, None);
    touch_project(host, &id);
    host.sort_projects();
    if start_session {
        let ses = make_fixture_session(host, &id, &dir);
        let sid = ses.id.clone();
        host.ensure_transcript(&sid);
        if let Some(sessions) = host.sessions_mut() {
            sessions.insert(0, ses);
        }
        host.sort_projects();
        host.open_session_now(&sid, tx);
    }
}

/// Applies `Incoming::ProjectOpened` from the server write worker.
pub fn apply_project_opened(
    host: &mut dyn ProjectsHost,
    location: LocationInfo,
    projects: Vec<Project>,
    session: Option<SessionInfo>,
    tx: &mut Tx,
) {
    tx.instant();
    *host.projects_mut() = projects;
    let dir = clean_dir(if location.directory.is_empty() { &location.project_directory } else { &location.directory });
    let root = clean_dir(if location.canonical.is_empty() { &dir } else { &location.canonical });
    let project_id = if !location.project_id.is_empty()
        && location.project_id != "global"
        && host.projects().iter().any(|p| p.id == location.project_id)
    {
        location.project_id.clone()
    } else {
        ensure_local_project(host, if root == "/" { &dir } else { &root }, Some(&location.project_id))
    };
    if !dir.is_empty() {
        host.set_draft_dir(dir.clone());
    }
    touch_project(host, &project_id);
    let server_label = host
        .live_server()
        .map(|s| s.base.trim_start_matches("http://").to_owned())
        .unwrap_or_else(|| DEFAULT_SERVER.trim_start_matches("http://").to_owned());
    host.set_status(format!("Live · {server_label}"));
    if let Some(mut info) = session {
        if info.project.is_empty() || info.project == "global" {
            info.project.clone_from(&project_id);
        }
        if info.directory.is_empty() {
            info.directory = dir;
        }
        let sid = info.id.clone();
        host.ensure_transcript(&sid);
        if let Some(sessions) = host.sessions_mut() {
            match sessions.iter_mut().find(|s| s.id == sid) {
                Some(old) => *old = info,
                None => sessions.insert(0, info),
            }
        }
        host.sort_projects();
        host.open_session_now(&sid, tx);
    } else {
        host.sort_projects();
    }
}

/// Refreshes `/api/project` on a live server.
pub fn refresh_projects(host: &dyn ProjectsHost) {
    host.request_projects();
}

fn ensure_local_project(host: &mut dyn ProjectsHost, dir: &str, preferred_id: Option<&str>) -> String {
    if let Some(existing) = project_at(host, dir) {
        return existing.id.clone();
    }
    let id = preferred_id
        .filter(|id| !id.is_empty() && *id != "global")
        .map(str::to_owned)
        .unwrap_or_else(|| synthetic_project_id(dir));
    let name = folder_name(dir);
    let active = host.now_ms();
    host.projects_mut().push(Project {
        id: id.clone(),
        name,
        directory: dir.to_owned(),
        active,
        icon: ProjectIcon::default(),
    });
    id
}

fn make_fixture_session(host: &mut dyn ProjectsHost, project_id: &str, directory: &str) -> SessionInfo {
    let opened = {
        let st = host.project_state_mut();
        st.opened += 1;
        st.opened
    };
    let selection = host.default_selection();
    let now = host.now_ms();
    SessionInfo {
        id: format!("ses_fxopen{opened:04}aaaaaaaaaaaaaa"),
        parent: None,
        project: project_id.to_owned(),
        title: String::new(),
        directory: directory.to_owned(),
        updated: now,
        model: selection.model,
        cost: 0.0,
        idle: None,
        viewed: Some(now),
        agent: selection.agent,
        created: now,
        outcome: None,
        revert: None,
        ..Default::default()
    }
}
