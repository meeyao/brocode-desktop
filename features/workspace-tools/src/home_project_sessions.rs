//! Scoped session fetch for Home's selected project (`packages/app/src/home/sessions/queries.ts:44-96`,
//! `projectSessionsQuery`): quiet projects can be missing from the server-wide `/api/session?limit=120`
//! index, so selecting a project on Home queries each checkout directory directly and merges the
//! returned root sessions into the Home list.

use opencode_kit::client::Server;
use opencode_kit::model::{self, SessionInfo};
use opencode_search::query::percent_encode;
use rune::machine::{Attempt, Effects, Seq};
use rune::{Latest, Waker};

use crate::home_search::HOME_SESSION_LIMIT;

/// Cache key identifying one server + selected project checkout set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub scope: String,
    pub directories: Vec<String>,
}

/// Explicit state machine for Home's selected-project session fetch.
#[derive(Clone, Debug, Default)]
pub enum State {
    #[default]
    Idle,
    Loading {
        target: Target,
        attempt: Attempt,
    },
    /// Keeping prior sessions visible while refetching after a reconnect, remount, or directory update.
    Refreshing {
        target: Target,
        attempt: Attempt,
        sessions: Vec<SessionInfo>,
    },
    Loaded {
        target: Target,
        sessions: Vec<SessionInfo>,
    },
    /// Server disconnected or Home unmounted while sessions were shown: keep showing them and refetch on return/reconnect.
    Stale {
        target: Target,
        sessions: Vec<SessionInfo>,
    },
    /// Fetch failed for `target`; retries automatically on reconnect or next Home visit.
    Failed {
        target: Target,
    },
}

pub enum Event {
    LeftHome,
    Synced { target: Option<Target>, connected: bool },
    Resolved { attempt: Attempt, sessions: Option<Vec<SessionInfo>> },
}

pub enum Effect {
    Fetch { attempt: Attempt, directories: Vec<String> },
    Cancel,
}

impl State {
    pub fn step(&mut self, seq: &mut Seq, event: Event) -> Effects<Effect> {
        match event {
            Event::LeftHome => match std::mem::take(self) {
                State::Loaded { target, sessions } | State::Stale { target, sessions } => {
                    *self = State::Stale { target, sessions };
                    Effects::none()
                }
                State::Refreshing { target, sessions, .. } => {
                    *self = State::Stale { target, sessions };
                    Effects::one(Effect::Cancel)
                }
                State::Loading { .. } => {
                    *self = State::Idle;
                    Effects::one(Effect::Cancel)
                }
                State::Failed { .. } | State::Idle => {
                    *self = State::Idle;
                    Effects::none()
                }
            },
            Event::Synced { target: None, .. } => {
                let had_in_flight = matches!(self, State::Loading { .. } | State::Refreshing { .. });
                *self = State::Idle;
                if had_in_flight { Effects::one(Effect::Cancel) } else { Effects::none() }
            }
            Event::Synced { target: Some(target), connected: false } => match std::mem::take(self) {
                State::Loaded { target: cur, sessions } | State::Stale { target: cur, sessions } if cur.scope == target.scope => {
                    *self = State::Stale { target: cur, sessions };
                    Effects::none()
                }
                State::Refreshing { target: cur, sessions, .. } if cur.scope == target.scope => {
                    *self = State::Stale { target: cur, sessions };
                    Effects::one(Effect::Cancel)
                }
                State::Loading { .. } | State::Refreshing { .. } => {
                    *self = State::Idle;
                    Effects::one(Effect::Cancel)
                }
                _ => {
                    *self = State::Idle;
                    Effects::none()
                }
            },
            Event::Synced { target: Some(target), connected: true } => {
                if target.directories.is_empty() {
                    let had_in_flight = matches!(self, State::Loading { .. } | State::Refreshing { .. });
                    *self = State::Idle;
                    return if had_in_flight { Effects::one(Effect::Cancel) } else { Effects::none() };
                }
                match std::mem::take(self) {
                    cur @ (State::Loading { .. } | State::Refreshing { .. } | State::Loaded { .. } | State::Failed { .. })
                        if cur.target() == Some(&target) =>
                    {
                        *self = cur;
                        Effects::none()
                    }
                    State::Stale { target: cur, sessions }
                    | State::Loaded { target: cur, sessions }
                    | State::Refreshing { target: cur, sessions, .. }
                        if cur.scope == target.scope =>
                    {
                        let attempt = seq.next();
                        let dirs = target.directories.clone();
                        *self = State::Refreshing { target, attempt, sessions };
                        Effects::one(Effect::Fetch { attempt, directories: dirs })
                    }
                    _ => {
                        let attempt = seq.next();
                        let dirs = target.directories.clone();
                        *self = State::Loading { target, attempt };
                        Effects::one(Effect::Fetch { attempt, directories: dirs })
                    }
                }
            }
            Event::Resolved { attempt, sessions } => match std::mem::take(self) {
                State::Loading { target, attempt: current } if current == attempt => {
                    *self = match sessions {
                        Some(sessions) => State::Loaded { target, sessions },
                        None => State::Failed { target },
                    };
                    Effects::none()
                }
                State::Refreshing { target, attempt: current, sessions: prev } if current == attempt => {
                    *self = match sessions {
                        Some(sessions) => State::Loaded { target, sessions },
                        None => State::Stale { target, sessions: prev },
                    };
                    Effects::none()
                }
                other => {
                    *self = other;
                    Effects::none()
                }
            },
        }
    }

    fn target(&self) -> Option<&Target> {
        match self {
            State::Loading { target, .. }
            | State::Refreshing { target, .. }
            | State::Loaded { target, .. }
            | State::Stale { target, .. }
            | State::Failed { target } => Some(target),
            State::Idle => None,
        }
    }

    pub fn sessions_for(&self, scope: &str) -> &[SessionInfo] {
        match self {
            State::Loaded { target, sessions }
            | State::Stale { target, sessions }
            | State::Refreshing { target, sessions, .. }
                if target.scope == scope =>
            {
                sessions
            }
            _ => &[],
        }
    }
}

/// Worker wrapper driving [`State`] off the UI thread.
pub struct ProjectSessions {
    state: State,
    seq: Seq,
    fetch: Latest<(Attempt, Option<Vec<SessionInfo>>)>,
}

impl Default for ProjectSessions {
    fn default() -> Self {
        Self { state: State::Idle, seq: Seq::default(), fetch: Latest::default() }
    }
}

impl ProjectSessions {
    pub fn set_waker(&mut self, waker: Waker) {
        self.fetch = Latest::new(waker);
    }

    pub fn sessions_for(&self, scope: &str) -> &[SessionInfo] {
        self.state.sessions_for(scope)
    }

    pub fn session(&self, id: &str) -> Option<&SessionInfo> {
        match &self.state {
            State::Loaded { sessions, .. } | State::Stale { sessions, .. } | State::Refreshing { sessions, .. } => {
                sessions.iter().find(|s| s.id == id)
            }
            _ => None,
        }
    }

    /// Cancels in-flight work and marks cached sessions stale when leaving Home (`refetchOnMount`).
    pub fn leave_home(&mut self) {
        for effect in self.state.step(&mut self.seq, Event::LeftHome) {
            if matches!(effect, Effect::Cancel) {
                self.fetch.cancel();
            }
        }
    }

    /// Polls in-flight results and syncs the target to the currently selected Home project.
    /// Returns `true` when loaded sessions changed.
    pub fn poll(&mut self, target: Option<Target>, connected: bool, server: Option<Server>) -> bool {
        let mut changed = false;
        if let Some((attempt, sessions)) = self.fetch.take() {
            let before_len = self.current_len();
            self.state.step(&mut self.seq, Event::Resolved { attempt, sessions });
            let after_len = self.current_len();
            changed = before_len != after_len || after_len.is_some_and(|n| n > 0);
        }
        let can_fetch = connected && server.is_some();
        let effects = self.state.step(&mut self.seq, Event::Synced { target, connected: can_fetch });
        for effect in effects {
            match effect {
                Effect::Cancel => self.fetch.cancel(),
                Effect::Fetch { attempt, directories } => {
                    let Some(server) = server.clone() else { continue };
                    self.fetch.request(directories, move |dirs| {
                        let mut out = Vec::new();
                        let mut any_ok = false;
                        for dir in dirs {
                            let path = format!(
                                "/api/session?directory={}&parentID=null&limit={HOME_SESSION_LIMIT}&order=desc",
                                percent_encode(&dir),
                            );
                            if let Ok(v) = server.get(&path) {
                                any_ok = true;
                                out.extend(model::parse_sessions(&v).into_iter().filter(|s| s.parent.is_none() && s.archived.is_none()));
                            }
                        }
                        (attempt, any_ok.then_some(out))
                    });
                }
            }
        }
        changed
    }

    fn current_len(&self) -> Option<usize> {
        match &self.state {
            State::Loaded { sessions, .. } | State::Stale { sessions, .. } | State::Refreshing { sessions, .. } => Some(sessions.len()),
            _ => None,
        }
    }
}
