//! The native backend: one SQLite database (`rusqlite`, bundled SQLite), opened once per process.
//! [`crate::Storage::open`] opens it on the caller's thread and reads what the first frame
//! draws; the connection then waits, untouched, until the first write starts the
//! `opencode-storage` thread, which owns it from then on.

mod boot;
mod dbfile;
pub(crate) mod lease;
pub mod boot_record;
pub mod import;
pub mod journal;
mod schema;
pub mod search;
pub mod seed;
pub(crate) mod sql;
mod writer;

use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

pub use boot::{BootReport, Opening};
pub use import::{ImportConfig, ImportGroup};
pub use schema::SCHEMA_VERSION;

use crate::{BootPacket, Cmd, FlushReason, FlushReport, Loaded, Storage};

/// Where the database is and what the open needs. Only this type names paths.
pub struct StorageConfig {
    /// `opencode.sqlite`, in the app's data folder.
    pub(crate) path: PathBuf,
    /// The folder the files Rune kept before the database live in (`%APPDATA%\brocode`);
    /// none skips the import.
    pub(crate) import: Option<ImportConfig>,
    /// Desktop's `userData` folders to seed a fresh profile from, newest data wins.
    pub(crate) official: Vec<PathBuf>,
    /// Wall-clock milliseconds since the epoch, for `updated_at` (injected: the workspace forbids
    /// `SystemTime::now` outside its config).
    pub(crate) clock: fn() -> i64,
    /// Cache rows read on the open's thread as soon as SQLite is open ([`StorageConfig::early_cache`]).
    pub(crate) early: Vec<EarlyCache>,
    /// SQLite's open waits for [`Opening::proceed`] (the single-instance claim).
    pub(crate) after_claim: bool,
    /// Called on the open's thread as it hands the boot packet over (a launch trace's mark).
    pub(crate) on_packet: Option<fn()>,
}

/// A `cache` row wanted before the first frame, off the UI thread.
pub(crate) struct EarlyCache {
    pub(crate) key: &'static str,
    pub(crate) version: i64,
    pub(crate) offer: Box<dyn FnOnce(Arc<[u8]>) + Send>,
}

impl StorageConfig {
    pub fn native(path: impl Into<PathBuf>, clock: fn() -> i64) -> StorageConfig {
        StorageConfig { path: path.into(), import: None, official: Vec::new(), clock, early: Vec::new(), after_claim: false, on_packet: None }
    }

    /// The record's raw read starts at once; SQLite's open, which takes the file's exclusive
    /// lock, waits for [`Opening::proceed`]: for an app that only knows after starting whether it
    /// is the one instance (a second launch hands off and ends, and must not take the lock).
    pub fn after_claim(mut self) -> StorageConfig {
        self.after_claim = true;
        self
    }

    /// `sent` runs on the open's thread as the boot packet goes to the UI: for a launch trace.
    pub fn on_packet(mut self, sent: fn()) -> StorageConfig {
        self.on_packet = Some(sent);
        self
    }

    /// Reads `cache` row `key` at `version` on the open's thread right after SQLite opens (once
    /// the boot packet is out, when the open owes it), and hands its bytes to `offer` there: for
    /// a cache the first frame uses if it's in time (Rune's glyph masks) and does without if not.
    /// No row, another version, or an open that fails: no call. Never on the UI thread, never in
    /// the boot record.
    pub fn early_cache(mut self, key: &'static str, version: i64, offer: impl FnOnce(Arc<[u8]>) + Send + 'static) -> StorageConfig {
        self.early.push(EarlyCache { key, version, offer: Box::new(offer) });
        self
    }

    /// Imports the files Rune kept before the database, once, from `import.dir`.
    pub fn import(mut self, import: ImportConfig) -> StorageConfig {
        self.import = Some(import);
        self
    }

    /// The app's database ([`crate::AppStorage`]); none without a data folder (state in memory).
    pub fn app(app: crate::AppStorage) -> Option<StorageConfig> {
        let data = app.data?;
        let context = opencode_servers::migration::Context { sidecar: app.sidecar, ..Default::default() };
        let mut import = ImportConfig::new(&data).context(context).groups(&app.groups);
        if let Some(updates) = app.updates {
            import = import.updates_dir(updates);
        }
        Some(StorageConfig::native(data.join("opencode.sqlite"), app.clock).import(import).official(app.official))
    }

    /// Desktop's `userData` folders (`ai.opencode.desktop`, `.beta`, `.dev`) to seed a fresh
    /// profile from after the first frame. Opened read-only; never written.
    pub fn official(mut self, dirs: Vec<PathBuf>) -> StorageConfig {
        self.official = dirs;
        self
    }
}

impl Storage {
    /// Opens the database on the calling thread, both stages. Never fails: a database that
    /// can't open or migrate leaves state in memory for this run ([`BootReport::error`]).
    pub fn open(config: StorageConfig) -> (Storage, BootReport) {
        let (storage, report, _) = boot::open(config);
        (storage, report)
    }

    /// [`Storage::open`] with the boot packet stage A read.
    pub fn open_with_boot(config: StorageConfig) -> (Storage, BootReport, BootPacket) {
        boot::open(config)
    }

    /// The open on its own thread, started at once: call it first thing in `main`, take
    /// [`Opening::boot`] where the first frame is built and [`Opening::storage`] after it.
    pub fn open_async(config: StorageConfig) -> Opening {
        Opening::start(config)
    }
}
/// The writer thread's handle, as the backend sees it.
pub(crate) enum Writer {
    /// Opened and read; no write yet, so no thread.
    Parked(Box<writer::Writer>),
    Running { tx: Sender<writer::Msg>, join: std::thread::JoinHandle<()> },
    /// In memory only, or closed.
    Off,
}

/// Commands on their way to the writer, in the order they were made. While a write-behind
/// buffer's timer is armed (`armed`), the writer will wake on it and take the queue then, so a
/// coalesced write (a keystroke's draft, a drag step's layout) is a push under a lock and no
/// wake-up of its own; anything else, or a write to a buffer with no timer armed, wakes it at once.
#[derive(Default)]
pub(crate) struct Queue {
    queries: search::query::Lane,
    pub(crate) cmds: Vec<Cmd>,
    /// The buffers (crate::BUFFER_*) whose timer is armed.
    pub(crate) armed: u8,
    /// A `Cmd::CommitSoon` waits in `cmds`: another one adds nothing (one commit per turn).
    pub(crate) commit_queued: bool,
}

pub(crate) type SharedQueue = Arc<Mutex<Queue>>;

pub(crate) fn lock_queue(q: &SharedQueue) -> std::sync::MutexGuard<'_, Queue> {
    q.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub(crate) struct Backend {
    writer: Mutex<Writer>,
    queue: SharedQueue,
    urgent: Arc<AtomicU64>,
}

impl Backend {
    pub(crate) fn memory() -> Backend {
        Backend { writer: Mutex::new(Writer::Off), queue: SharedQueue::default(), urgent: Arc::default() }
    }

    pub(crate) fn parked(w: writer::Writer) -> Backend {
        let dirty = w.wal_dirty();
        let backend = Backend { writer: Mutex::new(Writer::Parked(Box::new(w))), queue: SharedQueue::default(), urgent: Arc::default() };
        if dirty {
            // A database opened dirty has an armed idle-checkpoint timer that needs the writer
            // thread running even if no writes arrive.
            backend.sender();
        }
        backend
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Writer> {
        self.writer.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The running writer's sender, starting the thread on first use.
    fn sender(&self) -> Option<Sender<writer::Msg>> {
        let mut w = self.lock();
        match std::mem::replace(&mut *w, Writer::Off) {
            Writer::Parked(parked) => {
                let (tx, rx) = mpsc::channel();
                let queue = self.queue.clone();
                let urgent = self.urgent.clone();
                // Below normal: a commit the UI thread asked for (`commit_soon`) never preempts it.
                let spawned = std::thread::Builder::new().name("opencode-storage".into()).spawn(move || {
                    boot::priority(-1);
                    parked.run(rx, queue, urgent)
                });
                match spawned {
                    Ok(join) => {
                        *w = Writer::Running { tx: tx.clone(), join };
                        Some(tx)
                    }
                    Err(_) => None,
                }
            }
            Writer::Running { tx, join } => {
                *w = Writer::Running { tx: tx.clone(), join };
                Some(tx)
            }
            Writer::Off => None,
        }
    }

    pub(crate) fn send(&self, cmd: Cmd) {
        {
            let w = self.lock();
            if let Writer::Running { tx, .. } = &*w {
                if self.enqueue(cmd) {
                    tx.send(writer::Msg::Wake).ok();
                }
                return;
            }
        }
        match self.sender() {
            Some(tx) => {
                self.enqueue(cmd);
                tx.send(writer::Msg::Wake).ok();
            }
            None => answer_offline(cmd),
        }
    }

    /// Queues `cmd`; true when the writer must be woken for it.
    fn enqueue(&self, cmd: Cmd) -> bool {
        let mut q = lock_queue(&self.queue);
        self.urgent.fetch_add(1, Ordering::Release);
        if matches!(cmd, Cmd::CommitSoon) {
            if q.commit_queued {
                return false;
            }
            q.commit_queued = true;
        }
        let wake = q.armed & cmd.buffer() == 0;
        q.cmds.push(cmd);
        wake
    }

    fn query(&self, q: search::query::Query) {
        let Some(tx) = self.sender() else { return q.unavailable("storage is in memory".into()) };
        lock_queue(&self.queue).queries.replace(q);
        tx.send(writer::Msg::Wake).ok();
    }

    pub(crate) fn flush(&self, reason: FlushReason, deadline: Duration) -> FlushReport {
        let started = Instant::now();
        // Nothing was ever written (and the WAL holds nothing to checkpoint): nothing to land,
        // check or sync, and no thread is started for it.
        let quit = matches!(reason, FlushReason::Quit | FlushReason::SessionEnd | FlushReason::UpdateRestart);
        let parked_work = match &*self.lock() {
            Writer::Running { .. } => true,
            Writer::Parked(w) => quit && w.wal_dirty(),
            Writer::Off => false,
        };
        if !parked_work {
            return FlushReport { clean: true, ..FlushReport::default() };
        }
        // `parked_work` was true (a running writer or dirty WAL), so failing to spawn or reach the
        // writer means durability is unconfirmed; never return `clean: true` so `close()` keeps
        // the draft journal intact.
        let Some(tx) = self.sender() else {
            return FlushReport { clean: false, micros: micros(started), ..FlushReport::default() };
        };
        let (reply, answer) = mpsc::sync_channel(1);
        if tx.send(writer::Msg::Flush { reason, started, reply }).is_err() {
            return FlushReport { clean: false, micros: micros(started), ..FlushReport::default() };
        }
        self.urgent.fetch_add(1, Ordering::Release);
        answer.recv_timeout(deadline).unwrap_or(FlushReport { timed_out: true, micros: micros(started), ..FlushReport::default() })
    }

    pub(crate) fn first_frame(&self, done: Option<Loaded>) {
        // Search registration is connection-local, even on a fully migrated profile.
        if let Some(tx) = self.sender() {
            tx.send(writer::Msg::FirstFrame { done }).ok();
        }
    }

    pub(crate) fn close(&self) -> FlushReport {
        let report = self.flush(FlushReason::Quit, Duration::from_secs(2));
        // Before the writer stops (its repage may move pages): the journal is clean only once
        // every draft it holds is checkpointed into the database (or the database was clean).
        journal::finish(!report.timed_out && crate::write_trouble().is_none() && (report.clean || report.checkpointed));
        let w = std::mem::replace(&mut *self.lock(), Writer::Off);
        if let Writer::Running { tx, join } = w {
            tx.send(writer::Msg::Stop).ok();
            join.join().ok();
        }
        // The writer's last write and its repage are done: another process may have the profile.
        lease::release();
        report
    }
}

/// A command with no database behind it: callbacks still run, so a caller waiting on one never hangs.
fn answer_offline(cmd: Cmd) {
    match cmd {
        Cmd::PluginRecord { done: Some(done), .. } => done(Ok(())),
        Cmd::Outbox { op, done } => crate::outbox::offline(op, done),
        Cmd::LoadState { done: Some(done), .. } | Cmd::LoadDocs { done: Some(done), .. } | Cmd::LoadOutbox { done: Some(done) } => done(),
        Cmd::GetBlob { done, .. } => done(None),
        Cmd::GetPreview { done, .. } => done(None),
        Cmd::GetCache { done, .. } => done(None),
        Cmd::CommitSoon => {}
        _ => {}
    }
}

pub(crate) fn micros(since: Instant) -> u64 {
    u64::try_from(since.elapsed().as_micros()).unwrap_or(u64::MAX)
}

pub(crate) fn shared(cache: crate::cache::Cache) -> Arc<Mutex<crate::cache::Cache>> {
    Arc::new(Mutex::new(cache))
}

/// The tabs (`tabs`, `tabs.info`) of the window Desktop wrote last, in the newest of its
/// databases in `dirs` (its `userData` folders), read-only and in process. Feed them to
/// `opencode_kit::servers::migration::window::from_beta`.
pub fn desktop_recent_tabs(dirs: &[PathBuf]) -> Option<(serde_json::Value, serde_json::Value)> {
    seed::recent_tabs_in(dirs)
}
/// Has SQLite count its heap from now on, which the build leaves off (`SQLITE_DEFAULT_MEMSTATUS=0`):
/// for a measuring build, before the first connection opens.
pub fn count_sqlite_memory() {
    // SAFETY: SQLITE_CONFIG_MEMSTATUS (9) with an int, before SQLite is initialised; afterwards
    // SQLite refuses it with SQLITE_MISUSE and nothing changes.
    #[allow(unsafe_code)]
    unsafe {
        rusqlite::ffi::sqlite3_config(9, 1 as std::ffi::c_int);
    }
}

/// SQLite's own heap now and at its highest, in bytes (`sqlite3_memory_used`,
/// `sqlite3_memory_highwater`): its page caches and statements, which live outside the app's
/// allocator. Zero unless [`count_sqlite_memory`] ran first.
pub fn sqlite_memory() -> (i64, i64) {
    // SAFETY: both are thread-safe reads of SQLite's global counters.
    #[allow(unsafe_code)]
    unsafe {
        (rusqlite::ffi::sqlite3_memory_used(), rusqlite::ffi::sqlite3_memory_highwater(0))
    }
}
