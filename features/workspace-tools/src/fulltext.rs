//! Full text (better-search.md tier 2): message text kept on this device, searched by the storage
//! lane's tables (`opencode_storage::native::search`, STORAGE-4). They hold the copy of each
//! recently shown session (its last 200 messages), every tab's draft and the queued prompts, and
//! answer a query with up to 250 of the newest rows that match; `opencode_search::fulltext` ranks
//! them, two at most a session. The palette and Home's search each keep one, which asks for a
//! query of two characters or more; the newest question wins (`SearchSurface`).
//!
//! A profile without the tables (schema 2 off, `RUNE_STORAGE_V2`; storage in memory; the browser)
//! answers each question with an error: the palette and Home then search what official's do
//! (titles; Home's projects too), so they announce nothing.

use std::sync::{Arc, Mutex};

use opencode_kit::model::{Project, SessionInfo};
use opencode_search::SearchFold;
use opencode_search::fulltext::{self, Kind, Snippet};
use opencode_storage::Storage;

/// The fewest characters a question has: one letter matches nearly every message.
const MIN_CHARS: usize = 2;
/// Characters of context a snippet shows around its first match.
const SNIPPET: usize = 90;
/// Rows a surface shows from full text, after its title matches.
pub const ROWS: usize = 20;

/// Object-safe host boundary for local full-text message search across the palette and Home surfaces.
pub trait FulltextHost: opencode_kernel::AppEnv {
    fn fulltext(&self) -> &Surfaces;
    fn fulltext_mut(&mut self) -> &mut Surfaces;

    /// Raw search query currently active on `which` (`""` when closed or not searching).
    fn surface_query(&self, which: Surface) -> &str;
    /// Active server key string (matches `search_meta.server`).
    fn active_server_key(&self) -> &str;
    /// Whether the palette is currently searching archived sessions.
    fn palette_archived(&self) -> bool {
        false
    }
    /// Selected Home project id filter, if any.
    fn home_selected_id(&self) -> Option<String> {
        None
    }

    fn root_of<'a>(&'a self, id: &'a str) -> &'a str {
        id
    }
    fn session(&self, id: &str) -> Option<&SessionInfo>;
    fn search_session(&self, id: &str) -> Option<&SessionInfo>;
    fn project_for_session(&self, session: &SessionInfo) -> Option<&Project>;
    /// Fallback `(title, directory)` from an open tab matching `session_id` with a non-empty title.
    fn tab_title_and_dir(&self, session_id: &str) -> Option<(&str, &str)> {
        let _ = session_id;
        None
    }

    /// Runs `op` against the process's [`Storage`] instance (immediately if open, else queued).
    fn with_storage(&self, op: Box<dyn FnOnce(&Storage) + Send>);
}

/// A ranked row: the session, what matched and its line of context.
#[derive(Clone, Debug)]
pub struct Row {
    pub session: String,
    pub kind: Kind,
    /// The message that matched, when it's one of the session's (not a draft or a queued prompt).
    pub message: Option<String>,
    pub snippet: Snippet,
    /// The session's other rows that matched, left out.
    pub more: usize,
    /// The session as its tab (or storage) last saw it, when the list doesn't have it: offline, a
    /// session older than the loaded pages.
    pub fallback: Option<SessionInfo>,
}

/// What a surface shows from full text.
#[derive(Clone)]
pub enum Shown {
    Nothing,
    Rows(Arc<Vec<Row>>),
    /// The profile has no search tables: message text isn't searched.
    Off,
}

/// The storage lane's answer to `query`, as it lands.
struct Landed {
    query: String,
    hits: Result<(Coverage, Vec<Stored>), String>,
}

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
#[derive(Clone, Copy, PartialEq)]
enum Coverage {
    Partial,
    Complete,
}

/// A row as storage keeps it.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
struct Stored {
    server: String,
    session: String,
    entity: String,
    kind: String,
    title: String,
    body: String,
    updated_ms: i64,
}

/// One surface's full text: nothing asked (no query, or under [`MIN_CHARS`]); a question out,
/// showing what the last one found meanwhile; answered; or off (the profile has no tables).
#[derive(Default)]
enum State {
    #[default]
    Idle,
    Asking { query: String, shown: Last },
    Answered { query: String, rows: Arc<Vec<Row>>, coverage: Coverage },
    Off { query: String },
}

/// What the last answer was, shown while the next question is out so a key doesn't blink.
#[derive(Default)]
enum Last {
    #[default]
    Nothing,
    Rows(Arc<Vec<Row>>, Coverage),
    Off,
}

enum Event {
    Typed(String),
    /// An answer for `query`: its rows ranked, or the reason there are none.
    Landed { query: String, rows: Result<(Coverage, Vec<Row>), String> },
}

enum Effect {
    Ask(String),
    Cancel,
}

impl State {
    fn query(&self) -> &str {
        match self {
            State::Idle => "",
            State::Asking { query, .. } | State::Answered { query, .. } | State::Off { query } => query,
        }
    }

    fn step(&mut self, event: Event) -> Option<Effect> {
        match (std::mem::take(self), event) {
            (state, Event::Typed(q)) if q == state.query() => {
                *self = state;
                None
            }
            // Too short to ask: a question out is cancelled once, and an idle surface stays idle
            // (no effect, so the poll asks for no frame).
            (state, Event::Typed(q)) if q.chars().count() < MIN_CHARS => (!matches!(state, State::Idle)).then_some(Effect::Cancel),
            (state, Event::Typed(q)) => {
                let shown = match state {
                    State::Idle => Last::Nothing,
                    State::Asking { shown, .. } => shown,
                    State::Answered { rows, coverage, .. } => Last::Rows(rows, coverage),
                    State::Off { .. } => Last::Off,
                };
                *self = State::Asking { query: q.clone(), shown };
                Some(Effect::Ask(q))
            }
            (State::Asking { query, .. } | State::Answered { query, coverage: Coverage::Partial, .. }, Event::Landed { query: answered, rows: Ok((coverage, rows)) }) if answered == query => {
                *self = State::Answered { query, rows: Arc::new(rows), coverage };
                None
            }
            (State::Answered { query, coverage: Coverage::Partial, .. }, Event::Landed { query: answered, rows: Err(_) }) if answered == query => {
                *self = State::Off { query };
                None
            }
            (State::Asking { query, shown }, Event::Landed { query: answered, rows: Err(why) }) if answered == query => {
                // Said once, when it turns off, not on every question.
                if !matches!(shown, Last::Off) {
                    eprintln!("brocode: message text isn't searched: {why}");
                }
                *self = State::Off { query };
                None
            }
            (state, Event::Landed { .. }) => {
                *self = state;
                None
            }
        }
    }

    fn shown(&self) -> Shown {
        match self {
            State::Idle => Shown::Nothing,
            State::Asking { shown: Last::Nothing, .. } => Shown::Nothing,
            State::Asking { shown: Last::Rows(rows, _), .. } | State::Answered { rows, .. } => Shown::Rows(rows.clone()),
            State::Asking { shown: Last::Off, .. } | State::Off { .. } => Shown::Off,
        }
    }
}

/// Which surface asks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    Palette,
    Home,
}

#[derive(Default)]
pub struct FullText {
    state: State,
    /// Answers from the storage thread, for the window's next poll.
    inbox: Arc<Mutex<Vec<Landed>>>,
    #[cfg(not(target_arch = "wasm32"))]
    surface: opencode_storage::native::search::SearchSurface,
}

#[derive(Default)]
pub struct Surfaces {
    palette: FullText,
    home: FullText,
}

impl Surfaces {
    fn get(&self, which: Surface) -> &FullText {
        match which {
            Surface::Palette => &self.palette,
            Surface::Home => &self.home,
        }
    }

    fn get_mut(&mut self, which: Surface) -> &mut FullText {
        match which {
            Surface::Palette => &mut self.palette,
            Surface::Home => &mut self.home,
        }
    }

    pub fn shown(&self, which: Surface) -> Shown {
        self.get(which).state.shown()
    }

    pub fn indexing_history(&self, which: Surface) -> bool {
        matches!(self.get(which).state, State::Answered { coverage: Coverage::Partial, .. } | State::Asking { shown: Last::Rows(_, Coverage::Partial), .. })
    }

    pub fn rows(&self, which: Surface) -> &[Row] {
        match &self.get(which).state {
            State::Asking { shown: Last::Rows(rows, _), .. } | State::Answered { rows, .. } => rows,
            _ => &[],
        }
    }

    /// Shows `rows` on `which` as the whole answer for `query`, as if storage had found them
    /// (stories and fixtures, which have no search tables).
    pub fn stage(&mut self, which: Surface, query: &str, rows: Vec<Row>) {
        self.get_mut(which).state = State::Answered { query: query.to_owned(), rows: Arc::new(rows), coverage: Coverage::Complete };
    }

    pub fn row(&self, which: Surface, id: &str) -> Option<(Arc<Vec<Row>>, usize)> {
        let Shown::Rows(rows) = self.shown(which) else { return None };
        let at = rows.iter().position(|r| r.session == id)?;
        Some((rows, at))
    }
}

/// A reply's Markdown as a line of words: no fences, code ticks, emphasis marks, heading or
/// quote marks, table rules or pipes, and a link's text without its address.
fn plain(md: &str) -> String {
    let mut out = String::with_capacity(md.len());
    for line in md.lines() {
        let t = line.trim_start();
        if t.starts_with("```") || t.starts_with("~~~") || (t.starts_with('|') && t.chars().all(|c| matches!(c, '|' | '-' | ':' | ' '))) {
            continue;
        }
        let t = t.trim_start_matches('#').trim_start_matches('>').trim_start();
        let mut rest = t;
        while let Some(open) = rest.find('[') {
            out.push_str(&rest[..open]);
            let after = &rest[open + 1..];
            match after.find("](").and_then(|close| after[close + 2..].find(')').map(|end| (close, close + 2 + end))) {
                Some((close, end)) => {
                    out.push_str(&after[..close]);
                    rest = &after[end + 1..];
                }
                None => {
                    out.push('[');
                    rest = after;
                }
            }
        }
        out.push_str(rest);
        out.push(' ');
    }
    let out = out.replace("**", "").replace("__", "").replace('`', "").replace('|', " ");
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn kind(stored: &str) -> Kind {
    match stored {
        "draft" => Kind::Draft,
        // A queued prompt is the user's words.
        "outbox" | "user" => Kind::UserPrompt,
        _ => Kind::AssistantReply,
    }
}

/// What `which` shows from full text for its query now.
pub fn fulltext(host: &dyn FulltextHost, which: Surface) -> Shown {
    host.fulltext().shown(which)
}

pub fn indexing_history(host: &dyn FulltextHost, which: Surface) -> bool {
    host.fulltext().indexing_history(which)
}

/// The query each surface searches now, empty while it searches nothing.
fn fulltext_query(host: &dyn FulltextHost, which: Surface) -> String {
    host.surface_query(which).split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Asks each surface's question when its query changed, and lands the answers: true when
/// what shows changed.
pub fn poll_fulltext(host: &mut dyn FulltextHost) -> bool {
    let mut changed = false;
    for which in [Surface::Palette, Surface::Home] {
        let q = fulltext_query(host, which);
        let landed = std::mem::take(&mut *host.fulltext().get(which).inbox.lock().unwrap_or_else(|e| e.into_inner()));
        if let Some(Landed { query, hits }) = landed.into_iter().rev().find(|a| a.query == q) {
            let rows = hits.map(|(coverage, hits)| (coverage, fulltext_rank(host, which, &query, hits)));
            host.fulltext_mut().get_mut(which).state.step(Event::Landed { query, rows });
            changed = true;
        }
        let effect = host.fulltext_mut().get_mut(which).state.step(Event::Typed(q));
        match effect {
            Some(Effect::Ask(q)) => fulltext_ask(host, which, q),
            Some(Effect::Cancel) => {
                #[cfg(not(target_arch = "wasm32"))]
                host.fulltext().get(which).surface.cancel();
                changed = true;
            }
            None => {}
        }
    }
    changed
}

#[cfg(not(target_arch = "wasm32"))]
fn fulltext_ask(host: &dyn FulltextHost, which: Surface, query: String) {
    use opencode_storage::native::search::Scope;
    let ft = host.fulltext().get(which);
    let (surface, inbox, wake) = (ft.surface.clone(), ft.inbox.clone(), host.waker());
    let scope = Scope {
        server: Some(host.active_server_key().to_owned()),
        session: None,
        include_archived: which == Surface::Palette && host.palette_archived(),
    };
    host.with_storage(Box::new(move |storage| {
        let asked = query.clone();
        surface.query_live(storage, query, scope, move |_, result| {
            let hits = result
                .map(|(coverage, hits)| {
                    let coverage = match coverage {
                        opencode_storage::native::search::Coverage::Complete => Coverage::Complete,
                        opencode_storage::native::search::Coverage::Partial => Coverage::Partial,
                    };
                    (
                        coverage,
                        hits.into_iter()
                            .map(|h| Stored {
                                server: h.server,
                                session: h.session,
                                entity: h.entity,
                                kind: h.kind,
                                title: h.title,
                                body: h.body,
                                updated_ms: h.updated_ms,
                            })
                            .collect(),
                    )
                })
                .map_err(|e| e.to_string());
            inbox.lock().unwrap_or_else(|e| e.into_inner()).push(Landed { query: asked.clone(), hits });
            if let Some(w) = &wake {
                w();
            }
        });
    }));
}

/// The browser keeps no search tables.
#[cfg(target_arch = "wasm32")]
fn fulltext_ask(host: &dyn FulltextHost, which: Surface, query: String) {
    host.fulltext()
        .get(which)
        .inbox
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(Landed { query, hits: Err("no search tables in the browser".into()) });
}

/// Ranks what storage found for `query`: the rows of root sessions the surface lists, with a
/// session's title as the list has it now, each with its line of context.
fn fulltext_rank(host: &dyn FulltextHost, which: Surface, query: &str, stored: Vec<Stored>) -> Vec<Row> {
    let server = host.active_server_key();
    let archived = which == Surface::Palette && host.palette_archived();
    let home_project = (which == Surface::Home).then(|| host.home_selected_id()).flatten();
    let here = host
        .active_session_id()
        .and_then(|id| host.session(id))
        .and_then(|s| host.project_for_session(s))
        .map(|p| p.id.clone());
    let mut hits: Vec<fulltext::Hit> = Vec::new();
    // Each hit's row in what storage returned.
    let mut kept: Vec<(usize, Option<SessionInfo>)> = Vec::new();
    for (i, h) in stored.iter().enumerate() {
        if h.server != server {
            continue;
        }
        // A session the list doesn't have is shown as its tab saved it, or with storage's title.
        let listed = host.search_session(host.root_of(&h.session));
        let fallback = listed.is_none().then(|| fulltext_fallback(host, h)).flatten();
        let Some(info) = listed.or(fallback.as_ref()) else { continue };
        let project = host.project_for_session(info).map(|p| p.id.as_str());
        if info.archived.is_some() != archived || home_project.as_deref().is_some_and(|p| Some(p) != project) {
            continue;
        }
        let title = if h.title.is_empty() { info.title.clone() } else { h.title.clone() };
        hits.push(fulltext::Hit {
            session: info.id.clone(),
            entity: Some(h.entity.clone()),
            kind: kind(&h.kind),
            title,
            body: plain(&h.body),
            updated_ms: h.updated_ms,
            active_project: project.is_some() && project == home_project.as_deref().or(here.as_deref()),
        });
        kept.push((i, fallback));
    }
    fulltext::rank(&hits, query, host.now_ms(), 1)
        .into_iter()
        .filter_map(|r| {
            let hit = &hits[r.hit];
            let snippet = fulltext::snippet(&hit.body, query, SNIPPET)?;
            let (at, fallback) = &kept[r.hit];
            let message = hit.entity.clone().filter(|_| matches!(stored[*at].kind.as_str(), "user" | "assistant"));
            Some(Row { session: hit.session.clone(), kind: hit.kind, message, snippet, more: r.more, fallback: fallback.clone() })
        })
        .take(ROWS)
        .collect()
}

/// Stored row `h`'s session when the list doesn't have it: as a tab saved it (its title and
/// folder), else with storage's title. None with neither.
fn fulltext_fallback(host: &dyn FulltextHost, h: &Stored) -> Option<SessionInfo> {
    let tab = host.tab_title_and_dir(&h.session).filter(|(title, _)| !title.is_empty());
    let (title, directory) = match tab {
        Some((t, d)) => (t.to_owned(), d.to_owned()),
        None if !h.title.is_empty() => (h.title.clone(), String::new()),
        None => return None,
    };
    Some(SessionInfo { id: h.session.clone(), title, directory, updated: h.updated_ms, ..SessionInfo::default() })
}

/// `which`'s full text rows now (none while it shows nothing or is off).
pub fn fulltext_rows(host: &dyn FulltextHost, which: Surface) -> &[Row] {
    host.fulltext().rows(which)
}

/// The message to open session `id` at from `which`'s results for `query`: the one that
/// matched, when only its messages did (its title doesn't hold the query).
pub fn found_message(host: &dyn FulltextHost, which: Surface, id: &str, query: &str) -> Option<String> {
    found_message_in_rows(fulltext_rows(host, which), host.session(id), id, query)
}

/// Resolves the matching message ID for `id` from `rows` when `session`'s title does not contain `query`.
pub fn found_message_in_rows(rows: &[Row], session: Option<&SessionInfo>, id: &str, query: &str) -> Option<String> {
    let q = query.trim();
    let fold = SearchFold::for_query(q);
    let row = rows.iter().find(|r| r.session == id)?;
    let titled = session.is_some_and(|s| fold.fold(&s.title).contains(&fold.fold(q)));
    row.message.clone().filter(|_| !titled)
}

/// The row for `id` among `which`'s full text rows.
pub fn fulltext_row(host: &dyn FulltextHost, which: Surface, id: &str) -> Option<(Arc<Vec<Row>>, usize)> {
    host.fulltext().row(which, id)
}
