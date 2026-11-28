//! Session search from Home (`home/sessions/search.ts`, `home/sessions/view.tsx`):
//! - ⌘F on Home (and ⌘B opening Home from a session) focuses Home's session search.
//! - Typing filters Home's sessions by title and project name, and looks up a pasted `ses_…` id
//!   on the focused server (`GET /api/session/:id`), which heads the results while it's found.
//! - ↑ / ↓ step through matching rows, ↩ opens the highlighted session, Esc clears and blurs; a
//!   press outside the search clears it too, while Tab only moves focus on.

use std::collections::HashMap;

use opencode_kit::client::Server;
use opencode_kit::model::{self, Project, SessionInfo};
use opencode_search::query::{Highlight, cycle};
use rune::machine::{Attempt, Effects, Seq};
use rune::{Key, KeyEvent, Latest, TextEdit, Tx, Waker};

/// Home lists at most this many sessions (`HOME_SESSION_LIMIT`, `home/sessions/index.ts`).
pub const HOME_SESSION_LIMIT: usize = 64;

/// `HOME_SESSION_LOOKUP_TIMEOUT_MS`.
const LOOKUP_TIMEOUT: f64 = 10.0;

/// Whether a session title belongs to an automation folded away from Home's main list (`flow: …`).
pub fn is_automation(title: &str) -> bool {
    title.starts_with("flow:")
}

/// The list's order against the pointer (Kit's design; Desktop's list reorders under it).
#[derive(Default)]
pub enum HomeHold {
    /// The rows sit in their own order.
    #[default]
    Free,
    /// The pointer is on the list: `at` is each row's place and day as it came on.
    Holding { at: HashMap<String, (usize, usize)> },
    /// The pointer has left: the next poll lets the order go.
    Leaving { at: HashMap<String, (usize, usize)> },
}

impl HomeHold {
    /// Each row's held place and day, while the order holds.
    pub fn at(&self) -> Option<&HashMap<String, (usize, usize)>> {
        match self {
            HomeHold::Free => None,
            HomeHold::Holding { at } | HomeHold::Leaving { at } => Some(at),
        }
    }
}

/// Object-safe host boundary for Home's session search state machine, lookup worker, and key navigation.
pub trait HomeSearchHost: opencode_kernel::AppEnv {
    fn home_search(&self) -> &HomeSearch;
    fn home_search_mut(&mut self) -> &mut HomeSearch;

    fn search_input(&self) -> &TextEdit;
    fn search_input_mut(&mut self) -> &mut TextEdit;

    fn is_home(&self) -> bool;
    fn settings_open(&self) -> bool {
        false
    }
    /// Whether Home search filters the list in place rather than opening a results panel (#64668).
    fn search_inline(&self) -> bool {
        false
    }
    fn live_server(&self) -> Option<Server>;

    /// Current Home search scope key (`"<server>\0<directory>"`) and whether the focused server is
    /// live and healthy.
    fn home_search_scope(&self) -> (String, bool);
    /// Currently selected Home project filter, if any.
    fn home_selected(&self) -> Option<&Project>;

    fn session(&self, id: &str) -> Option<&SessionInfo>;
    fn push_session(&mut self, session: SessionInfo);
    fn search_session(&self, id: &str) -> Option<&SessionInfo>;
    fn project_for_session(&self, session: &SessionInfo) -> Option<&Project>;
    /// Whether `session` belongs to `project` (`packages/app/src/home/sessions/records.ts:24-58`).
    fn session_in_project(&self, session: &SessionInfo, project: &Project) -> bool {
        self.project_for_session(session).is_some_and(|p| p.id == project.id)
    }
    /// Checkout and known worktree/sandbox directories for `project` (`projectSessionDirectories`).
    fn project_directories(&self, project: &Project) -> Vec<String> {
        vec![project.directory.clone()]
    }
    fn merged_roots(&self) -> Vec<&SessionInfo>;
    fn merged_roots_with<'a>(&'a self, extra: &'a [SessionInfo]) -> Vec<&'a SessionInfo> {
        let _ = extra;
        self.merged_roots()
    }

    fn find_home_sessions(&self, project_id: Option<&str>, query: &str, limit: usize) -> Vec<(i32, i64, Box<str>)>;
    fn session_search_building(&self) -> bool {
        false
    }
    fn fulltext_home_rows(&self) -> &[crate::fulltext::Row] {
        &[]
    }

    /// Closes conflicting pickers/popovers before focusing Home's search field.
    fn before_focus_home_search(&mut self) {}
    fn prefetch_session(&self, _id: &str) {}
    fn open_session(&mut self, id: &str, tx: &mut Tx);
    fn open_find_at(&mut self, query: &str, message: &str, tx: &mut Tx);
    /// Fallback key handler when Home's search input did not consume `ev` (`switcher_key || shortcut`).
    fn handle_fallback_key(&mut self, _ev: &KeyEvent, _tx: &mut Tx) -> bool {
        false
    }
}

/// Home's session search: Desktop's `HomeSessionSearchState` (`home/sessions/search-machine.ts:
/// 6-24`). Its value is the field's text (`App::search`), its highlight SM-7's [`Highlight`].
#[derive(Clone, Debug, Default)]
pub struct Search {
    /// The server and project the results belong to: an exact match from another is never shown.
    scope: String,
    connected: bool,
    /// The field, or anything else in the search, has focus.
    focused: bool,
    /// The trimmed text the status was settled for.
    query: String,
    seq: Seq,
    pub status: Status,
}

#[derive(Clone, Debug, Default)]
pub enum Status {
    #[default]
    Idle,
    /// A typed session id is being looked up on the focused server, until `due` on the app clock.
    LookingUp { attempt: Attempt, id: String, due: f64 },
    /// The lookup found it: it heads the results until the text, server or project changes.
    Found(Box<SessionInfo>),
}

#[derive(Clone, Debug)]
pub enum Event<'a> {
    /// `focused`.
    Focused,
    /// `blurred`: focus left the search by the keys.
    Blurred,
    /// `closed`: Escape, a press outside it, or a result opening.
    Closed,
    /// `input.changed` and `scope.changed`: what the field holds, and where it searches.
    Synced { query: &'a str, scope: &'a str, connected: bool, now: f64 },
    /// `lookup.resolved` (and `lookup.failed`, as `None`).
    Resolved { attempt: Attempt, session: Option<Box<SessionInfo>> },
    /// `lookup.timed-out` once the clock reaches `now`.
    Tick { now: f64 },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// `lookup.start`.
    Lookup { attempt: Attempt, id: String },
    /// `lookup.abort`.
    Abort,
    /// The field empties and nothing is highlighted.
    Clear,
    /// The highlight goes back to the first result.
    Unhighlight,
}

impl Search {
    /// `transitionHomeSessionSearch`.
    pub fn step(&mut self, event: Event<'_>) -> Effects<Effect> {
        match event {
            Event::Focused => {
                self.focused = true;
                Effects::none()
            }
            Event::Blurred => {
                self.focused = false;
                Effects::none()
            }
            Event::Closed => {
                self.focused = false;
                self.query.clear();
                let mut effects = self.settle(0.0);
                effects.push(Effect::Clear);
                effects
            }
            Event::Synced { query, scope, connected, now } => {
                if query == self.query && scope == self.scope && connected == self.connected {
                    return Effects::none();
                }
                let rescoped = scope != self.scope;
                let retyped = query != self.query;
                let reconnected = connected && !self.connected;
                (self.query, self.scope, self.connected) = (query.to_owned(), scope.to_owned(), connected);
                if retyped || rescoped || (reconnected && matches!(self.status, Status::Idle)) {
                    let mut effects = self.settle(now);
                    if retyped || rescoped {
                        effects.push(Effect::Unhighlight);
                    }
                    return effects;
                }
                if !connected && matches!(self.status, Status::LookingUp { .. }) {
                    self.status = Status::Idle;
                    return Effects::one(Effect::Abort);
                }
                Effects::none()
            }
            Event::Resolved { attempt, session } => {
                if !matches!(&self.status, Status::LookingUp { attempt: a, .. } if *a == attempt) {
                    return Effects::none();
                }
                self.status = session.map_or(Status::Idle, Status::Found);
                Effects::none()
            }
            Event::Tick { now } => {
                if !matches!(&self.status, Status::LookingUp { due, .. } if now + 1e-9 >= *due) {
                    return Effects::none();
                }
                self.status = Status::Idle;
                Effects::one(Effect::Abort)
            }
        }
    }

    /// `settle`: a pending lookup goes, and a query that looks like a session id is looked up
    /// while the server is connected.
    fn settle(&mut self, now: f64) -> Effects<Effect> {
        let mut effects = Effects::none();
        if matches!(self.status, Status::LookingUp { .. }) {
            effects.push(Effect::Abort);
        }
        if !looks_like_session_id(&self.query) || !self.connected {
            self.status = Status::Idle;
            return effects;
        }
        let attempt = self.seq.next();
        self.status = Status::LookingUp { attempt, id: self.query.clone(), due: now + LOOKUP_TIMEOUT };
        effects.push(Effect::Lookup { attempt, id: self.query.clone() });
        effects
    }

    /// `selectSearchOpen`.
    pub fn open(&self) -> bool {
        self.focused && !self.query.is_empty()
    }

    /// `selectSearchExact`.
    pub fn exact(&self) -> Option<&SessionInfo> {
        match &self.status {
            Status::Found(session) => Some(session.as_ref()),
            _ => None,
        }
    }

    pub fn focused(&self) -> bool {
        self.focused
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    /// What tells one status from another.
    fn mark(&self) -> (u8, Option<Attempt>, Option<String>) {
        match &self.status {
            Status::Idle => (0, None, None),
            Status::LookingUp { attempt, .. } => (1, Some(*attempt), None),
            Status::Found(session) => (2, None, Some(session.id.clone())),
        }
    }

    /// When a pending lookup times out.
    fn due(&self) -> Option<f64> {
        match self.status {
            Status::LookingUp { due, .. } => Some(due),
            _ => None,
        }
    }
}

pub struct HomeSearch {
    pub search: Search,
    active: Highlight,
    /// The Home row under the pointer: it shows its worktree and branch (`HomeSessionLocation`).
    pub hovered: Option<String>,
    /// Alt is held: every row shows its worktree and branch (`alt-hold.ts`, Desktop only).
    pub alt_held: bool,
    /// The list's order while the pointer is on it (`view::home::list`).
    pub hold: HomeHold,
    /// The exact match, as a list: what [`Search::exact`] holds, for readers that look sessions up.
    pub loaded: Vec<SessionInfo>,
    /// The id lookup on its worker: its attempt and what it found.
    lookup: Latest<(Attempt, Option<SessionInfo>)>,
    /// Scoped session fetch for the selected Home project (`projectSessionsQuery`).
    pub project_sessions: crate::home_project_sessions::ProjectSessions,
}

impl Default for HomeSearch {
    fn default() -> Self {
        HomeSearch {
            search: Search::default(),
            active: Highlight::default(),
            hovered: None,
            alt_held: false,
            hold: HomeHold::Free,
            loaded: Vec::new(),
            lookup: Latest::default(),
            project_sessions: crate::home_project_sessions::ProjectSessions::default(),
        }
    }
}

impl HomeSearch {
    pub fn set_waker(&mut self, waker: Waker) {
        self.lookup = Latest::new(waker.clone());
        self.project_sessions.set_waker(waker);
    }

    pub fn activate(&mut self, id: String) {
        self.active.set(id);
    }

    /// The query changed: the first result lights (search-machine.ts:87).
    pub fn query_changed(&mut self) {
        self.active.clear();
    }
}

/// Desktop's `looksLikeSessionID`: `value.length > 20 && value.startsWith("ses_")`.
pub fn looks_like_session_id(s: &str) -> bool {
    s.len() > 20 && s.starts_with("ses_")
}

pub fn focus_home_search(host: &mut dyn HomeSearchHost, tx: &mut Tx) {
    host.before_focus_home_search();
    {
        let edit = host.search_input_mut();
        edit.focused = true;
        let len = edit.text.len();
        edit.set_selection(0, len);
    }
    home_search_step(host, Event::Focused, tx);
    tx.instant();
}

pub fn clear_home_search(host: &mut dyn HomeSearchHost, keep_focus: bool, tx: &mut Tx) {
    home_search_step(host, Event::Closed, tx);
    host.search_input_mut().focused = keep_focus;
    if keep_focus {
        home_search_step(host, Event::Focused, tx);
    }
    tx.instant();
}

/// Steps the search's machine and carries out what it asks.
fn home_search_step(host: &mut dyn HomeSearchHost, event: Event<'_>, tx: &mut Tx) {
    let effects = host.home_search_mut().search.step(event);
    for effect in effects {
        match effect {
            Effect::Lookup { attempt, id } => {
                let Some(server) = host.live_server() else { continue };
                host.home_search_mut().lookup.request(id, move |id| {
                    let found = server.get(&format!("/api/session/{id}")).ok().map(|v| model::parse_session(v.get("data").unwrap_or(&v)));
                    // An archived session isn't one Home finds (`lookup`).
                    (attempt, found.filter(|s| s.archived.is_none()))
                });
            }
            Effect::Abort => host.home_search_mut().lookup.cancel(),
            Effect::Clear => {
                let edit = host.search_input_mut();
                edit.clear();
                edit.cancel_composition();
                edit.focused = false;
                host.home_search_mut().active.clear();
            }
            Effect::Unhighlight => host.home_search_mut().active.clear(),
        }
    }
    let hs = host.home_search_mut();
    hs.loaded = hs.search.exact().cloned().into_iter().collect();
    if let Some(due) = hs.search.due() {
        tx.wake_at(due);
    }
}

/// A person's root sessions Home lists for `selected` project, newest first: at most
/// `HOME_SESSION_LIMIT` (64, `home/sessions/index.ts`). An automation's sessions
/// fold away (`view::home::list`). Search never narrows them; it shows its matches in its own panel.
pub fn home_sessions<'a>(host: &'a dyn HomeSearchHost, selected: Option<&Project>) -> Vec<&'a SessionInfo> {
    home_roots(host, selected).filter(|s| !is_automation(&s.title)).take(HOME_SESSION_LIMIT).collect()
}

/// The search panel's results (`search.ts`): sessions whose `"{title} {project}"` holds the
/// trimmed query, after the session a typed id found when it isn't one of them.
pub fn home_search_results<'a>(host: &'a dyn HomeSearchHost, selected: Option<&Project>) -> Vec<&'a SessionInfo> {
    let q = host.search_input().text.trim();
    if q.is_empty() {
        return vec![];
    }
    let project_id = selected.map(|p| p.id.as_str());
    // Multiple non-git folders share `id == "global"`: fetch a wider slice before directory filtering.
    let limit = if project_id == Some("global") { 200 } else { 50 };
    let mut records: Vec<&'a SessionInfo> = host
        .find_home_sessions(project_id, q, limit)
        .into_iter()
        .filter(|(score, _, _)| *score >= opencode_search::SUBSTRING)
        .filter_map(|(_, _, id)| host.search_session(&id))
        .filter(|s| s.archived.is_none() && selected.is_none_or(|proj| host.session_in_project(s, proj)))
        .take(50)
        .collect();
    // Also match quiet-project sessions loaded by `project_sessions` outside the global index.
    if let Some(proj) = selected {
        let (scope, _) = host.home_search_scope();
        let fold = opencode_search::SearchFold::for_query(q);
        let needle = fold.fold(q);
        for s in host.home_search().project_sessions.sessions_for(&scope) {
            if records.len() >= 50 {
                break;
            }
            if s.archived.is_none()
                && !records.iter().any(|r| r.id == s.id)
                && host.session_in_project(s, proj)
                && fold.fold(&s.title).contains(&needle)
            {
                records.push(s);
            }
        }
    }
    // Then the sessions only their messages match (full text), each with its line of context.
    for r in host.fulltext_home_rows() {
        if !records.iter().any(|s| s.id == r.session)
            && let Some(s) = host.search_session(&r.session).or(r.fallback.as_ref())
            && selected.is_none_or(|proj| host.session_in_project(s, proj))
        {
            records.push(s);
        }
    }
    match host
        .home_search()
        .search
        .exact()
        .filter(|exact| !records.iter().any(|r| r.id == exact.id) && selected.is_none_or(|proj| host.session_in_project(exact, proj)))
    {
        Some(exact) => std::iter::once(exact).chain(records).collect(),
        None => records,
    }
}

pub fn home_roots<'a>(host: &'a dyn HomeSearchHost, selected: Option<&Project>) -> impl Iterator<Item = &'a SessionInfo> + use<'a> {
    let (scope, _) = host.home_search_scope();
    let extra = if selected.is_some() { host.home_search().project_sessions.sessions_for(&scope) } else { &[] };
    let selected = selected.cloned();
    host.merged_roots_with(extra)
        .into_iter()
        .filter(move |ses| selected.as_ref().is_none_or(|proj| host.session_in_project(ses, proj)))
}

/// Opens result `id`, closing the search; a session only its messages matched opens with Find
/// on the query at what matched.
pub fn open_home_result(host: &mut dyn HomeSearchHost, id: &str, tx: &mut Tx) {
    let q = host.search_input().text.trim().to_owned();
    let found = crate::fulltext::found_message_in_rows(host.fulltext_home_rows(), host.session(id), id, &q);
    clear_home_search(host, false, tx);
    host.open_session(id, tx);
    if let Some(message) = found {
        host.open_find_at(&q, &message, tx);
    }
}

/// Whether the results panel shows: while the search has focus and text (`selectSearchOpen`),
/// never while filtering Home's list in place.
pub fn home_search_open(host: &dyn HomeSearchHost) -> bool {
    !host.search_inline() && host.home_search().search.open() && !host.search_input().text.trim().is_empty()
}

/// Whether a typed id is being looked up, or a large list's index is still building, which
/// shows the panel's loader (`searchLoading`).
pub fn home_search_loading(host: &dyn HomeSearchHost) -> bool {
    matches!(host.home_search().search.status, Status::LookingUp { .. })
        || (!host.search_input().text.trim().is_empty() && host.session_search_building())
}

/// The highlighted result while the panel shows: the one last pointed or stepped to, else
/// the first (`search.ts`'s `active`).
pub fn home_search_active(host: &dyn HomeSearchHost, ids: &[String]) -> Option<String> {
    if !home_search_open(host) || ids.is_empty() {
        return None;
    }
    host.home_search().active.resolve(ids.iter().map(String::as_str))
}

pub fn home_search_key(host: &mut dyn HomeSearchHost, ev: &KeyEvent, tx: &mut Tx) -> bool {
    if host.search_input().is_composing() {
        return false;
    }
    if ev.key == Key::Escape {
        clear_home_search(host, false, tx);
        return true;
    }
    let m = ev.mods;
    // Filtering in place, Enter opens the first match (`HomeSessionSearch`'s `searchInline`).
    if host.search_inline() && ev.key == Key::Enter && !m.shift && !m.alt && !m.cmd && !m.ctrl {
        let selected = host.home_selected().cloned();
        let first = home_search_results(host, selected.as_ref()).first().map(|s| s.id.clone());
        if let Some(id) = first {
            if host.session(&id).is_none()
                && let Some(ses) = host.home_search().search.exact().filter(|s| s.id == id).cloned()
            {
                host.push_session(ses);
            }
            open_home_result(host, &id, tx);
            return true;
        }
    }
    if !m.alt && !m.cmd && !m.ctrl && home_search_open(host) {
        let selected = host.home_selected().cloned();
        let ids: Vec<String> = home_search_results(host, selected.as_ref()).into_iter().map(|s| s.id.clone()).collect();
        match ev.key {
            Key::Down | Key::Up if !m.shift && !ids.is_empty() => {
                let active = home_search_active(host, &ids);
                let from = ids.iter().position(|id| Some(id) == active.as_ref());
                let next = cycle(ids.len(), from, if ev.key == Key::Down { 1 } else { -1 });
                let picked = ids[next].clone();
                host.home_search_mut().activate(picked.clone());
                host.prefetch_session(&picked);
                tx.instant();
                tx.scroll_into_view_in("home-search-list", format!("sr-{picked}"));
                return true;
            }
            Key::Enter if !m.shift => {
                if let Some(id) = home_search_active(host, &ids) {
                    if host.session(&id).is_none()
                        && let Some(ses) = host.home_search().search.exact().filter(|s| s.id == id).cloned()
                    {
                        host.push_session(ses);
                    }
                    open_home_result(host, &id, tx);
                }
                return true;
            }
            _ => {}
        }
    }
    host.handle_fallback_key(ev, tx)
}

/// Off Home nothing holds the list, and once the pointer has left it the rows glide to their
/// places on the layout spring. True when they move.
pub fn release_home_hold(host: &mut dyn HomeSearchHost, tx: &mut Tx) -> bool {
    let off = !host.is_home() || host.settings_open();
    let leaving = matches!(host.home_search().hold, HomeHold::Leaving { .. });
    if matches!(host.home_search().hold, HomeHold::Free) || !(off || leaving) {
        return false;
    }
    host.home_search_mut().hold = HomeHold::Free;
    tx.animated();
    !off
}

/// Keeps the search up with its field, focus, scope and lookup.
pub fn poll_home_search(host: &mut dyn HomeSearchHost, tx: &mut Tx) -> bool {
    let mut changed = release_home_hold(host, tx);
    if let Some((attempt, session)) = host.home_search_mut().lookup.take() {
        home_search_step(host, Event::Resolved { attempt, session: session.map(Box::new) }, tx);
        tx.instant();
        changed = true;
    }
    // The search goes with Home (`disposed`): back on Home, it starts empty.
    if !host.is_home() {
        host.home_search_mut().project_sessions.leave_home();
        if host.home_search().search.focused || !host.home_search().search.query.is_empty() || !host.search_input().text.is_empty() {
            home_search_step(host, Event::Closed, tx);
            host.home_search_mut().search = Search::default();
        }
        return changed;
    }
    if host.settings_open() {
        return changed;
    }
    // Focus anywhere in the search keeps it open; leaving it by the keys keeps its text, a
    // press anywhere else clears it (`blur`, and the document `pointerdown` that closes it).
    let within = host.search_input().focused || tx.focused() == Some("search-clear");
    if within != host.home_search().search.focused {
        let event = if within { Event::Focused } else if tx.focus_visible() { Event::Blurred } else { Event::Closed };
        home_search_step(host, event, tx);
        tx.instant();
        changed = true;
    }
    let (scope, connected) = host.home_search_scope();
    let project_target = host.home_selected().map(|proj| crate::home_project_sessions::Target {
        scope: scope.clone(),
        directories: host.project_directories(proj),
    });
    let live_server = host.live_server();
    if host.home_search_mut().project_sessions.poll(project_target, connected, live_server) {
        tx.instant();
        changed = true;
    }
    let query = host.search_input().text.trim().to_owned();
    let now = host.clock();
    let before = host.home_search().search.mark();
    home_search_step(host, Event::Synced { query: &query, scope: &scope, connected, now }, tx);
    home_search_step(host, Event::Tick { now }, tx);
    changed | (host.home_search().search.mark() != before)
}
