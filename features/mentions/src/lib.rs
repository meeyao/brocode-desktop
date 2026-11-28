//! `@` in the composer: Desktop's context popover (`composer/editor/interaction.ts`,
//! `suggestions/machine.ts`, `composer/model.ts`). Typing `@` after a space or at the start
//! opens a list of skills, subagents, sessions and the server's file search; picking one puts
//! an atomic pill in the text, and sending turns the pills into the prompt's `agents`, `skills`
//! and `files` and session tokens (`composer/request.ts`, `prompt-parts.ts`). Whether the list
//! is open, and its query and highlight, are owned by the host's suggestion machine; the server's
//! searches for it are [`search`]'s.

#![forbid(unsafe_code)]

use std::cell::RefCell;

use opencode_kit::client::{Request, Server};
use opencode_kit::model::{self, SessionInfo};
use opencode_search::path_index::LivePaths;
use opencode_search::query::percent_encode;
use rune::machine::{Attempt, Seq};
use rune::{Cx, El, KeyEvent, Latest, PointerPress, TextEdit, Tx, Waker, icon};
use rune_theme_opencode::icons as ic;
use serde_json::Value;
use opencode_ui::palette::Pal;
use opencode_ui::style::SESSION as SPEECH_BUBBLE;

pub mod badge;
pub mod fuzzysort;
pub mod index;
pub mod list;
pub mod refs;
pub mod search;
pub mod wire;

pub use badge::{bubble, bubble_text, echo, echo_mentions, echo_wire_mentions, kind_of, mention_title, open_mentioned};
pub use index::{SessionsStamp, mention_items, mention_sessions, merged_roots, shown_mention};
pub use refs::{ContextMenuItem, ContextMenuSpec, open_session_chip_menu, session_ref_of};
pub use wire::{Wire, context_input, wire};

use list::{Row, Spec, data_list, popup, upsert_sessions};

/// Session rows: Desktop shows at most 5 (`composer/model.ts:405`).
pub(crate) const SESSIONS: usize = 5;
/// The server's default `limit` for `/api/fs/find`.
pub(crate) const FILE_LIMIT: usize = 50;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Skill,
    Agent,
    Session,
    /// A project reference (`/api/reference`), mentioned as its folder.
    Reference,
    File,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    /// `kind:value`, also the pill's id: `skill:<id>`, `agent:<name>`, `session:<id>`,
    /// `file:<path>`.
    pub key: String,
    pub kind: Kind,
    pub label: String,
    pub description: String,
    /// The pill's text.
    pub content: String,
    /// A file open in the session's side tabs (Desktop's `recent`), which the query filters.
    pub recent: bool,
}

/// `/api/reference`'s shown references (`composer/model.ts:181-200`).
#[derive(Clone, Debug, PartialEq)]
pub struct Reference {
    pub name: String,
    pub path: String,
    /// Its own, or where it comes from: the git repository or the local folder.
    pub description: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub description: String,
}

/// Callback invoked when a context menu item is chosen (or `None` when dismissed).
pub type ContextMenuPick = Box<dyn FnOnce(&mut dyn MentionsHost, Option<&str>, &mut Tx) + 'static>;

/// A directory's subagents, skills and references, each `None` when its read failed.
type Catalog = (Attempt, Option<Vec<(String, String)>>, Option<Vec<Skill>>, Option<Vec<Reference>>);

/// Object-safe host boundary for the `@` mentions popover, search polling, mention badges, and
/// session chips.
pub trait MentionsHost: opencode_kernel::AppEnv + opencode_kernel::AppFeedback {
    fn mentions(&self) -> &Mentions;
    fn mentions_mut(&mut self) -> &mut Mentions;

    fn session_refs(&self) -> &refs::Lookups;
    fn session_refs_mut(&mut self) -> &mut refs::Lookups;

    fn composer(&self) -> &TextEdit;
    fn composer_mut(&mut self) -> &mut TextEdit;

    /// The composer's directory: the active session's, or where a new draft starts.
    fn active_directory(&self) -> String;
    /// Metadata for session `id`, if loaded.
    fn session_info(&self, id: &str) -> Option<&SessionInfo>;
    /// All loaded session metadata.
    fn sessions(&self) -> &[SessionInfo];
    /// Effective clamped recency timestamp for ordering `session` (`recent_page.active_ms`).
    fn session_active_ms(&self, session: &SessionInfo) -> i64 {
        session.active_ms()
    }
    /// Whether session `id` is currently running (used for session chips).
    fn is_session_live(&self, id: &str) -> bool {
        let _ = id;
        false
    }
    /// Project ID associated with `dir`, if any.
    fn project_id_at(&self, dir: &str) -> Option<String> {
        let _ = dir;
        None
    }
    /// Display label for `session`'s project (defaults to directory basename).
    fn project_label(&self, session: &SessionInfo) -> String {
        opencode_kit::path::basename(&session.directory).to_owned()
    }
    /// Whether `id` is currently open in a session tab.
    fn is_session_tab_open(&self, id: &str) -> bool {
        let _ = id;
        false
    }
    /// Cache-invalidation stamp for loaded sessions, projects, and tabs.
    fn sessions_stamp(&self) -> SessionsStamp {
        SessionsStamp::default()
    }
    /// Side-region tab keys `(strip_keys, active_key)` for the routed session, used to surface
    /// open `"file:"` tabs as recent mentions via `opencode_search::file_rank::file_tab_path`.
    fn side_tab_keys(&self) -> Option<(Vec<String>, Option<String>)> {
        None
    }

    /// Live OpenCode server connection for `/api/agent`, `/api/skill`, `/api/reference`,
    /// `/api/fs/find`, and `/api/session` searches.
    fn live_server(&self) -> Option<Server> {
        None
    }
    /// Sends a background read request (such as `Request::Lookup` or `Request::Info`).
    fn send_request(&self, req: Request) {
        let _ = req;
    }
    /// Inserts placeholder or found `SessionInfo` before navigating to an unloaded session.
    fn push_session_info(&mut self, info: SessionInfo) {
        let _ = info;
    }
    /// Requests metadata for an unloaded session `id` (defaults to `Request::Info`).
    fn fetch_session_info(&self, id: &str) {
        self.send_request(Request::Info(id.to_owned()));
    }

    /// Opens session `id` (invoked when a session mention badge is pressed).
    fn open_session(&mut self, id: &str, tx: &mut Tx);
    /// Opens file `path` in the host, if supported.
    fn open_file(&mut self, path: &str, tx: &mut Tx) {
        let _ = (path, tx);
    }
    /// Opens a right-click context menu at `press`, invoking `on_pick` with the selected item id
    /// (or `None` when dismissed).
    fn open_context_menu(
        &mut self,
        _spec: ContextMenuSpec,
        _press: PointerPress,
        _tx: &mut Tx,
        _on_pick: ContextMenuPick,
    ) {
    }
    /// Whether picking a popover row should focus the composer.
    fn auto_focus_composer(&self, tx: &Tx) -> bool {
        !opencode_ui::style::touch_controls() || tx.by_keys()
    }

    /// The open `@` context popover's query (`None` when closed).
    fn mention_query(&self) -> Option<&str>;
    /// The highlighted row key in the open `@` context popover.
    fn mention_active(&self) -> Option<&str>;
    /// Reconciles suggestion highlight and auto-close rules after mention search results land.
    fn refresh_suggestions(&mut self, tx: &mut Tx);
    /// Pointer moved over suggestion row `key`.
    fn suggest_pointed(&mut self, key: &str, tx: &mut Tx);
    /// User picked suggestion row `key`.
    fn suggest_pick(&mut self, key: &str, tx: &mut Tx);
    /// Search input used by [`list::suggest_search`] (commands menu and prompt history).
    fn suggest_search_mut(&mut self) -> &mut TextEdit {
        &mut self.mentions_mut().scratch_search
    }
    /// Search input text changed in [`list::suggest_search`].
    fn suggest_query(&mut self, tx: &mut Tx) {
        let _ = tx;
    }
    /// Key event in [`list::suggest_search`] or while the mentions popover is open.
    fn suggest_key(&mut self, ev: &KeyEvent, tx: &mut Tx) -> bool {
        let _ = (ev, tx);
        false
    }
    /// Opens the `@` mentions popover in the host's suggestion machine.
    fn open_mentions_popover(&mut self, tx: &mut Tx) {
        let _ = tx;
    }
    /// Dismisses the active suggestion popover in the host's suggestion machine.
    fn dismiss_suggestions(&mut self, tx: &mut Tx) {
        let _ = tx;
    }
    /// Called after [`pick`] inserts a mention pill into the composer.
    fn on_mention_inserted(&mut self, item: &Item) {
        let _ = item;
    }
}

#[derive(Default)]
pub struct Mentions {
    /// Subagents (`mode != primary`, not hidden) by name, with their descriptions.
    pub agents: Vec<(String, String)>,
    pub skills: Vec<Skill>,
    pub references: Vec<Reference>,
    /// Whose `agents` and `skills` these are.
    catalog: search::Catalog,
    catalog_fetch: Latest<Catalog>,
    /// The open list's file and session searches.
    pub search: search::Search,
    pub seq: Seq,
    /// Sessions the server found for a query's first word.
    pub(crate) found: Vec<SessionInfo>,
    files_fetch: Latest<(Attempt, Option<Vec<String>>)>,
    sessions_fetch: Latest<(Attempt, Option<Vec<SessionInfo>>)>,
    /// The fixture's files, searched locally.
    pub file_index: Vec<String>,
    fixture_paths: RefCell<LivePaths>,
    /// What the rows are matched against, kept until what they come from changes.
    pub(crate) cache: RefCell<index::Cache>,
    /// Fallback search input for hosts that don't override [`MentionsHost::suggest_search_mut`].
    scratch_search: TextEdit,
}

impl Mentions {
    pub fn set_waker(&mut self, waker: Waker) {
        self.catalog_fetch.set_waker(waker.clone());
        self.files_fetch.set_waker(waker.clone());
        self.sessions_fetch.set_waker(waker.clone());
        self.fixture_paths.get_mut().set_waker(waker);
    }

    /// A session the `@` search found.
    pub fn found_session(&self, id: &str) -> Option<&SessionInfo> {
        self.found.iter().find(|s| s.id == id)
    }

    /// All sessions found by `@` session searches.
    pub fn found_sessions(&self) -> &[SessionInfo] {
        &self.found
    }

    /// Fixture data in the server's shapes: `agent_list` as `/api/agent`, `skills` as `/api/skill`.
    pub fn load(&mut self, fixture: &Value) {
        self.agents = subagents(&fixture["agent_list"]);
        self.skills = skills(&fixture["skills"]);
        self.references = references(&fixture["references"]);
        self.file_index = found_paths(&fixture["files"]);
        self.fixture_paths.get_mut().changed(self.file_index.iter().map(|p| shown_path(p)).collect());
        *self.cache.get_mut() = Default::default();
    }

    /// Searches the local fixture path index for up to `limit` paths matching `q`.
    pub fn local_paths(&self, q: &str, limit: usize) -> Vec<String> {
        self.fixture_paths.borrow().query(q, limit)
    }

    /// Which fixture path index answers: changes when one lands.
    pub fn paths_version(&self) -> u64 {
        self.fixture_paths.borrow().version()
    }

    /// Closes active file and session searches (`search::Event::Closed`).
    pub fn close_search(&mut self) {
        search::step(&mut self.seq, &mut self.search, search::Event::Closed);
    }

    /// Clears the cached mention-list projection.
    pub fn invalidate_cache(&mut self) {
        *self.cache.get_mut() = Default::default();
    }

    /// Marks the cached `/api/agent` + `/api/skill` catalog stale when `agent.updated` or
    /// `skill.updated` arrives (`packages/client/src/solid/data.ts:1522-1540`, `opencode#66813`).
    pub fn invalidate_catalog(&mut self, dir: Option<&str>) {
        search::step_catalog(&mut self.seq, &mut self.catalog, search::CatalogEvent::Invalidated { directory: dir });
    }
}

/// Display title for a session, falling back to the localized "Untitled session" label.
pub fn title_of(s: &SessionInfo) -> String {
    if s.title.is_empty() {
        opencode_i18n::t!("rune.session.untitled").into()
    } else {
        s.title.clone()
    }
}

/// `/api/agent`'s subagents: Desktop offers agents that aren't hidden or primary.
fn subagents(v: &Value) -> Vec<(String, String)> {
    data_list(v)
        .filter(|a| a["hidden"].as_bool() != Some(true) && a["mode"].as_str() != Some("primary"))
        .filter_map(|a| Some((a["id"].as_str().or(a["name"].as_str())?.to_owned(), a["description"].as_str().unwrap_or("").to_owned())))
        .collect()
}

/// A found path as a row and its pill say it: the server's path as it comes, separators and all,
/// as Desktop's `path.normalize` keeps them (`workspaces/files/path.ts:91-121`), so a folder
/// on Windows reads `src/util\` there too; a `./` before it goes.
pub fn shown_path(path: &str) -> String {
    path.strip_prefix("./").or_else(|| path.strip_prefix(".\\")).unwrap_or(path).to_owned()
}

/// `/api/fs/find`'s paths.
pub fn found_paths(v: &Value) -> Vec<String> {
    data_list(v).filter_map(|f| f["path"].as_str().or(f.as_str()).map(str::to_owned)).collect()
}

/// `/api/reference`'s references that aren't hidden, described by their own words or their source.
fn references(v: &Value) -> Vec<Reference> {
    data_list(v)
        .filter(|r| r["hidden"].as_bool() != Some(true))
        .filter_map(|r| {
            let source = &r["source"];
            let from = if source["type"].as_str() == Some("git") { &source["repository"] } else { &source["path"] };
            Some(Reference {
                name: r["name"].as_str()?.to_owned(),
                path: r["path"].as_str()?.to_owned(),
                description: r["description"].as_str().or(from.as_str()).unwrap_or("").to_owned(),
            })
        })
        .collect()
}

fn skills(v: &Value) -> Vec<Skill> {
    data_list(v)
        .filter_map(|s| {
            let id = s["id"].as_str()?.to_owned();
            Some(Skill { name: s["name"].as_str().unwrap_or(&id).to_owned(), description: s["description"].as_str().unwrap_or("").to_owned(), id })
        })
        .collect()
}

/// The `@` before the caret and the query after it. Closed, the query is one run of non-space
/// characters; open, single spaces join words (and one may trail), so multi-word session titles
/// stay searchable. The `@` starts the text or follows whitespace.
pub fn trigger(before: &str, open: bool) -> Option<(usize, &str)> {
    let at = before.rfind('@')?;
    if before[..at].chars().next_back().is_some_and(|c| !c.is_whitespace()) {
        return None;
    }
    let query = &before[at + 1..];
    let ok = if open {
        !query.starts_with(' ') && !query.contains("  ") && query.chars().all(|c| c == ' ' || !c.is_whitespace())
    } else {
        !query.chars().any(char::is_whitespace)
    };
    ok.then_some((at, query))
}

// ---------------------------------------------------------------------------------------------
// Non-generic host entrypoints

/// Connects the window waker to all background fetchers and the fixture path index.
pub fn setup(host: &mut dyn MentionsHost, waker: Waker) {
    host.mentions_mut().set_waker(waker);
}

/// Whether there is a live server to search (fixtures search locally).
pub fn is_server(host: &dyn MentionsHost) -> bool {
    host.is_live()
}

/// The open `@` list's query.
pub fn query(host: &dyn MentionsHost) -> Option<String> {
    host.mention_query().map(str::to_owned)
}

/// The highlighted row key in the open `@` list.
pub fn active(host: &dyn MentionsHost) -> Option<String> {
    host.mention_active().map(str::to_owned)
}

/// Opens the `@` mentions popover and syncs its searches.
pub fn open(host: &mut dyn MentionsHost, tx: &mut Tx) {
    host.open_mentions_popover(tx);
    sync_search(host, tx);
}

/// Closes active mention searches and dismisses the suggestion popover.
pub fn close(host: &mut dyn MentionsHost, tx: &mut Tx) {
    close_search(host);
    host.dismiss_suggestions(tx);
}

/// Closes active file and session searches without dismissing unrelated popovers.
pub fn close_search(host: &mut dyn MentionsHost) {
    if !matches!(host.mentions().search, search::Search::Closed) {
        host.mentions_mut().close_search();
    }
}

/// Routes a key event to the open `@` mentions popover; returns `false` when closed.
pub fn key(host: &mut dyn MentionsHost, ev: &KeyEvent, tx: &mut Tx) -> bool {
    if host.mention_query().is_none() {
        return false;
    }
    host.suggest_key(ev, tx)
}

/// The row's pill replaces the `@query` before the caret, with a space after it
/// (`addMention`): from the last `@` before the caret that isn't a pill's, or at the caret
/// when there's none.
pub fn pick(host: &mut dyn MentionsHost, key: &str) {
    let Some(item) = index::mention_items(host)
        .into_iter()
        .find(|i| i.key == key)
        .or_else(|| index::shown_mention(host, key))
    else {
        return;
    };
    let composer = host.composer_mut();
    let caret = composer.focus.min(composer.text.len());
    let pills: Vec<_> = composer.pills().map(|(r, _)| r).collect();
    let at = composer.text[..caret]
        .rmatch_indices('@')
        .map(|(i, _)| i)
        .find(|i| !pills.iter().any(|r| r.start <= *i && *i < r.end))
        .unwrap_or(caret);
    composer.replace_with_pill(at..caret, item.key.clone(), &item.content);
    composer.insert(" ");
    host.on_mention_inserted(&item);
}

/// The open list's searches follow its query (`context.changed`, `context.closed`), and the
/// directory's catalog is read once it opens.
pub fn sync_search(host: &mut dyn MentionsHost, tx: &mut Tx) {
    let now = tx.clock();
    let server = host.is_live();
    let query = host.mention_query().map(str::to_owned);
    if query.is_none() && matches!(host.mentions().search, search::Search::Closed) {
        return;
    }
    let effects = {
        let m = host.mentions_mut();
        match query.as_deref() {
            Some(q) => search::step(&mut m.seq, &mut m.search, search::Event::Changed { query: q, server, now }),
            None => search::step(&mut m.seq, &mut m.search, search::Event::Closed),
        }
    };
    run_search(host, effects, tx);
    let Some(server) = host.live_server() else { return };
    if query.is_none() {
        return;
    }
    let directory = host.active_directory();
    let opened = match &host.mentions().catalog {
        search::Catalog::Unread | search::Catalog::Failed { .. } => true,
        search::Catalog::Reading { directory: d, .. } | search::Catalog::Read { directory: d } if d == &directory => return,
        search::Catalog::Reading { .. } | search::Catalog::Read { .. } => false,
    };
    let m = host.mentions_mut();
    let Some((attempt, directory)) = search::step_catalog(&mut m.seq, &mut m.catalog, search::CatalogEvent::Wanted { directory: &directory, opened }) else {
        return;
    };
    // `/api/skill` answers 503 until the server's plugins load, which `get_ready` waits out.
    m.catalog_fetch.request(directory, move |directory| {
        let at = format!("location%5Bdirectory%5D={}", percent_encode(&directory));
        let agents = server.get(&format!("/api/agent?{at}")).ok().map(|v| subagents(&v));
        let skills = server.get_ready(&format!("/api/skill?{at}")).ok().map(|v| skills(&v));
        let references = server.get(&format!("/api/reference?{at}")).ok().map(|v| references(&v));
        (attempt, agents, skills, references)
    });
}

fn run_search(host: &mut dyn MentionsHost, effects: impl IntoIterator<Item = search::Effect>, tx: &mut Tx) {
    let Some(server) = host.live_server() else {
        for effect in effects {
            if let search::Effect::WakeAt(due) = effect {
                tx.wake_at(due);
            }
        }
        return;
    };
    let directory = host.active_directory();
    for effect in effects {
        match effect {
            search::Effect::WakeAt(due) => tx.wake_at(due),
            search::Effect::SearchFiles { attempt, query } => {
                let (server, directory) = (server.clone(), directory.clone());
                host.mentions_mut().files_fetch.request(query, move |query| {
                    let path = format!("/api/fs/find?location%5Bdirectory%5D={}&query={}", percent_encode(&directory), percent_encode(&query));
                    (attempt, server.get(&path).ok().map(|v| found_paths(&v)))
                });
            }
            search::Effect::SearchSessions { attempt, word } => {
                let server = server.clone();
                host.mentions_mut().sessions_fetch.request(word, move |word| {
                    let path = format!("/api/session?parentID=null&archived=false&limit=50&search={}", percent_encode(&word));
                    (attempt, server.get(&path).ok().map(|v| model::parse_sessions(&v)))
                });
            }
        }
    }
}

/// Takes in what the searches answered and the timers that came due; the open list's rows
/// land with this frame, without fading in or sliding.
pub fn poll(host: &mut dyn MentionsHost, tx: &mut Tx) -> bool {
    let now = tx.clock();
    // Every poll comes here: what arrived is taken without writing, and the mentions are
    // written only when something did or the search came due.
    let (catalog, files, sessions, due, indexed) = {
        let m = host.mentions();
        (
            m.catalog_fetch.take(),
            m.files_fetch.take(),
            m.sessions_fetch.take(),
            m.search.next_due().is_some_and(|due| now + 1e-9 >= due),
            m.fixture_paths.borrow_mut().poll(),
        )
    };
    if catalog.is_none() && files.is_none() && sessions.is_none() && !due {
        if indexed {
            host.mentions_mut().invalidate_cache();
        }
        if host.is_live() && host.mention_query().is_some() && matches!(host.mentions().catalog, search::Catalog::Unread) {
            sync_search(host, tx);
            return true;
        }
        return indexed;
    }
    let (changed, effects) = {
        let m = host.mentions_mut();
        let mut changed = indexed;
        if let Some((attempt, agents, skills, references)) = catalog {
            // A failed read keeps what was there, as Desktop's location data does, rather than
            // settling on an empty list.
            let failed = agents.is_none() || skills.is_none() || references.is_none();
            if matches!(&m.catalog, search::Catalog::Reading { attempt: a, .. } if *a == attempt) {
                if let Some(agents) = agents {
                    m.agents = agents;
                }
                if let Some(skills) = skills {
                    m.skills = skills;
                }
                if let Some(references) = references {
                    m.references = references;
                }
                *m.cache.get_mut() = Default::default();
                changed = true;
            }
            search::step_catalog(&mut m.seq, &mut m.catalog, search::CatalogEvent::Answered { attempt, failed });
        }
        let mut effects = Vec::new();
        if let Some((attempt, paths)) = files {
            let before = m.search.clone();
            effects.extend(search::step(&mut m.seq, &mut m.search, search::Event::Files { attempt, paths }));
            changed |= m.search != before;
        }
        if let Some((attempt, list)) = sessions {
            if let Some(list) = list
                && matches!(&m.search, search::Search::Active { sessions: search::Sessions::Loading { attempt: current, .. }, .. } if *current == attempt)
            {
                upsert_sessions(&mut m.found, list);
                *m.cache.get_mut() = Default::default();
                changed = true;
            }
            effects.extend(search::step(&mut m.seq, &mut m.search, search::Event::Sessions { attempt }));
        }
        if m.search.next_due().is_some_and(|due| now + 1e-9 >= due) {
            let before = m.search.clone();
            effects.extend(search::step(&mut m.seq, &mut m.search, search::Event::Due { now }));
            changed |= m.search != before;
        }
        (changed, effects)
    };
    run_search(host, effects, tx);
    if changed && host.mention_query().is_some() {
        tx.instant();
        host.refresh_suggestions(tx);
    }
    changed
}

/// The prompt's text and mentions for what's in the composer.
pub fn mention_wire(host: &dyn MentionsHost) -> Wire {
    let composer = host.composer();
    let pills: Vec<_> = composer.pills().collect();
    let directory = host.active_directory();
    let skills = &host.mentions().skills;
    wire(&composer.text, &pills, &directory, |id| skills.iter().find(|s| s.id == id).map_or_else(|| id.to_owned(), |s| s.name.clone()))
}

// ---------------------------------------------------------------------------------------------
// View

/// A row's mark (`editor.tsx:1740-1755`): 16 pt `comment` (session) and `post-skill` (skill),
/// 14 pt `brain` (`size="small"`, `icon-info-active`), 16 pt colour file/folder badges.
fn row_icon(p: &Pal, item: &Item) -> El {
    let ink = if p.dark { rune::Color::WHITE } else { rune::Color::BLACK };
    let info = rune::color::hex(if p.dark { "#fabbff" } else { "#912a9a" }).unwrap_or(p.info);
    match item.kind {
        Kind::Session => icon(&SPEECH_BUBBLE, 16.0).color(ink),
        Kind::Agent => icon(&ic::BRAIN, 14.0).color(info),
        Kind::Skill => icon(&ic::POST_SKILL, 16.0).color(ink),
        Kind::File => icon(rune_theme_opencode::file_icons::file(&item.label), 16.0).color(p.text),
        Kind::Reference => icon(rune_theme_opencode::file_icons::folder(item.key.strip_prefix("reference:").unwrap_or(&item.label), false), 16.0).color(p.text),
    }
}

fn prepare_view(host: &dyn MentionsHost, p: &Pal, width: f32, boundary: Option<f32>) -> Option<(Spec, Vec<Row>, Option<String>)> {
    host.mention_query()?;
    let items = index::mention_items(host);
    let active = host.mention_active().map(str::to_owned);
    let rows = items
        .iter()
        .map(|i| Row {
            id: i.key.clone(),
            key: format!("mention-{}", i.key),
            label: i.label.clone(),
            lead: Some(row_icon(p, i)),
            description: i.description.clone(),
            trail: None,
        })
        .collect();
    let spec = Spec {
        key: "mentions",
        label: opencode_i18n::t!("rune.composer.mentions"),
        empty: opencode_i18n::t!("ui.promptInput.noMatchingItems"),
        width,
        boundary,
        search: None,
    };
    Some((spec, rows, active))
}

fn on_pointed(host: &mut dyn MentionsHost, key: &str, tx: &mut Tx) {
    host.suggest_pointed(key, tx);
}

fn on_pick(host: &mut dyn MentionsHost, key: &str, tx: &mut Tx) {
    host.suggest_pick(key, tx);
}

/// The popover over the composer, full width, 8 points up: rows of icon, label and a muted
/// description. It appears and goes at once.
pub fn view<H: MentionsHost + 'static>(s: &H, p: &Pal, cx: &Cx<H>) -> Option<El> {
    s.mention_query()?;
    let width = cx.layout_size("composer").map_or(0.0, |[w, _]| w);
    let (spec, rows, active) = prepare_view(s, p, width, list::boundary(cx))?;
    Some(popup(p, spec, rows, active.as_deref(), |s: &mut H, key, tx| on_pointed(s, key, tx), |s: &mut H, key, tx| on_pick(s, key, tx)))
}
