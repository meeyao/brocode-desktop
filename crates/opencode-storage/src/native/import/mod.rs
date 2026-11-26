//! The one-time import of the files Rune kept before the database, driven by
//! [`machine::step`]: scan, import in one transaction (`ON CONFLICT DO NOTHING`, so an existing
//! row always wins, as Desktop's `storage/legacy.ts:76-81`), checkpoint, read every row back
//! through a fresh read-only connection, and only then (after the first frame) remove the files
//! that verified. Unreadable files are never imported and never removed (`legacy.ts:38-41`).

pub mod machine;
mod outbox;
mod retire;
pub mod shapes;

use std::path::PathBuf;
use std::time::Instant;

use rune::machine::Effects;
use rusqlite::{Connection, OpenFlags, OptionalExtension};

use self::machine::{ImportEffect, ImportEvent, ImportState};
use self::shapes::{Plan, Row};
use super::schema::{self, FLAG_RETIRED, FLAG_SEED_SETTLED};
pub use super::schema::ImportGroup;
use super::sql;
use opencode_servers::migration::Context;

pub use self::outbox::rebuild as rebuild_outbox_row;

/// Where the old files are and what maps their server addresses.
#[derive(Clone, Debug)]
pub struct ImportConfig {
    pub(crate) dir: PathBuf,
    pub(crate) updates_dir: Option<PathBuf>,
    pub(crate) context: Context,
    pub(crate) servers: Vec<String>,
    pub(crate) groups: Vec<ImportGroup>,
}

impl ImportConfig {
    /// `dir`: the app's data folder (`%APPDATA%\brocode`). Every group is imported unless
    /// [`ImportConfig::groups`] narrows it.
    pub fn new(dir: impl Into<PathBuf>) -> ImportConfig {
        let context = Context { sidecar: vec![opencode_kit::client::DEFAULT_SERVER.to_owned()], ..Context::default() };
        ImportConfig { dir: dir.into(), updates_dir: None, context, servers: Vec::new(), groups: ImportGroup::ALL.to_vec() }
    }

    /// The groups the running build has cut over: only their files are scanned, imported,
    /// verified and retired; every other file is left as it is, unread.
    pub fn groups(mut self, groups: &[ImportGroup]) -> ImportConfig {
        self.groups = groups.to_vec();
        self
    }

    /// The flags of the cut-over groups not settled in `flags`.
    pub(crate) fn pending(&self, flags: u32) -> u32 {
        self.groups.iter().map(|g| g.flag()).filter(|f| flags & f == 0).fold(0, |a, f| a | f)
    }

    /// The updater's folder (`Cache\Updates`), whose `ready` and `rollout-bucket` markers move in.
    pub fn updates_dir(mut self, dir: impl Into<PathBuf>) -> ImportConfig {
        self.updates_dir = Some(dir.into());
        self
    }

    /// What maps old server addresses to keys (MS-1's `migration::Context`): the app's own
    /// service's addresses, contributed servers' endpoints, the saved list.
    pub fn context(mut self, context: Context) -> ImportConfig {
        self.context = context;
        self
    }

    /// More server addresses an outbox journal may be named after.
    pub fn servers(mut self, servers: Vec<String>) -> ImportConfig {
        self.servers = servers;
        self
    }
}

/// What the import did.
#[derive(Clone, Debug, Default)]
pub struct ImportSummary {
    pub files: usize,
    pub rows: usize,
    pub inserted: usize,
    /// Rows already in the database, which won.
    pub kept_existing: usize,
    pub verified_files: usize,
    pub mismatched: Vec<PathBuf>,
    pub unreadable: Vec<PathBuf>,
    /// Files moved into the retired folder.
    pub retired: usize,
    pub seeded_rows: usize,
    /// Drafts skipped during first-run Official Desktop migration because a chunk/attachment blob
    /// was missing or unreadable (surfaced once to the user; Official's files remain untouched).
    pub seeded_dropped_drafts: usize,
    pub seeded_source_dir: Option<PathBuf>,
    pub scan_ns: u64,
    pub write_ns: u64,
    pub verify_ns: u64,
    pub reports: Vec<String>,
}

/// The import's state between the open and the first frame.
#[derive(Debug, Default)]
pub(crate) struct Deferred {
    pub state: ImportState,
    pub official: Vec<PathBuf>,
    pub flags: u32,
    /// The cut-over groups this run settles once their files are retired.
    pub groups: u32,
    /// The data folder, where the retired folder goes.
    pub dir: Option<PathBuf>,
    /// Parent directory of OUR `opencode.sqlite`, where temporary seed snapshots live.
    pub scratch_parent: Option<PathBuf>,
    pub summary: ImportSummary,
}

impl Deferred {
    /// After the first frame there's work: files to retire, a seed, or a retired folder to check.
    pub fn pending(&self) -> bool {
        matches!(self.state, ImportState::RetireDeferred { .. } | ImportState::SeedDeferred) || self.flags & FLAG_RETIRED != 0
    }
}

/// Reopens the read-write connection (its exclusive lock must go for a fresh connection to read).
pub(crate) type Reopen<'a> = &'a dyn Fn() -> rusqlite::Result<Connection>;

struct Driver<'a> {
    conn: &'a mut Connection,
    reopen: Option<Reopen<'a>>,
    cfg: Option<&'a ImportConfig>,
    official: &'a [PathBuf],
    now_ms: i64,
    flags: u32,
    groups: u32,
    dir: Option<PathBuf>,
    scratch_parent: Option<PathBuf>,
    summary: ImportSummary,
    files: Vec<PathBuf>,
    plan: Plan,
    /// Per row of `plan`, in order: whether this import inserted it (false: it was there).
    inserted: Vec<bool>,
    seeded: Vec<(String, String, String)>,
    /// What the open already read from Desktop for a new profile (`seed::collect`), to commit.
    collected: Option<super::seed::Collected>,
}

/// Runs the open's part: schema flags to verified import (and first-run Official Desktop seed).
/// Returns the summary, what waits for the first frame, and the header flags to store.
pub(crate) fn run(
    conn: &mut Connection,
    reopen: Reopen<'_>,
    cfg: &ImportConfig,
    official: &[PathBuf],
    scratch_parent: PathBuf,
    flags: u32,
    now_ms: i64,
    collected: Option<super::seed::Collected>,
) -> (ImportSummary, Deferred, u32) {
    let mut state = ImportState::default();
    let groups = cfg.pending(flags);
    let effects = machine::step(&mut state, ImportEvent::SchemaReady { pending: groups != 0, seed_settled: flags & FLAG_SEED_SETTLED != 0 });
    let mut d = Driver {
        conn,
        reopen: Some(reopen),
        cfg: Some(cfg),
        official,
        now_ms,
        flags,
        groups,
        dir: Some(cfg.dir.clone()),
        scratch_parent: Some(scratch_parent.clone()),
        summary: ImportSummary::default(),
        files: Vec::new(),
        plan: Plan::default(),
        inserted: Vec::new(),
        seeded: Vec::new(),
        collected,
    };
    drive(&mut state, effects, &mut d);
    let flags = d.flags;
    let summary = d.summary;
    (summary.clone(), Deferred { state, official: official.to_vec(), flags, groups, dir: Some(cfg.dir.clone()), scratch_parent: Some(scratch_parent), summary }, flags)
}

/// The first frame is up: retires verified files, or seeds a fresh profile from Desktop's
/// database, then purges a retired folder whose 14 clean days are up. Returns the `state` rows
/// the seed added, for the cache.
pub(crate) fn after_first_frame(conn: &mut Connection, deferred: &mut Deferred, now_ms: i64) -> Vec<(String, String, String)> {
    let effects = machine::step(&mut deferred.state, ImportEvent::FirstFramePresented);
    let official = std::mem::take(&mut deferred.official);
    let mut d = Driver {
        conn,
        reopen: None,
        cfg: None,
        official: &official,
        now_ms,
        flags: deferred.flags,
        groups: deferred.groups,
        dir: deferred.dir.clone(),
        scratch_parent: deferred.scratch_parent.clone(),
        summary: std::mem::take(&mut deferred.summary),
        files: Vec::new(),
        plan: Plan::default(),
        inserted: Vec::new(),
        seeded: Vec::new(),
        collected: None,
    };
    drive(&mut deferred.state, effects, &mut d);
    if d.flags & FLAG_RETIRED != 0 {
        let clean = d.summary.mismatched.is_empty() && !d.summary.reports.iter().any(|r| r.contains("failed"));
        retire::sweep(&mut d, clean);
    }
    deferred.flags = d.flags;
    deferred.summary = d.summary;
    d.seeded
}

fn drive(state: &mut ImportState, effects: Effects<ImportEffect>, d: &mut Driver<'_>) {
    let mut queue: Vec<ImportEffect> = effects.into_iter().collect();
    queue.reverse();
    while let Some(effect) = queue.pop() {
        let event = match effect {
            ImportEffect::Scan => Some(scan(d)),
            ImportEffect::Import => Some(import(d)),
            ImportEffect::Verify => Some(verify(d)),
            ImportEffect::Retire(verified) => {
                let failed = retire::retire(d, &verified);
                Some(ImportEvent::FilesRetired { failed })
            }
            ImportEffect::Seed => {
                let scratch = d
                    .scratch_parent
                    .clone()
                    .or_else(|| d.dir.clone())
                    .unwrap_or_else(std::env::temp_dir);
                let settle_flags = d.flags | d.groups | FLAG_SEED_SETTLED;
                // Read by the open before the database existed (a new profile), or here.
                let collected = d.collected.take().unwrap_or_else(|| super::seed::collect(d.official, &scratch));
                match super::seed::commit(d.conn, collected, settle_flags, d.now_ms) {
                    super::seed::SeedOutcome::NoSource => {
                        d.flags = settle_flags;
                        d.summary.reports.push("no Desktop database to seed from".into());
                        Some(ImportEvent::OfficialSeedSettled)
                    }
                    super::seed::SeedOutcome::Seeded { rows, dropped_drafts, source_dir, report } => {
                        d.flags = settle_flags;
                        d.summary.seeded_rows = rows.len();
                        d.summary.seeded_dropped_drafts = dropped_drafts;
                        d.summary.seeded_source_dir = Some(source_dir);
                        d.summary.reports.extend(report);
                        d.seeded = rows;
                        Some(ImportEvent::OfficialSeedSettled)
                    }
                    super::seed::SeedOutcome::Failed { exhausted: true, report } => {
                        d.flags = settle_flags;
                        d.summary.reports.push(report);
                        Some(ImportEvent::OfficialSeedSettled)
                    }
                    super::seed::SeedOutcome::Failed { exhausted: false, report } => {
                        Some(ImportEvent::OfficialSeedFailed { error: report })
                    }
                }
            }
            ImportEffect::MarkGroupsSettled => mark(d, d.groups),
            ImportEffect::MarkSeedSettled => mark(d, FLAG_SEED_SETTLED),
            ImportEffect::FallBackToMemory => None,
            ImportEffect::Report(msg) => {
                eprintln!("opencode-storage: {msg}");
                d.summary.reports.push(msg);
                None
            }
        };
        if let Some(event) = event {
            let mut next: Vec<ImportEffect> = machine::step(state, event).into_iter().collect();
            next.reverse();
            queue.extend(next);
        }
    }
}

fn mark(d: &mut Driver<'_>, flag: u32) -> Option<ImportEvent> {
    d.flags |= flag;
    if let Err(e) = schema::set_flags(d.conn, d.flags) {
        d.summary.reports.push(format!("couldn't record the import flag: {e}"));
    }
    None
}

fn ns_since(t: Instant) -> u64 {
    u64::try_from(t.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

fn scan(d: &mut Driver<'_>) -> ImportEvent {
    let t = Instant::now();
    let Some(cfg) = d.cfg else { return ImportEvent::NoRuneFiles { fresh: false } };
    // One listing; only the pending groups' files are kept, the others only counted.
    let all = shapes::scan(cfg);
    let any = !all.is_empty();
    d.files = all.into_iter().filter(|p| shapes::group_of(cfg, p).is_some_and(|g| d.groups & g.flag() != 0)).collect();
    d.summary.scan_ns = ns_since(t);
    if d.files.is_empty() {
        let empty = !d
            .conn
            .query_row(
                "SELECT EXISTS (SELECT 1 FROM state WHERE name != 'rune.storage') OR EXISTS (SELECT 1 FROM document) OR EXISTS (SELECT 1 FROM window_state)",
                [],
                |r| r.get::<_, bool>(0),
            )
            .unwrap_or(true);
        let retrying = super::seed::prior_attempts(d.conn) > 0;
        return ImportEvent::NoRuneFiles { fresh: (empty || retrying) && !any };
    }
    ImportEvent::FoundRuneFiles { files: d.files.len() }
}

fn import(d: &mut Driver<'_>) -> ImportEvent {
    let t = Instant::now();
    let Some(cfg) = d.cfg else { return ImportEvent::ImportFailed { error: "no import config".into() } };
    d.plan = shapes::plan(cfg, &d.files);
    d.summary.files = d.plan.sources.iter().map(|s| s.paths.len()).sum();
    d.summary.unreadable = d.plan.unreadable.clone();
    d.summary.rows = d.plan.rows();
    let now = d.now_ms;
    let result = (|| -> rusqlite::Result<Vec<bool>> {
        let tx = d.conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let mut inserted = Vec::with_capacity(d.plan.rows());
        for source in &d.plan.sources {
            for (id, data) in &source.blobs {
                sql::insert_blob(&tx, id, data, now)?;
            }
            for row in &source.rows {
                inserted.push(insert(&tx, row, now)?);
            }
        }
        super::boot_record::rebuild(&tx)?;
        tx.commit()?;
        Ok(inserted)
    })();
    match result {
        Ok(inserted) => {
            d.summary.inserted = inserted.iter().filter(|i| **i).count();
            d.summary.kept_existing = inserted.len() - d.summary.inserted;
            d.inserted = inserted;
            let checkpoint = d.conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| r.get::<_, i64>(0));
            d.summary.write_ns = ns_since(t);
            match checkpoint {
                Ok(0) => ImportEvent::CommittedAndCheckpointed,
                Ok(_) => ImportEvent::ImportFailed { error: "checkpoint busy".into() },
                Err(e) => ImportEvent::ImportFailed { error: e.to_string() },
            }
        }
        Err(e) => ImportEvent::ImportFailed { error: e.to_string() },
    }
}

/// One row, kept when the database already has it. True when it was inserted.
fn insert(conn: &Connection, row: &Row, now: i64) -> rusqlite::Result<bool> {
    let n = match row {
        Row::State { name, key, value } => conn.prepare_cached("INSERT INTO state (name, key, value, updated_at) VALUES (?1, ?2, ?3, ?4) ON CONFLICT DO NOTHING")?.execute((name, key, value, now))?,
        Row::Doc { key, value } => {
            let (stored, chunks) = sql::encode_text(value);
            for (id, data) in &chunks {
                sql::insert_blob(conn, id, data, now)?;
            }
            conn.prepare_cached("INSERT INTO document (key, value) VALUES (?1, ?2) ON CONFLICT DO NOTHING")?.execute((key, stored))?
        }
        Row::Window { id, frame } => {
            if conn.prepare_cached("SELECT 1 FROM window_state WHERE id = ?1")?.query_row([id], |_| Ok(())).optional()?.is_some() {
                return Ok(false);
            }
            sql::put_window(conn, id, Some(frame), now)?;
            1
        }
        Row::Plugin { id, enabled } => conn.prepare_cached("INSERT INTO plugin (id, enabled) VALUES (?1, ?2) ON CONFLICT DO NOTHING")?.execute((id, enabled))?,
        Row::Trust { id, sha256 } => conn.prepare_cached("INSERT INTO plugin_trust (id, sha256, trusted_at) VALUES (?1, ?2, ?3) ON CONFLICT DO NOTHING")?.execute((id, sha256, now))?,
        Row::Outbox(row) => usize::from(sql::put_outbox(conn, row, now, false)?),
    };
    Ok(n > 0)
}

/// Reads every row back through a fresh read-only connection, after the checkpoint. A source's
/// files verify when each row is there with the imported value (or was there before, and won).
fn verify(d: &mut Driver<'_>) -> ImportEvent {
    let t = Instant::now();
    let path = d.conn.path().map(PathBuf::from).unwrap_or_default();
    let all: Vec<PathBuf> = d.plan.sources.iter().flat_map(|s| s.paths.clone()).collect();
    // The read-write connection holds the file exclusively: close it, read back through a fresh
    // one, then open it again.
    let (Some(reopen), Ok(placeholder)) = (d.reopen, Connection::open_in_memory()) else { return ImportEvent::Verified { matched: Vec::new(), mismatched: all } };
    drop(std::mem::replace(d.conn, placeholder));
    let ro = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX).inspect(sql::persist_wal);
    let verified = ro.map(|ro| verify_with(d, &ro));
    match reopen() {
        Ok(conn) => *d.conn = conn,
        Err(e) => d.summary.reports.push(format!("couldn't reopen the database after the import's read-back: {e}")),
    }
    d.summary.verify_ns = ns_since(t);
    verified.unwrap_or(ImportEvent::Verified { matched: Vec::new(), mismatched: all })
}

fn verify_with(d: &mut Driver<'_>, ro: &Connection) -> ImportEvent {
    let (mut matched, mut mismatched) = (Vec::new(), Vec::new());
    let mut at = 0;
    for source in &d.plan.sources {
        let blobs_ok = source.blobs.iter().all(|(id, data)| sql::read_blob(ro, id).ok().flatten().as_deref() == Some(data.as_slice()));
        let mut ok = blobs_ok;
        for row in &source.rows {
            let fresh = d.inserted.get(at).copied().unwrap_or(false);
            at += 1;
            ok &= check(ro, row, fresh).unwrap_or(false);
        }
        if ok {
            matched.extend(source.paths.iter().cloned());
        } else {
            mismatched.extend(source.paths.iter().cloned());
        }
    }
    d.summary.verified_files = matched.len();
    d.summary.mismatched = mismatched.clone();
    ImportEvent::Verified { matched, mismatched }
}

fn check(ro: &Connection, row: &Row, fresh: bool) -> rusqlite::Result<bool> {
    // A row that was there before this import won: it only has to still be there.
    let same = |stored: Option<String>, expected: &str| stored.is_some_and(|s| !fresh || s == expected);
    Ok(match row {
        Row::State { name, key, value } => same(ro.query_row("SELECT value FROM state WHERE name = ?1 AND key = ?2", [name, key], |r| r.get(0)).optional()?, value),
        Row::Doc { key, value } => same(sql::read_document(ro, key)?, value),
        Row::Window { id, frame } => {
            let found: Option<(Option<i32>, Option<i32>, u32, u32)> = ro.query_row("SELECT x, y, width, height FROM window_state WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).optional()?;
            found.is_some_and(|f| !fresh || f == (frame.x, frame.y, frame.width, frame.height))
        }
        Row::Plugin { id, enabled } => ro.query_row("SELECT enabled FROM plugin WHERE id = ?1", [id], |r| r.get::<_, bool>(0)).optional()?.is_some_and(|e| !fresh || e == *enabled),
        Row::Trust { id, sha256 } => same(ro.query_row("SELECT sha256 FROM plugin_trust WHERE id = ?1", [id], |r| r.get(0)).optional()?, sha256),
        Row::Outbox(row) => {
            let found = sql::read_outbox_row(ro, &row.server, &row.id)?;
            found.is_some_and(|f| !fresh || f == **row)
        }
    })
}

/// When the database can't be opened: the cut-over groups' files as rows in memory, so the app
/// runs from what the user had rather than from nothing. The files are only read.
pub(crate) fn hydrate_memory(cache: &mut crate::cache::Cache, cfg: &ImportConfig) {
    let files: Vec<PathBuf> = shapes::scan(cfg).into_iter().filter(|p| shapes::group_of(cfg, p).is_some_and(|g| cfg.groups.contains(&g))).collect();
    let plan = shapes::plan(cfg, &files);
    for row in plan.sources.into_iter().flat_map(|s| s.rows) {
        match row {
            Row::State { name, key, value } => {
                cache.set(&name, key, Some(value.into()));
            }
            Row::Doc { key, value } => {
                cache.docs.insert(key, crate::cache::Doc::Present(value.into()));
            }
            Row::Window { id, frame } => {
                cache.windows.insert(id, frame);
            }
            Row::Plugin { id, enabled } => {
                cache.plugins.insert(id, enabled);
            }
            Row::Trust { id, sha256 } => {
                cache.trust.insert(id, sha256);
            }
            Row::Outbox(row) => {
                if let crate::cache::OutboxCache::Loaded(all) = &mut cache.outbox {
                    all.entry(row.server.clone()).or_default().push(*row);
                }
            }
        }
    }
}
