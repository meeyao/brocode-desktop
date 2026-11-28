//! Pinned tabs, from every server the app keeps live, in one list per window, as OpenCode
//! Desktop keeps them (`shell/tabs/schema.ts`): each tab carries its server, so a window mixes
//! sessions of the app's own server, servers added by address and plugins' servers. The list,
//! the saved titles and the closed tabs live in the window's own rows (`windows::store`,
//! `opencode.window.<id>.dat`'s `tabs`, `tabs.info` and `tabs.closed`), never on a server.
//!
//! A fresh profile's first window reads Desktop's list after its first frame (read-only, in
//! process) so the two show the same tabs; without Desktop it starts with none.

pub mod actions;
pub mod auto_groups;
pub mod badges;
pub mod groups;
pub mod hints;
pub mod navigation_switch;
pub mod pins;
pub mod recently_closed;
pub mod sections;

use opencode_servers::tabs as doc;
use opencode_kit::servers::{ServerKey, TabKey};

use crate::Route;

/// What a tab holds: a session, or Desktop's "New session" draft (`TabStorage.Draft`), which
/// becomes a session tab in its place when its first prompt goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TabKind {
    Session,
    Draft,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Tab {
    /// A session's id, or a draft's own `draft_…` id.
    pub id: String,
    pub kind: TabKind,
    /// The server its session is on, or where a draft's session will start.
    pub server: ServerKey,
    /// The title and directory when the tab was saved, for a session outside the loaded list.
    /// A draft's directory is where its session will start; its title is unused.
    pub title: String,
    pub directory: String,
}

impl Tab {
    /// A tab for session `id` on the server the window works against now.
    pub fn session(id: impl Into<String>, title: impl Into<String>, directory: impl Into<String>) -> Tab {
        Tab { id: id.into(), kind: TabKind::Session, server: crate::servers::runtime::current(), title: title.into(), directory: directory.into() }
    }

    /// A draft tab whose session will start on the server the window works against now.
    pub fn draft(id: impl Into<String>, directory: impl Into<String>) -> Tab {
        Tab { id: id.into(), kind: TabKind::Draft, server: crate::servers::runtime::current(), title: String::new(), directory: directory.into() }
    }

    /// The tab on `server` instead.
    pub fn on(mut self, server: ServerKey) -> Tab {
        self.server = server;
        self
    }

    pub fn is_draft(&self) -> bool {
        self.kind == TabKind::Draft
    }

    /// Whether it is this session's tab on `server`, or this draft's.
    pub fn is(&self, server: &ServerKey, id: &str) -> bool {
        self.id == id && (self.is_draft() || self.server == *server)
    }

    /// The page this tab shows.
    pub fn route(&self) -> Route {
        match self.kind {
            TabKind::Session => Route::session(self.server.clone(), self.id.clone()),
            TabKind::Draft => Route::Draft(self.id.clone()),
        }
    }

    /// What the strips, menus and keys call it: unique among a window's tabs, as Desktop's
    /// `tabKey` is. A draft's id, a session's id on the app's own server, and on another server the
    /// id marked with its server, so a session two keys list (one server added twice) is two tabs.
    pub fn uid(&self) -> String {
        match self.kind {
            TabKind::Session => session_uid(&self.server, &self.id),
            TabKind::Draft => self.id.clone(),
        }
    }

    /// Its identity in the window's file (`tabKey`).
    pub fn key(&self) -> TabKey {
        match self.kind {
            TabKind::Session => TabKey::session(self.server.clone(), self.id.clone()),
            TabKind::Draft => TabKey::draft(self.id.clone()),
        }
    }

    /// Pure-data descriptor shared with feature crates (`projects`, `session-menu`).
    pub fn summary(&self) -> opencode_runtime::TabSummary {
        opencode_runtime::TabSummary {
            uid: self.uid(),
            id: self.id.clone(),
            directory: self.directory.clone(),
            server: self.server.clone(),
            is_draft: self.is_draft(),
        }
    }

    /// The tab as the window's file keeps it, with what `tabs.info` saves of it.
    pub fn to_doc(&self) -> (doc::Tab, doc::TabInfo) {
        let text = |s: &str| (!s.is_empty()).then(|| s.to_owned());
        match self.kind {
            TabKind::Session => (doc::Tab::Session(doc::SessionTab::new(self.server.clone(), &self.id)), doc::TabInfo { title: text(&self.title), directory: text(&self.directory), prompted: None }),
            TabKind::Draft => (doc::Tab::Draft(doc::DraftTab::new(&self.id, self.server.clone(), &self.directory)), doc::TabInfo::default()),
        }
    }

    pub fn from_doc(tab: &doc::Tab, info: Option<&doc::TabInfo>) -> Tab {
        let field = |f: fn(&doc::TabInfo) -> &Option<String>| info.and_then(|i| f(i).clone()).unwrap_or_default();
        match tab {
            doc::Tab::Session(t) => Tab { id: t.session_id.clone(), kind: TabKind::Session, server: t.server.clone(), title: field(|i| &i.title), directory: field(|i| &i.directory) },
            doc::Tab::Draft(d) => Tab { id: d.draft_id.clone(), kind: TabKind::Draft, server: d.server.clone(), title: String::new(), directory: d.directory.clone() },
        }
    }
}

/// Session `id` on `server` as tabs and inbox rows name it (`Tab::uid`).
pub fn session_uid(server: &ServerKey, id: &str) -> String {
    if server.is_sidecar() { id.to_owned() } else { format!("{id}~{:08x}", fnv(server.as_str())) }
}

/// FNV-1a: the same mark for a server on every run (element keys stay put across launches).
fn fnv(text: &str) -> u32 {
    text.bytes().fold(0x811c_9dc5, |h, b| (h ^ u32::from(b)).wrapping_mul(0x0100_0193))
}

/// A tab's label: drafts, sessions still starting and untitled sessions all read "Session", as
/// a missing or timestamped fallback title does (`sessionTabTitle`).
pub fn tab_title(title: &str) -> String {
    let fallback = ["New session - ", "Child session - "].iter().any(|p| title.strip_prefix(p).is_some_and(is_iso_stamp));
    if title.is_empty() || fallback { opencode_i18n::t!("session.tab.session").into() } else { title.to_owned() }
}

/// `2026-07-11T09:20:00.000Z`, as JavaScript's `toISOString` writes it.
fn is_iso_stamp(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 24
        && b.iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            10 => *c == b'T',
            13 | 16 => *c == b':',
            19 => *c == b'.',
            23 => *c == b'Z',
            _ => c.is_ascii_digit(),
        })
}

/// A fresh draft id, unique as a session id is (Desktop uses a UUID).
pub fn draft_id() -> String {
    let id = opencode_kit::client::session_id();
    format!("draft_{}", id.strip_prefix("ses_").unwrap_or(&id))
}

/// How many closed tabs Desktop keeps for reopening (`CLOSED_TAB_LIMIT`).
pub const CLOSED_LIMIT: usize = 100;

/// A closed session tab and the place it had (`TabStorage.ClosedTab`). `prompted`: its session
/// had a prompt, so "Recently closed" lists it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ClosedTab {
    pub tab: Tab,
    pub index: usize,
    pub prompted: bool,
}

/// Closed tabs, oldest first, as Desktop's `tabs.closed`: a session appears once, at its latest
/// close, keyed by its server and id (`shell/tabs/closed.ts`). Beyond Desktop, which disposes a
/// closed draft with its words, a draft tab closed with something in it is kept too, with when it
/// closed (`DRAFT_DAYS`), and ⇧⌘T reopens it whole; `Recently closed` lists sessions only.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Closed(Vec<ClosedTab>, Vec<(String, i64)>);

/// How long a closed draft stays reopenable.
pub const DRAFT_DAYS: i64 = 7;

impl Closed {
    pub fn push(&mut self, entry: ClosedTab) {
        self.0.retain(|e| !e.tab.is(&entry.tab.server, &entry.tab.id));
        self.0.push(entry);
        let over = self.0.len().saturating_sub(CLOSED_LIMIT);
        self.0.drain(..over);
        let kept: Vec<&str> = self.0.iter().filter(|e| e.tab.is_draft()).map(|e| e.tab.id.as_str()).collect();
        self.1.retain(|(id, _)| kept.contains(&id.as_str()));
    }

    /// A draft tab closed at `at_ms` with something in it: reopenable.
    pub fn push_draft(&mut self, entry: ClosedTab, at_ms: i64) {
        let id = entry.tab.id.clone();
        self.push(entry);
        self.1.retain(|(d, _)| *d != id);
        self.1.push((id, at_ms));
    }

    /// Every entry, oldest first, drafts included.
    pub fn entries(&self) -> impl Iterator<Item = &ClosedTab> + '_ {
        self.0.iter()
    }

    /// The closed drafts as the window keeps them (`rune.closedDrafts`): id, folder, place, when.
    pub fn drafts_json(&self) -> serde_json::Value {
        self.1.iter().filter_map(|(id, at)| {
            let e = self.0.iter().find(|e| e.tab.is_draft() && e.tab.id == *id)?;
            Some(serde_json::json!({ "draftID": id, "directory": e.tab.directory, "index": e.index, "closedAt": at }))
        }).collect()
    }

    /// Takes back the closed drafts kept as `drafts_json` writes them, dropping those closed more
    /// than `DRAFT_DAYS` before `now_ms`; the ids of those dropped, whose words go.
    pub fn take_drafts_json(&mut self, v: &serde_json::Value, now_ms: i64) -> Vec<String> {
        let mut expired = Vec::new();
        for d in v.as_array().into_iter().flatten() {
            let (Some(id), Some(at)) = (d["draftID"].as_str(), d["closedAt"].as_i64()) else { continue };
            if now_ms - at > DRAFT_DAYS * 24 * 3600 * 1000 {
                expired.push(id.to_owned());
                continue;
            }
            let tab = Tab::draft(id, d["directory"].as_str().unwrap_or_default());
            let entry = ClosedTab { tab, index: d["index"].as_u64().unwrap_or(0) as usize, prompted: false };
            self.push_draft(entry, at);
        }
        expired
    }

    /// Takes the newest closed tab that isn't open again (or `target`'s), dropping stale ones
    /// passed on the way (`takeClosedTab`).
    pub fn take(&mut self, open: &[Tab], target: Option<&str>) -> Option<ClosedTab> {
        let taken = self.take_entry(open, target)?;
        self.1.retain(|(id, _)| *id != taken.tab.id);
        Some(taken)
    }

    fn take_entry(&mut self, open: &[Tab], target: Option<&str>) -> Option<ClosedTab> {
        let is_open = |e: &ClosedTab| open.iter().any(|t| t.kind == e.tab.kind && t.is(&e.tab.server, &e.tab.id));
        if let Some(id) = target {
            let at = self.0.iter().rposition(|e| e.tab.id == id)?;
            return (!is_open(&self.0[at])).then(|| self.0.remove(at));
        }
        while let Some(e) = self.0.pop() {
            if !is_open(&e) {
                return Some(e);
            }
        }
        None
    }

    /// Newest first, without tabs open again (`listClosedTabs`).
    pub fn list<'a>(&'a self, open: &'a [Tab]) -> impl Iterator<Item = &'a ClosedTab> + 'a {
        self.0.iter().rev().filter(|e| !e.tab.is_draft()).filter(|e| !open.iter().any(|t| t.kind == TabKind::Session && t.is(&e.tab.server, &e.tab.id)))
    }

    /// Sessions of the server the window works against now that are gone for good leave the
    /// list (`removeClosedTabs`).
    pub fn remove(&mut self, gone: impl Fn(&str) -> bool) {
        let server = crate::servers::runtime::current();
        self.0.retain(|e| e.tab.server != server || !gone(&e.tab.id));
    }

    /// `tabs.closed`, as the window's file keeps it.
    pub fn to_doc(&self) -> Vec<doc::ClosedTab> {
        self.0
            .iter()
            .filter(|e| !e.tab.is_draft())
            .map(|e| {
                let (_, mut info) = e.tab.to_doc();
                info.prompted = e.prompted.then_some(true);
                doc::ClosedTab { tab: doc::SessionTab::new(e.tab.server.clone(), &e.tab.id), index: e.index as u64, info: (!info.is_empty()).then_some(info) }
            })
            .collect()
    }

    pub fn from_doc(list: &[doc::ClosedTab]) -> Closed {
        let mut closed = Closed::default();
        for c in list {
            let tab = Tab::from_doc(&doc::Tab::Session(c.tab.clone()), c.info.as_ref());
            closed.push(ClosedTab { tab, index: c.index as usize, prompted: c.info.as_ref().and_then(|i| i.prompted).unwrap_or(false) });
        }
        closed
    }
}

/// The app-wide keys (Desktop's `opencode.global.dat`).
const GLOBAL: &str = "opencode.global.dat";

/// The saved UI zoom (1.0 is 100%), Desktop's global `zoom`.
pub fn load_zoom() -> Option<f32> {
    crate::storage::json(GLOBAL, "zoom")?.as_f64().map(|z| z as f32)
}

pub fn save_zoom(zoom: f32) {
    crate::storage::set_json(GLOBAL, "zoom", &serde_json::json!(f64::from((zoom * 100.0).round()) / 100.0));
}

/// Project IDs this client has used on `server` (an address), most recent first: Desktop's
/// `server.usedProjectIDs`, by the server's scope.
pub fn load_projects(server: &str) -> Option<Vec<String>> {
    let v = crate::storage::json(GLOBAL, "server")?;
    let list = v["usedProjectIDs"].get(scope(server))?.as_array()?;
    Some(list.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect())
}

/// Project IDs used on `key` (`ServerScope.fromServerKey`, `runtime/server/scope.ts:25-30`),
/// falling back to `server_base` for legacy address-keyed entries.
pub fn load_projects_for_key(key: &ServerKey, server_base: &str) -> Option<Vec<String>> {
    let v = crate::storage::json(GLOBAL, "server")?;
    let primary = key.scope();
    let list = v["usedProjectIDs"]
        .get(primary.as_str())
        .or_else(|| {
            if key.is_sidecar() {
                return None;
            }
            // Legacy address-keyed fallback for non-sidecar servers: never read sidecar's "local".
            let fallback = ServerKey::new(server_base).scope();
            (fallback.as_str() != opencode_kit::servers::ServerScope::LOCAL && fallback != primary)
                .then(|| v["usedProjectIDs"].get(fallback.as_str()))
                .flatten()
        })?
        .as_array()?;
    Some(list.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect())
}

/// The scope Desktop keys a server's projects by (`ServerKey::scope`): the app's own server's
/// is `local`.
fn scope(server: &str) -> String {
    let key = if crate::servers::runtime::own_server().is_some_and(|s| s.base == server) || server == opencode_kit::client::DEFAULT_SERVER { ServerKey::sidecar() } else { ServerKey::new(server) };
    key.scope().to_string()
}

/// Desktop's tabs for its own sidecar server (the local `:4096` service) from the window it
/// wrote last, read once into a fresh profile's first window after the first frame. Read-only
/// and in process (`opencode_storage::native::seed`): the newest of Desktop's databases (its
/// production, Beta and Dev builds).
pub fn from_beta() -> Option<Vec<Tab>> {
    let (tabs, info) = opencode_storage::desktop_recent_tabs(&crate::dirs::official())?;
    // Desktop's rows are already in the mixed-server shape; its own sidecar's tabs are this app's.
    let beta = opencode_servers::migration::window::from_beta(&tabs, &info);
    let list = beta.tabs.iter().flatten().filter(|t| matches!(t, doc::Tab::Session(s) if s.server.is_sidecar())).map(|t| Tab::from_doc(t, beta.info(&t.key()))).collect();
    Some(list)
}
