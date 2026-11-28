//! Reply-path checks are reads: discovering a path during a view starts work, but isn't a
//! reason to run the view again. Only an answer that turns plain code into a link is one.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::mpsc::{Receiver, Sender, channel};

use opencode_kit::client::Server;
use rune::Waker;
use rune::time::Instant;

use super::{FOUND_FOR, MISSING_FOR, reply_path_exists};

enum Check {
    Checking { attempt: u64, was_link: bool },
    Answer { exists: bool, at: Instant },
}

/// Keep checks for the bounded working set. Check results are hints, not authorization to
/// open a path: a click resolves the link again on the current server.
const MAX_SESSIONS: usize = 32;
const MAX_CODES: usize = 1024;
const MAX_KEY_BYTES: usize = 256 * 1024;

struct Paths {
    base: String,
    directory: String,
    home: String,
    entries: HashMap<String, Check>,
    bytes: usize,
    used: u64,
    rev: u64,
}

impl Paths {
    fn new(base: &str, directory: &str, home: &str, serial: u64) -> Self {
        Self {
            base: base.into(),
            directory: directory.into(),
            home: home.into(),
            entries: HashMap::new(),
            bytes: 0,
            used: serial,
            rev: serial,
        }
    }

    fn expire(&mut self, now: Instant) {
        self.entries.retain(|key, c| {
            // A retained row may still be drawing a link. Keep its last positive answer
            // until revalidation says the link is gone, including when this cache is full.
            let expired = matches!(c, Check::Answer { exists: false, at } if now.duration_since(*at) >= MISSING_FOR);
            if expired {
                self.bytes -= key.len();
            }
            !expired
        });
    }
}

struct Answer {
    sid: String,
    attempt: u64,
    entries: Vec<(String, bool)>,
}

pub(crate) struct Replies {
    sessions: RefCell<HashMap<String, Paths>>,
    attempt: Cell<u64>,
    out: Sender<Answer>,
    answers: Receiver<Answer>,
}

impl Default for Replies {
    fn default() -> Self {
        let (out, answers) = channel();
        Self { sessions: Default::default(), attempt: Cell::new(0), out, answers }
    }
}

impl Replies {
    pub(crate) fn retain(&self, keep: impl Fn(&str) -> bool) {
        self.sessions.borrow_mut().retain(|sid, _| keep(sid));
    }

    pub(crate) fn version(&self, sid: &str) -> u64 {
        self.sessions.borrow().get(sid).map_or(0, |p| p.rev)
    }

    pub(super) fn read(
        &self,
        sid: &str,
        codes: Vec<String>,
        server: &Server,
        directory: impl FnOnce() -> String,
        home: String,
        wake: Option<&Waker>,
    ) -> Option<Rc<HashSet<String>>> {
        let now = Instant::now();
        let attempt = self.attempt.get().wrapping_add(1);
        self.attempt.set(attempt);
        let mut ask = Vec::new();
        let mut found = HashSet::new();
        let mut sessions = self.sessions.borrow_mut();
        let directory = directory();
        if sessions.get(sid).is_none_or(|p| p.base != server.base || p.directory != directory || p.home != home) {
            if sessions.len() >= MAX_SESSIONS
                && !sessions.contains_key(sid)
                && let Some(old) = sessions.iter().min_by_key(|(_, p)| p.used).map(|(k, _)| k.clone())
            {
                sessions.remove(&old);
            }
            sessions.insert(sid.to_owned(), Paths::new(&server.base, &directory, &home, attempt));
        }
        let paths = sessions.get_mut(sid)?;
        paths.used = attempt;
        if paths.entries.len() >= MAX_CODES || paths.bytes >= MAX_KEY_BYTES {
            paths.expire(now);
        }
        for code in codes {
            match paths.entries.get(&code) {
                Some(Check::Checking { was_link, .. }) => {
                    if *was_link {
                        found.insert(code);
                    }
                }
                Some(Check::Answer { exists, at })
                    if now.duration_since(*at) < if *exists { FOUND_FOR } else { MISSING_FOR } =>
                {
                    if *exists {
                        found.insert(code);
                    }
                }
                stale => {
                    let was_link = matches!(stale, Some(Check::Answer { exists: true, .. }));
                    if was_link {
                        found.insert(code.clone());
                    }
                    if stale.is_none() {
                        if paths.entries.len() >= MAX_CODES || code.len() > MAX_KEY_BYTES - paths.bytes {
                            continue;
                        }
                        paths.bytes += code.len();
                    }
                    paths.entries.insert(code.clone(), Check::Checking { attempt, was_link });
                    ask.push(code);
                }
            }
        }
        if !ask.is_empty() {
            let (out, wake, server, session) = (self.out.clone(), wake.cloned(), server.clone(), sid.to_owned());
            let started = rune::task::spawn("reply-code-links", move || {
                let entries = ask
                    .into_iter()
                    .map(|code| {
                        let exists = reply_path_exists(&server, &directory, &home, &code);
                        (code, exists)
                    })
                    .collect();
                if out.send(Answer { sid: session, attempt, entries }).is_ok()
                    && let Some(wake) = wake
                {
                    wake();
                }
            });
            if started.is_err() {
                paths.entries.retain(|key, c| {
                    let failed = matches!(c, Check::Checking { attempt: a, .. } if *a == attempt);
                    if failed && matches!(c, Check::Checking { was_link: true, .. }) {
                        *c = Check::Answer { exists: true, at: now };
                        return true;
                    }
                    if failed {
                        paths.bytes -= key.len();
                    }
                    !failed
                });
            }
        }
        (!found.is_empty()).then(|| Rc::new(found))
    }

    pub(crate) fn land(&self, shown: Option<&str>) -> bool {
        let mut changed = false;
        for answer in self.answers.try_iter() {
            let mut sessions = self.sessions.borrow_mut();
            let Some(paths) = sessions.get_mut(&answer.sid) else { continue };
            let mut links = false;
            let now = Instant::now();
            for (code, exists) in answer.entries {
                let Some(state) = paths.entries.get_mut(&code) else { continue };
                let Check::Checking { attempt, was_link } = state else { continue };
                if *attempt != answer.attempt {
                    continue;
                }
                // A recheck never clears the old link while the answer is on its way. Only
                // a different answer changes what a row draws.
                links |= exists != *was_link;
                *state = Check::Answer { exists, at: now };
            }
            if links {
                paths.rev = paths.rev.wrapping_add(1);
                changed |= shown == Some(&answer.sid);
            }
        }
        changed
    }
}
