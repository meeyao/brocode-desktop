//! Where the outbox keeps its prompts: one row each in `opencode.sqlite` (the storage crate's
//! Tier 0, committed with a full sync), written and read on the storage thread. The UI thread
//! only hands over what changed; nothing here touches a file, and the web build backs the same
//! calls with IndexedDB.
//!
//! A hand-off's write travels with its row: the storage thread gives it to the window's writer
//! the moment the row is durable, so the network never waits on the UI thread and never sees a
//! prompt the next launch wouldn't check for. What comes back (a read, a row that couldn't be
//! saved, a writer that was gone) waits in the server's mailbox for the next window that polls
//! that server.

use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, Ordering};

use opencode_kit::client::Write;
use opencode_storage::{self as storage, Storage, StorageOp};
use rune::Waker;

use super::{Log, Sent, row};

/// Where the outbox's prompts are kept.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) enum Store {
    /// In memory only: a fixture, a story, a run that doesn't save, or storage that couldn't open.
    #[default]
    Memory,
    /// Server `server`'s rows are being read. New prompts are written meanwhile; none is handed
    /// over until the older ones are in, so the outbox keeps their order.
    Reading { server: String },
    /// Read: every change lands in the database.
    Open { server: String },
}

impl Store {
    pub(super) fn server(&self) -> Option<&str> {
        match self {
            Store::Memory => None,
            Store::Reading { server } | Store::Open { server } => Some(server),
        }
    }
}

/// Whether a prompt has a row in the database.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Row {
    /// None yet (a fixture's prompt, an outbox in memory, or one whose first row failed).
    #[default]
    None,
    /// Written, or on its way to the storage thread: later steps change its columns.
    Written,
}

/// What the storage thread reports back to the UI thread.
pub(super) enum Mail {
    /// The server's stored prompts, oldest first; an error leaves the outbox in memory.
    Read(Result<Vec<Sent>, String>),
    /// A row couldn't be committed (`handoff`: the write that waited on it never went).
    NotSaved { id: String, handoff: Option<u32> },
    /// Hand-off `handoff`'s row is durable, but the window's writer was gone.
    NotSent { id: String, handoff: u32 },
}

/// How a hand-off's write waits for its row.
pub(super) enum Forward {
    /// It's on the write lane already, making its request; `gate` lets it post once the row is
    /// durable (or drops it).
    #[cfg(not(target_arch = "wasm32"))]
    Gate { gate: std::sync::mpsc::SyncSender<bool>, handoff: u32 },
    /// It goes to `writer` once the row is durable (a page has one thread: the lane can't wait).
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    After { writer: std::sync::mpsc::Sender<Write>, write: Box<Write>, handoff: u32 },
}

impl Forward {
    fn handoff(&self) -> u32 {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Forward::Gate { handoff, .. } => *handoff,
            Forward::After { handoff, .. } => *handoff,
        }
    }

    /// The row is durable (`true`) or couldn't be saved: the write goes, or never does.
    fn settle(self, saved: bool, to: &str, id: String) {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Forward::Gate { gate, .. } => {
                if gate.send(saved).is_err() {
                    // The writer lane dropped while waiting for the row commit.
                }
            }
            Forward::After { writer, write, handoff } => {
                if saved && writer.send(*write).is_err() {
                    post(to, Mail::NotSent { id, handoff });
                }
            }
        }
    }
}

#[derive(Default)]
struct Mailbox {
    mail: Vec<(String, Mail)>,
    /// The windows of each server, woken when its mail comes.
    wakers: Vec<(String, Waker)>,
}

static MAILBOX: Mutex<Mailbox> = Mutex::new(Mailbox { mail: Vec::new(), wakers: Vec::new() });

fn mailbox() -> std::sync::MutexGuard<'static, Mailbox> {
    MAILBOX.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn post(server: &str, mail: Mail) {
    let wake: Vec<Waker> = {
        let mut m = mailbox();
        m.mail.push((server.to_owned(), mail));
        m.wakers.iter().filter(|(s, _)| s == server).map(|(_, w)| w.clone()).collect()
    };
    wake.iter().for_each(|w| w());
}

/// Window `waker` polls `server`'s outbox: it's woken when mail for it comes.
pub(super) fn listen(server: &str, waker: Option<Waker>) {
    let Some(waker) = waker else { return };
    let mut m = mailbox();
    if !m.wakers.iter().any(|(s, w)| s == server && std::sync::Arc::ptr_eq(w, &waker)) {
        m.wakers.push((server.to_owned(), waker));
    }
}

/// The mail waiting for `server`, oldest first.
pub(super) fn take(server: &str) -> Vec<Mail> {
    let mut m = mailbox();
    if !m.mail.iter().any(|(s, _)| s == server) {
        return Vec::new();
    }
    let (mine, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut m.mail).into_iter().partition(|(s, _)| s == server);
    m.mail = rest;
    mine.into_iter().map(|(_, mail)| mail).collect()
}

/// Builds a storage-thread read of server `base`'s prompts, taking in the rows an import left
/// under the file hash of its journal (`outbox-<sha256>.json`: this server's, or an older build's
/// per-window one, `window`'s).
pub(super) fn read_op(base: &str, window: &str) -> StorageOp {
    let adopt = [format!("server\n{base}"), format!("{window}\n{base}")]
        .map(|scope| format!("sha256:{}", opencode_storage::chunk::blob_id(scope.as_bytes())))
        .to_vec();
    let server = base.to_owned();
    Box::new(move |st: &Storage| {
        st.outbox().read(
            &server.clone(),
            adopt,
            Box::new(move |rows| {
                let rows = rows.map(|rows| rows.into_iter().map(row::sent).collect()).map_err(|e| e.to_string());
                post(&server, Mail::Read(rows));
            }),
        );
    })
}

/// Orders a server's prompts across launches: later than any stored before, with headroom for
/// queue-first prompts inserted ahead of waiting entries.
pub(super) fn next_seq() -> i64 {
    static LAST: AtomicI64 = AtomicI64::new(0);
    let now = opencode_platform::wall_ms().saturating_mul(1000);
    let mut last = LAST.load(Ordering::Relaxed);
    loop {
        let next = now.max(last.saturating_add(1024));
        match LAST.compare_exchange_weak(last, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return next,
            Err(seen) => last = seen,
        }
    }
}

impl Log {
    /// Builds the storage operation to land prompt `id` as it is now: its first row (with its
    /// bytes), the columns a step changed, or its removal once the server has it. `forward` goes
    /// to its writer once the row is durable. In memory, returns `None`.
    pub(super) fn persist_op(&mut self, id: &str, forward: Option<Forward>) -> Option<StorageOp> {
        let server = self.store.server().map(str::to_owned)?;
        let s = self.sent.iter_mut().find(|s| s.id == id)?;
        if !s.journaled {
            return None;
        }
        if s.phase == super::Phase::Accepted {
            if std::mem::take(&mut s.row) == Row::Written {
                let id = id.to_owned();
                return Some(Box::new(move |st: &Storage| st.outbox().delete(&server, &id, None)));
            }
            return None;
        }
        let first = s.row == Row::None;
        s.row = Row::Written;
        let handoff = forward.as_ref().map(Forward::handoff);
        let (to, row_id) = (server.clone(), id.to_owned());
        let done: opencode_storage::Done = Box::new(move |result| match (result, forward) {
            (Ok(()), Some(f)) => {
                rune::marks::mark("outbox.committed");
                f.settle(true, &to, row_id);
            }
            (Ok(()), None) => {}
            (Err(e), forward) => {
                eprintln!("brocode: outbox row {row_id} not saved: {e}");
                if let Some(f) = forward {
                    f.settle(false, &to, row_id.clone());
                }
                // A later step's row failing leaves the last durable one, which still holds it.
                if first || handoff.is_some() {
                    post(&to, Mail::NotSaved { id: row_id, handoff });
                }
            }
        });
        rune::marks::mark("outbox.put");
        if first {
            let build = {
                let snapshot = s.clone();
                // Its draft goes with its first row only.
                s.left_draft = super::LeftDraft::None;
                let (server, s) = (server.clone(), snapshot);
                Box::new(move || row::build(server, s)) as opencode_storage::OutboxBuild
            };
            return Some(Box::new(move |st: &Storage| st.outbox().put(build, Some(done))));
        }
        let update = row::update(s);
        let id = id.to_owned();
        Some(Box::new(move |st: &Storage| st.outbox().update(&server, &id, update, Some(done))))
    }

    /// Lands prompt `id` using the configured process-wide storage runner.
    pub(super) fn persist(&mut self, id: &str, forward: Option<Forward>) {
        if let Some(op) = self.persist_op(id, forward) {
            storage::with_storage(op);
        }
    }

    /// Builds the storage delete operation when prompt `s` leaves the outbox.
    pub(super) fn forget_op(&mut self, s: &mut Sent) -> Option<StorageOp> {
        let server = self.store.server().map(str::to_owned)?;
        if std::mem::take(&mut s.row) == Row::Written {
            let id = s.id.clone();
            return Some(Box::new(move |st: &Storage| st.outbox().delete(&server, &id, None)));
        }
        None
    }

    /// Prompt `s` left the outbox (sent elsewhere, edited back, deleted): its row goes.
    pub(super) fn forget(&mut self, s: &mut Sent) {
        if let Some(op) = self.forget_op(s) {
            storage::with_storage(op);
        }
    }
}
