//! The searches behind an open `@` list, Desktop's `transitionMentionSearch`
//! (`packages/app/src/composer/editor/mention-search-machine.ts`): the server's file search
//! for the query, 100 ms after the last keystroke, and its session search for the query's
//! first word, at once. Each carries the attempt it started with, so an answer to an earlier
//! query, or one that comes after the list closed, changes nothing; one that never comes
//! settles empty after 10 s. While the next file search waits or runs, the last one's rows
//! stay listed, so the list keeps its height, but they can't be highlighted or picked.
//!
//! The directory's subagents and skills ([`Catalog`]) are read once per directory, as
//! Desktop's location data is, and again on an opening after a failed read.

use rune::machine::{Attempt, Effects, Seq};

/// `MENTION_FILE_DEBOUNCE_MS`.
pub const FILE_DEBOUNCE: f64 = 0.100;
/// `MENTION_SEARCH_TIMEOUT_MS`.
pub const TIMEOUT: f64 = 10.0;

/// A file search that answered: its query and paths (none when it failed or timed out).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Found {
    pub query: String,
    pub paths: Vec<String>,
}

/// `MentionFileSearchState`.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Files {
    #[default]
    Idle,
    /// Waiting out the typing; `last` stays listed meanwhile.
    Debouncing { attempt: Attempt, due: f64, last: Option<Found> },
    /// Asked, and given up on at `due`.
    Loading { attempt: Attempt, due: f64, last: Option<Found> },
    Settled(Found),
}

/// `MentionSessionSearchState`: the server narrows sessions by the query's first word.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Sessions {
    /// No server to ask (the fixture).
    #[default]
    Disabled,
    Loading { attempt: Attempt, word: String, due: f64 },
    Settled { word: String },
}

/// `MentionSearchState`.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Search {
    /// No list; closing drops every search, so reopening on the same word searches again.
    #[default]
    Closed,
    Active { query: String, files: Files, sessions: Sessions },
}

pub enum Event<'a> {
    /// The open list's query (`context.changed`); `server`: there's a server to search.
    Changed { query: &'a str, server: bool, now: f64 },
    Closed,
    /// A timer came due: a debounce or a timeout.
    Due { now: f64 },
    /// A file search answered, `None` when it failed.
    Files { attempt: Attempt, paths: Option<Vec<String>> },
    /// A session search answered (failed or not: the titles known still match).
    Sessions { attempt: Attempt },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    SearchFiles { attempt: Attempt, query: String },
    SearchSessions { attempt: Attempt, word: String },
    WakeAt(f64),
}

/// The server narrows sessions by the query's lowercased first word (`mentionSearchWord`).
pub fn word(query: &str) -> String {
    query.to_lowercase().split(' ').next().unwrap_or_default().to_owned()
}

pub fn step(seq: &mut Seq, state: &mut Search, event: Event<'_>) -> Effects<Effect> {
    match event {
        Event::Changed { query, server, now } => changed(seq, state, query, server, now),
        Event::Closed => {
            *state = Search::Closed;
            Effects::none()
        }
        Event::Due { now } => due(state, now),
        Event::Files { attempt, paths } => {
            let Search::Active { query, files, .. } = state else { return Effects::none() };
            if !matches!(files, Files::Loading { attempt: a, .. } if *a == attempt) {
                return Effects::none();
            }
            *files = Files::Settled(Found { query: query.clone(), paths: paths.unwrap_or_default() });
            Effects::none()
        }
        Event::Sessions { attempt } => {
            let Search::Active { sessions, .. } = state else { return Effects::none() };
            if let Sessions::Loading { attempt: a, word, .. } = sessions
                && *a == attempt
            {
                *sessions = Sessions::Settled { word: std::mem::take(word) };
            }
            Effects::none()
        }
    }
}

fn changed(seq: &mut Seq, state: &mut Search, query: &str, server: bool, now: f64) -> Effects<Effect> {
    let (files, sessions) = match std::mem::take(state) {
        Search::Active { query: was, files, sessions } => {
            if was == query && !matches!(sessions, Sessions::Disabled) == server {
                *state = Search::Active { query: was, files, sessions };
                return Effects::none();
            }
            let files = if was == query { files } else { next_files(seq, files, query, now) };
            (files, Some(sessions))
        }
        Search::Closed => (next_files(seq, Files::Idle, query, now), None),
    };
    let mut effects = Effects::none();
    let word = word(query);
    let sessions = match sessions {
        Some(Sessions::Loading { attempt, word: w, due }) if server && w == word => Sessions::Loading { attempt, word: w, due },
        Some(Sessions::Settled { word: w }) if server && w == word => Sessions::Settled { word: w },
        _ if server => {
            let attempt = seq.next();
            effects.push(Effect::SearchSessions { attempt, word: word.clone() });
            effects.push(Effect::WakeAt(now + TIMEOUT));
            Sessions::Loading { attempt, word, due: now + TIMEOUT }
        }
        _ => Sessions::Disabled,
    };
    let files = if server { files } else { Files::Idle };
    if let Files::Debouncing { due, .. } = &files {
        effects.push(Effect::WakeAt(*due));
    }
    *state = Search::Active { query: query.to_owned(), files, sessions };
    effects
}

/// Only paths are searched: an empty query lists no files, and a space means prose or a
/// session title.
fn next_files(seq: &mut Seq, previous: Files, query: &str, now: f64) -> Files {
    if query.trim().is_empty() || query.chars().any(char::is_whitespace) {
        return Files::Idle;
    }
    let last = match previous {
        Files::Idle => None,
        Files::Debouncing { last, .. } | Files::Loading { last, .. } => last,
        Files::Settled(found) => Some(found),
    };
    Files::Debouncing { attempt: seq.next(), due: now + FILE_DEBOUNCE, last }
}

fn due(state: &mut Search, now: f64) -> Effects<Effect> {
    let Search::Active { query, files, sessions } = state else { return Effects::none() };
    let mut effects = Effects::none();
    let late = |due: f64| now + 1e-9 >= due;
    if let Sessions::Loading { word, due, .. } = sessions
        && late(*due)
    {
        *sessions = Sessions::Settled { word: std::mem::take(word) };
    }
    match files {
        Files::Debouncing { attempt, due, last } if late(*due) => {
            let (attempt, last) = (*attempt, last.take());
            *files = Files::Loading { attempt, due: now + TIMEOUT, last };
            effects.push(Effect::SearchFiles { attempt, query: query.clone() });
            effects.push(Effect::WakeAt(now + TIMEOUT));
        }
        Files::Loading { due, .. } if late(*due) => *files = Files::Settled(Found { query: query.clone(), paths: Vec::new() }),
        _ => {}
    }
    effects
}

impl Search {
    /// The file rows to list: this query's, or the last one's while the next search runs.
    pub fn files(&self) -> &[String] {
        let Search::Active { files, .. } = self else { return &[] };
        match files {
            Files::Idle => &[],
            Files::Debouncing { last, .. } | Files::Loading { last, .. } => last.as_ref().map_or(&[], |f| &f.paths),
            Files::Settled(found) => &found.paths,
        }
    }

    /// The query the listed file rows answer.
    pub fn files_query(&self) -> &str {
        let Search::Active { files, .. } = self else { return "" };
        match files {
            Files::Idle => "",
            Files::Debouncing { last, .. } | Files::Loading { last, .. } => last.as_ref().map_or("", |f| &f.query),
            Files::Settled(found) => &found.query,
        }
    }

    /// Whether the listed file rows answer an earlier query, so the keys can't highlight or
    /// pick them (`isMentionFileStale`).
    pub fn files_stale(&self, query: &str) -> bool {
        let Search::Active { files, .. } = self else { return true };
        match files {
            Files::Idle => true,
            Files::Debouncing { last, .. } | Files::Loading { last, .. } => last.as_ref().is_none_or(|f| f.query != query),
            Files::Settled(found) => found.query != query,
        }
    }

    /// Whether the session search for `query` has yet to settle, so the list mustn't close as
    /// empty yet (`selectMentionSessionsPending`).
    pub fn sessions_pending(&self, query: &str, server: bool) -> bool {
        if !server {
            return false;
        }
        let Search::Active { sessions, .. } = self else { return true };
        match sessions {
            Sessions::Disabled => false,
            Sessions::Loading { .. } => true,
            Sessions::Settled { word: w } => *w != word(query),
        }
    }

    /// The next timer the search waits on.
    pub fn next_due(&self) -> Option<f64> {
        let Search::Active { files, sessions, .. } = self else { return None };
        let files = match files {
            Files::Debouncing { due, .. } | Files::Loading { due, .. } => Some(*due),
            _ => None,
        };
        let sessions = match sessions {
            Sessions::Loading { due, .. } => Some(*due),
            _ => None,
        };
        files.into_iter().chain(sessions).reduce(f64::min)
    }
}

/// The directory's subagents and skills, read once per directory.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Catalog {
    #[default]
    Unread,
    Reading { directory: String, attempt: Attempt },
    Read { directory: String },
    /// A read failed: the list's next opening asks again.
    Failed { directory: String },
}

pub enum CatalogEvent<'a> {
    /// The list is open over `directory`; `opened`: it just opened.
    Wanted { directory: &'a str, opened: bool },
    Answered { attempt: Attempt, failed: bool },
    /// `agent.updated` or `skill.updated` arrived for `directory` (`None` = locationless
    /// deferred plugin registration, `packages/client/src/solid/data.ts:1522-1540`, `opencode#66813`).
    Invalidated { directory: Option<&'a str> },
}

pub fn step_catalog(seq: &mut Seq, state: &mut Catalog, event: CatalogEvent<'_>) -> Option<(Attempt, String)> {
    match event {
        CatalogEvent::Wanted { directory, opened } => {
            let ask = match state {
                Catalog::Unread => true,
                Catalog::Reading { directory: d, .. } | Catalog::Read { directory: d } => d != directory,
                Catalog::Failed { directory: d } => d != directory || opened,
            };
            if !ask {
                return None;
            }
            let attempt = seq.next();
            *state = Catalog::Reading { directory: directory.to_owned(), attempt };
            Some((attempt, directory.to_owned()))
        }
        CatalogEvent::Answered { attempt, failed } => {
            if let Catalog::Reading { directory, attempt: a } = state
                && *a == attempt
            {
                let directory = std::mem::take(directory);
                *state = if failed { Catalog::Failed { directory } } else { Catalog::Read { directory } };
            }
            None
        }
        CatalogEvent::Invalidated { directory } => {
            let stale = match state {
                Catalog::Unread => false,
                Catalog::Reading { directory: d, .. } | Catalog::Read { directory: d } | Catalog::Failed { directory: d } => {
                    directory.is_none() || directory == Some(d.as_str())
                }
            };
            if stale {
                let _ = seq.next();
                *state = Catalog::Unread;
            }
            None
        }
    }
}
