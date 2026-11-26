//! First-run migration from Official Desktop (`drafts.sqlite`, `opencode.settings`,
//! `window-state-<id>.json`) on a fresh profile, in two halves: [`collect`] reads Desktop's rows
//! (it needs no database of ours, so a new profile's open runs it before the database exists and
//! builds the first `BootPacket` from what it read), [`commit`] writes them.
//!
//! Safety invariants:
//! 1. **Zero writes to Official Desktop's folder:** `drafts.sqlite` is read in place only while
//!    nobody writes it (no `-wal`, no rollback journal: opened `immutable`, so no lock and no
//!    `-shm`, and thrown away if the file changed during the read); otherwise it (+ `-wal`; never
//!    `-shm`) is read via plain shared file reads into memory with the WAL's committed frames
//!    applied (`image`), thrown away if either file changed during the read, and verified with
//!    `PRAGMA integrity_check`. When that fails, both files are copied into a scratch directory
//!    inside OUR profile, recovered and verified there, and deleted on drop.
//! 2. **Explicit allow-list + credential scrubbing:** only approved settings, zoom, window frames,
//!    tabs, drafts + attachment blobs, prompt history, and strictly allow-listed, userinfo-stripped
//!    HTTP server entries are imported (never passwords, tokens, `ssh.servers`, `bundledCliVersion`,
//!    or plugin stores such as `plugin.browser/sitePermissions` or `plugin.pairing`).
//! 3. **Atomic settle + bounded retries:** migrated data, rebuilt boot record, and the settled
//!    header flags (`groups | FLAG_SEED_SETTLED`) commit in one SQLite transaction. A crash before
//!    commit or a transient locked/torn read leaves migration unsettled and retries up to
//!    `MAX_SEED_ATTEMPTS` launches; once settled, migration never runs again.

mod drafts;
mod image;
mod snapshot;

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use opencode_kit::servers::{Route, TabKey};
use opencode_servers::tabs::Tab;
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde_json::{Map, Value, json};

use crate::WindowFrame;

use self::drafts::{
    MAX_SEED_TOTAL_BLOBS_BYTES, convert_composer_value, convert_prompt_history, load_session_docs,
    official_draft_doc_key, parse_official_doc_strict, read_official_doc,
};
use self::snapshot::{
    MAX_SETTINGS_JSON_BYTES, MAX_WINDOW_STATE_BYTES, PickResult, open_snapshot, pick_dir,
    read_capped,
};
use super::{boot_record, schema, sql};

/// Maximum windows imported from an Official Desktop profile.
const MAX_SEED_WINDOWS: usize = 32;
/// Maximum value size for any single allow-listed `state` row (1 MiB).
const MAX_STATE_VALUE_BYTES: usize = 1024 * 1024;
/// Maximum total bytes across all copied `state` values (8 MiB).
const MAX_TOTAL_STATE_BYTES: usize = 8 * 1024 * 1024;
/// Bounded number of launches that retry after a transient lock / torn copy before settling.
pub(crate) const MAX_SEED_ATTEMPTS: u32 = 5;
/// Internal storage metadata namespace (ignored by fresh-DB checks).
pub(crate) const META_NS: &str = "rune.storage";
pub(crate) const ATTEMPTS_KEY: &str = "official_seed_attempts";

/// Allow-listed keys copied verbatim from Official's `opencode.global.dat` (`server` is normalized
/// and scrubbed separately via [`scrub_server_json`]).
const GLOBAL_ALLOW: &[&str] = &["language", "layout", "tips.queue", "settings.v3"];
/// Allow-listed keys copied from each `opencode.window.<id>.dat` namespace (consumed by Rune).
const WINDOW_ALLOW: &[&str] = &["tabs", "tabs.recent", "tabs.info", "tabs.closed", "lastActiveUrl"];

/// Outcome of a first-run seed attempt.
pub(crate) enum SeedOutcome {
    /// No Official Desktop database existed in any candidate folder; flags settled atomically.
    NoSource,
    /// Migrated rows + boot record + settled flags committed in one atomic transaction.
    Seeded {
        rows: Vec<(String, String, String)>,
        dropped_drafts: usize,
        source_dir: PathBuf,
        report: Vec<String>,
    },
    /// Official database existed but could not be read/verified/committed cleanly.
    /// When `exhausted` is false, `FLAG_SEED_SETTLED` remains unset so the next launch retries.
    Failed { exhausted: bool, report: String },
}

/// The tabs (`tabs`, `tabs.info`) of the window Desktop wrote last, read via a private scratch
/// copy so Official Desktop's folder is never written to.
pub fn recent_tabs_in(dirs: &[PathBuf]) -> Option<(Value, Value)> {
    let scratch = std::env::temp_dir().join(format!("brocode-seed-{}", std::process::id()));
    // Read in place when nobody writes the database; from a copy when it was written to meanwhile.
    let res = [true, false].into_iter().find_map(|in_place| {
        let PickResult::Picked(_, snap) = pick_dir(dirs, &scratch, in_place) else { return Some(None) };
        let res = recent_tabs_from(snap.conn());
        snap.held().then_some(res)
    });
    let _ = std::fs::remove_dir_all(&scratch);
    res.flatten()
}

/// Legacy single-file helper kept safe via a private read (the image in memory, or the scratch copy; never writes `-shm` beside `db`).
pub fn recent_tabs(db: &Path) -> Option<(Value, Value)> {
    let dir = db.parent()?;
    let scratch = std::env::temp_dir().join(format!("brocode-seed-{}", std::process::id()));
    let snap = open_snapshot(dir, &scratch).ok()?;
    let res = recent_tabs_from(snap.conn());
    drop(snap);
    let _ = std::fs::remove_dir_all(&scratch);
    res
}

fn recent_tabs_from(conn: &Connection) -> Option<(Value, Value)> {
    let name: String = conn
        .query_row(
            "SELECT name FROM state WHERE key = 'tabs' AND name LIKE 'opencode.window.%' ORDER BY updated_at DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .optional()
        .ok()??;
    let get = |key: &str| -> Value {
        conn.query_row("SELECT value FROM state WHERE name = ?1 AND key = ?2", [&name, key], |r| r.get::<_, String>(0))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(Value::Null)
    };
    Some((get("tabs"), get("tabs.info")))
}

fn is_safe_window_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 128 && !id.contains("..") && id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

fn derived_last_active_url(win_rows: &[(String, String)], surviving_tabs: &[Tab]) -> Option<String> {
    if win_rows.iter().any(|(k, _)| k == "lastActiveUrl") {
        return None;
    }
    let recent_raw = win_rows.iter().find(|(k, _)| k == "tabs.recent")?.1.as_str();
    let recent_key = serde_json::from_str::<Value>(recent_raw).ok()?.get("key")?.as_str().and_then(TabKey::parse)?;
    if !surviving_tabs.iter().any(|t| t.key() == recent_key) {
        return None;
    }
    let href = match recent_key {
        TabKey::Session { server, session_id } => Route::Session { server, session_id }.href(),
        TabKey::Draft(id) => Route::Draft(id).href(),
    };
    Some(json!(href).to_string())
}

/// Strips embedded `user:pass@` userinfo from an `http://` or `https://` URL.
/// Rejects non-HTTP(S) schemes, whitespace/control chars, and empty hosts.
fn strip_url_userinfo(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return None;
    }
    let (scheme, rest) = trimmed.split_once("://")?;
    let scheme_lower = scheme.to_ascii_lowercase();
    if !matches!(scheme_lower.as_str(), "http" | "https") {
        return None;
    }
    let split_at = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, suffix) = rest.split_at(split_at);
    let host_port = authority.rsplit_once('@').map_or(authority, |(_, hp)| hp);
    if host_port.is_empty() {
        return None;
    }
    Some(format!("{scheme_lower}://{host_port}{suffix}"))
}

/// Strictly allow-lists and normalizes Official's `opencode.global.dat / server` JSON:
/// - Keeps `lastProjectID`, `usedProjectIDs`, `hostnames`, `projects`, `hidden`.
/// - Normalizes legacy bare-string and flat `{ url }` `StoredServer` entries as well as modern
///   `{ type: "http", http: { url, username }, displayName, label }` entries.
/// - Copies ONLY explicitly allow-listed fields (`type`, `displayName`, `label`, `http.url` with
///   userinfo stripped, `http.username`) so no passwords, tokens, passphrases, or unlisted nested
///   objects can ever pass through.
fn scrub_server_json(raw: &str) -> Option<(String, HashSet<String>)> {
    let root = serde_json::from_str::<Value>(raw).ok()?;
    let obj = root.as_object()?;
    let mut out = Map::new();
    for keep in ["lastProjectID", "usedProjectIDs", "hostnames", "projects", "hidden"] {
        if let Some(v) = obj.get(keep) {
            out.insert(keep.into(), v.clone());
        }
    }
    let mut imported_urls = HashSet::new();
    if let Some(list) = obj.get("list").and_then(Value::as_array) {
        let mut clean_list = Vec::with_capacity(list.len());
        for entry in list {
            let clean_entry = match entry {
                // Legacy bare-string StoredServer (`persistence.ts:19-30`)
                Value::String(raw_url) => {
                    let Some(url) = strip_url_userinfo(raw_url) else { continue };
                    imported_urls.insert(url.trim_end_matches('/').to_owned());
                    json!({ "type": "http", "http": { "url": url } })
                }
                Value::Object(em) => {
                    let kind = em.get("type").and_then(Value::as_str);
                    // Only import HTTP servers (or legacy flat objects with no `type` and a `url`).
                    if let Some(t) = kind && t != "http" {
                        continue;
                    }
                    let (raw_url, username) = if let Some(hm) = em.get("http").and_then(Value::as_object) {
                        (
                            hm.get("url").and_then(Value::as_str),
                            hm.get("username").and_then(Value::as_str).filter(|s| !s.is_empty()),
                        )
                    } else {
                        (
                            em.get("url").and_then(Value::as_str),
                            em.get("username").and_then(Value::as_str).filter(|s| !s.is_empty()),
                        )
                    };
                    let Some(url) = raw_url.and_then(strip_url_userinfo) else { continue };
                    imported_urls.insert(url.trim_end_matches('/').to_owned());
                    let mut clean_http = Map::new();
                    clean_http.insert("url".into(), json!(url));
                    if let Some(user) = username {
                        clean_http.insert("username".into(), json!(user));
                    }
                    let mut clean = Map::new();
                    clean.insert("type".into(), json!("http"));
                    if let Some(dn) = em.get("displayName").and_then(Value::as_str).filter(|s| !s.is_empty()) {
                        clean.insert("displayName".into(), json!(dn));
                    }
                    if let Some(lb) = em.get("label").and_then(Value::as_str).filter(|s| !s.is_empty()) {
                        clean.insert("label".into(), json!(lb));
                    }
                    clean.insert("http".into(), Value::Object(clean_http));
                    Value::Object(clean)
                }
                _ => continue,
            };
            clean_list.push(clean_entry);
        }
        out.insert("list".into(), Value::Array(clean_list));
    }
    Some((Value::Object(out).to_string(), imported_urls))
}

/// Returns how many prior failed seed attempts are recorded in `rune.storage`.
pub(crate) fn prior_attempts(conn: &Connection) -> u32 {
    conn.query_row("SELECT value FROM state WHERE name = ?1 AND key = ?2", [META_NS, ATTEMPTS_KEY], |r| r.get::<_, String>(0))
        .optional()
        .ok()
        .flatten()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0)
}

fn record_attempt(conn: &mut Connection, settle_flags: u32, now_ms: i64, why: String) -> SeedOutcome {
    let next = prior_attempts(conn).saturating_add(1);
    let exhausted = next >= MAX_SEED_ATTEMPTS;
    let _ = (|| -> rusqlite::Result<()> {
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if exhausted {
            tx.execute("DELETE FROM state WHERE name = ?1 AND key = ?2", [META_NS, ATTEMPTS_KEY])?;
            schema::set_flags(&tx, settle_flags)?;
        } else {
            tx.execute(
                "INSERT INTO state (name, key, value, updated_at) VALUES (?1, ?2, ?3, ?4) ON CONFLICT (name, key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
                (META_NS, ATTEMPTS_KEY, next.to_string(), now_ms),
            )?;
        }
        tx.commit()
    })();
    let suffix = if exhausted {
        format!("giving up after {next}/{MAX_SEED_ATTEMPTS} launches; official Desktop data untouched")
    } else {
        format!("attempt {next}/{MAX_SEED_ATTEMPTS}; will retry on next launch")
    };
    SeedOutcome::Failed { exhausted, report: format!("seed from Desktop deferred ({why}); {suffix}") }
}

/// On a retry launch (`prior_attempts > 0`), detects untouched blank default rows written by a
/// prior locked launch (a single empty Home window with no closed tabs / no URL, or an empty
/// default `server` row) only when the incoming bundle actually carries replacements.
/// Never overwrites any real user work (open tabs, closed tabs, non-home route, settings, servers).
fn clear_blank_retry_defaults(tx: &Transaction<'_>, bundle: &SeedBundle) -> rusqlite::Result<()> {
    if bundle.window_count > 0 {
        let win_ids: Option<Vec<String>> = tx
            .query_row("SELECT value FROM state WHERE name = 'opencode.settings' AND key = 'windowIds'", [], |r| r.get::<_, String>(0))
            .optional()?
            .and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok());
        if let Some(ids) = win_ids && ids.len() == 1 {
            let ns = crate::window_namespace(&ids[0]);
            let tabs_empty = tx
                .query_row("SELECT value FROM state WHERE name = ?1 AND key = 'tabs'", [&ns], |r| r.get::<_, String>(0))
                .optional()?
                .and_then(|s| serde_json::from_str::<Vec<Value>>(&s).ok())
                .is_none_or(|t| t.is_empty());
            let closed_empty = tx
                .query_row("SELECT value FROM state WHERE name = ?1 AND key = 'tabs.closed'", [&ns], |r| r.get::<_, String>(0))
                .optional()?
                .and_then(|s| serde_json::from_str::<Vec<Value>>(&s).ok())
                .is_none_or(|t| t.is_empty());
            let no_url = tx
                .query_row("SELECT value FROM state WHERE name = ?1 AND key = 'lastActiveUrl'", [&ns], |r| r.get::<_, String>(0))
                .optional()?
                .is_none_or(|u| u == "\"/\"" || u == "null");
            if tabs_empty && closed_empty && no_url {
                tx.execute("DELETE FROM state WHERE name = 'opencode.settings' AND key = 'windowIds'", [])?;
                tx.execute("DELETE FROM state WHERE name = ?1", [&ns])?;
                tx.execute("DELETE FROM window_state WHERE id = ?1", [&ids[0]])?;
            }
        }
    }
    if bundle.has_server {
        let srv_blank = tx
            .query_row("SELECT value FROM state WHERE name = 'opencode.global.dat' AND key = 'server'", [], |r| r.get::<_, String>(0))
            .optional()?
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
            .is_some_and(|v| {
                v["list"].as_array().is_none_or(Vec::is_empty)
                    && v["usedProjectIDs"].as_object().is_none_or(|m| m.values().all(|a| a.as_array().is_none_or(Vec::is_empty)))
            });
        if srv_blank {
            tx.execute("DELETE FROM state WHERE name = 'opencode.global.dat' AND key = 'server'", [])?;
        }
    }
    Ok(())
}

pub(crate) struct SeedBundle {
    global_rows: Vec<(String, String, String)>,
    window_rows: Vec<(String, String, String)>,
    global_docs: Vec<(String, String)>,
    window_docs: Vec<(String, String)>,
    window_blobs: Vec<(String, Vec<u8>)>,
    frames: Vec<(String, WindowFrame)>,
    window_count: usize,
    has_server: bool,
    dropped_drafts: usize,
    notes: Vec<String>,
}

fn collect_bundle(dir: &Path, snap: &snapshot::Snapshot) -> Result<SeedBundle, String> {
    let official = snap.conn();
    let has_state = snap.has_table("state");
    let has_doc = snap.has_table("document");
    let has_blobs = snap.has_table("blob");

    let mut notes = Vec::new();
    let settings_val: Value = match read_capped(&dir.join("opencode.settings"), MAX_SETTINGS_JSON_BYTES) {
        Ok(Some(raw)) => serde_json::from_str(&raw).unwrap_or(Value::Null),
        Ok(None) => Value::Null,
        Err(e) => {
            notes.push(format!("skipped opencode.settings: {e}"));
            Value::Null
        }
    };

    let mut global_rows: Vec<(String, String, String)> = Vec::new();
    let mut window_rows: Vec<(String, String, String)> = Vec::new();
    let mut state_bytes: usize = 0;
    let push_capped = |dst: &mut Vec<(String, String, String)>, budget: &mut usize, name: String, key: String, val: String| {
        if val.len() <= MAX_STATE_VALUE_BYTES && budget.saturating_add(val.len()) <= MAX_TOTAL_STATE_BYTES {
            *budget += val.len();
            dst.push((name, key, val));
        }
    };

    let mut imported_server_urls = HashSet::new();
    let mut has_server = false;
    if has_state {
        if let Some(v3) = official
            .query_row("SELECT value FROM state WHERE name = 'default.dat' AND key = 'settings.v3'", [], |r| r.get::<_, String>(0))
            .optional()
            .map_err(|e| format!("read default.dat/settings.v3: {e}"))?
        {
            push_capped(&mut global_rows, &mut state_bytes, "opencode.global.dat".into(), "settings.v3".into(), v3);
        }

        let mut stmt = official
            .prepare("SELECT key, value FROM state WHERE name = 'opencode.global.dat'")
            .map_err(|e| format!("prepare opencode.global.dat: {e}"))?;
        let found = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| format!("query opencode.global.dat: {e}"))?;
        for item in found {
            let (k, v) = item.map_err(|e| format!("read opencode.global.dat row: {e}"))?;
            if k == "server" {
                if let Some((scrubbed, urls)) = scrub_server_json(&v) {
                    imported_server_urls = urls;
                    has_server = true;
                    push_capped(&mut global_rows, &mut state_bytes, "opencode.global.dat".into(), "server".into(), scrubbed);
                }
            } else if GLOBAL_ALLOW.contains(&k.as_str()) {
                push_capped(&mut global_rows, &mut state_bytes, "opencode.global.dat".into(), k, v);
            }
        }
    }

    let mut window_ids: Vec<String> = Vec::new();
    for id in settings_val["windowIds"].as_array().into_iter().flatten().filter_map(Value::as_str) {
        if is_safe_window_id(id) && !window_ids.iter().any(|x| x == id) {
            window_ids.push(id.to_owned());
            if window_ids.len() >= MAX_SEED_WINDOWS {
                break;
            }
        }
    }
    if window_ids.is_empty() && has_state {
        if let Some(name) = official
            .query_row(
                "SELECT name FROM state WHERE name LIKE 'opencode.window.%.dat' GROUP BY name ORDER BY MAX(updated_at) DESC LIMIT 1",
                [],
                |r| r.get::<_, String>(0),
            )
            .optional()
            .map_err(|e| format!("query newest window namespace: {e}"))?
            && let Some(id) = name.strip_prefix("opencode.window.").and_then(|s| s.strip_suffix(".dat"))
            && is_safe_window_id(id)
        {
            window_ids.push(id.to_owned());
        }
    }

    if let Some(def_url) = settings_val["defaultServerUrl"].as_str().and_then(strip_url_userinfo) {
        let norm = def_url.trim_end_matches('/');
        if imported_server_urls.contains(norm) {
            push_capped(&mut global_rows, &mut state_bytes, "opencode.settings".into(), "defaultServerUrl".into(), json!(def_url).to_string());
        }
    }
    if !window_ids.is_empty() {
        push_capped(&mut window_rows, &mut state_bytes, "opencode.settings".into(), "windowIds".into(), json!(&window_ids).to_string());
    }
    if let Some(z) = settings_val["zoomLevel"].as_f64().filter(|z| z.is_finite() && (0.25..=4.0).contains(z)) {
        push_capped(&mut global_rows, &mut state_bytes, "opencode.global.dat".into(), "zoom".into(), json!(z).to_string());
    }

    // Pass 1 over windows: load allow-listed window state rows and collect open session IDs so
    // `load_session_docs` queries ONLY open sessions (never full historical sessions).
    let mut per_window: Vec<(String, String, Vec<(String, String)>)> = Vec::with_capacity(window_ids.len());
    let mut open_sessions: HashSet<String> = HashSet::new();
    for wid in &window_ids {
        let ns = crate::window_namespace(wid);
        let mut win_rows: Vec<(String, String)> = Vec::new();
        if has_state {
            let mut stmt = official
                .prepare("SELECT key, value FROM state WHERE name = ?1")
                .map_err(|e| format!("prepare {ns}: {e}"))?;
            let found = stmt
                .query_map([&ns], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
                .map_err(|e| format!("query {ns}: {e}"))?;
            for item in found {
                let (k, v) = item.map_err(|e| format!("read {ns} row: {e}"))?;
                if WINDOW_ALLOW.contains(&k.as_str()) {
                    if k == "tabs"
                        && let Ok(Value::Array(arr)) = serde_json::from_str::<Value>(&v)
                    {
                        for raw_tab in arr {
                            if let Some(Tab::Session(sess)) = Tab::from_json(&raw_tab) {
                                open_sessions.insert(sess.session_id);
                            }
                        }
                    }
                    win_rows.push((k, v));
                }
            }
        }
        per_window.push((wid.clone(), ns, win_rows));
    }

    let session_docs = if has_doc { load_session_docs(official, &open_sessions)? } else { std::collections::HashMap::new() };
    let mut window_docs: Vec<(String, String)> = Vec::new();
    let mut window_blobs: Vec<(String, Vec<u8>)> = Vec::new();
    let mut blob_budget = MAX_SEED_TOTAL_BLOBS_BYTES;
    let mut frames: Vec<(String, WindowFrame)> = Vec::new();
    let mut dropped_drafts = 0usize;

    // Pass 2 over windows: convert drafts per window (tracking dropped drafts strictly per window).
    for (wid, ns, mut win_rows) in per_window {
        let mut surviving_tabs: Vec<Tab> = Vec::new();
        let mut win_dropped = 0usize;
        if let Some((_, tabs_raw)) = win_rows.iter_mut().find(|(k, _)| k == "tabs")
            && let Ok(Value::Array(arr)) = serde_json::from_str::<Value>(tabs_raw)
        {
            let mut kept_raw = Vec::with_capacity(arr.len());
            for raw_tab in arr {
                let Some(tab) = Tab::from_json(&raw_tab) else {
                    kept_raw.push(raw_tab);
                    continue;
                };
                match &tab {
                    Tab::Draft(draft) => {
                        let official_key = official_draft_doc_key(&draft.draft_id);
                        let mut draft_blobs = Vec::new();
                        let mut draft_budget = blob_budget;
                        let doc_res = if has_doc {
                            read_official_doc(official, &official_key, has_blobs).and_then(|opt| match opt {
                                Some(val) => convert_composer_value(&val, official, has_blobs, &mut draft_blobs, &mut draft_budget),
                                None => Ok(None),
                            })
                        } else {
                            Ok(None)
                        };
                        match doc_res {
                            Ok(Some(rune_doc)) => {
                                blob_budget = draft_budget;
                                window_blobs.extend(draft_blobs);
                                window_docs.push((crate::draft_document(&wid, &tab.key().encode()), rune_doc));
                                surviving_tabs.push(tab);
                                kept_raw.push(raw_tab);
                            }
                            Ok(None) => {
                                surviving_tabs.push(tab);
                                kept_raw.push(raw_tab);
                            }
                            Err(why) => {
                                win_dropped += 1;
                                notes.push(format!("dropped draft tab {} (draft could not be migrated: {why})", draft.draft_id));
                            }
                        }
                    }
                    Tab::Session(sess) => {
                        if let Some(raw_doc) = session_docs.get(&sess.session_id) {
                            let mut draft_blobs = Vec::new();
                            let mut draft_budget = blob_budget;
                            match parse_official_doc_strict(raw_doc, official, has_blobs)
                                .and_then(|val| convert_composer_value(&val, official, has_blobs, &mut draft_blobs, &mut draft_budget))
                            {
                                Ok(Some(rune_doc)) => {
                                    blob_budget = draft_budget;
                                    window_blobs.extend(draft_blobs);
                                    window_docs.push((crate::draft_document(&wid, &tab.key().encode()), rune_doc));
                                }
                                Ok(None) => {}
                                Err(why) => {
                                    win_dropped += 1;
                                    notes.push(format!("skipped unsent draft for session {} ({why})", sess.session_id));
                                }
                            }
                        }
                        surviving_tabs.push(tab);
                        kept_raw.push(raw_tab);
                    }
                }
            }
            if win_dropped > 0 {
                *tabs_raw = Value::Array(kept_raw).to_string();
                dropped_drafts += win_dropped;
            }
        }

        if let Some(url_json) = derived_last_active_url(&win_rows, &surviving_tabs) {
            win_rows.push(("lastActiveUrl".into(), url_json));
        }
        for (k, v) in win_rows {
            push_capped(&mut window_rows, &mut state_bytes, ns.clone(), k, v);
        }

        match read_capped(&dir.join(format!("window-state-{wid}.json")), MAX_WINDOW_STATE_BYTES) {
            Ok(Some(raw)) => {
                if let Some(frame) = serde_json::from_str::<Value>(&raw).ok().and_then(|v| WindowFrame::from_json(&v)) {
                    frames.push((wid, frame));
                }
            }
            Ok(None) => {}
            Err(e) => notes.push(format!("skipped window-state-{wid}.json: {e}")),
        }
    }

    let mut global_docs: Vec<(String, String)> = Vec::new();
    if has_doc && let Some(hist) = convert_prompt_history(official, has_blobs) {
        global_docs.push((crate::PROMPT_HISTORY_DOCUMENT.into(), hist));
    }

    Ok(SeedBundle {
        global_rows,
        window_rows,
        global_docs,
        window_docs,
        window_blobs,
        frames,
        window_count: window_ids.len(),
        has_server,
        dropped_drafts,
        notes,
    })
}

/// What the seed read from Official Desktop, before any of it is written: the first half of the
/// seed, which needs no database of ours. On a new profile the open reads it before it makes the
/// database, so the first frame is built from it ([`Collected::packet`]) without waiting for the
/// file, its schema or the commit.
pub(crate) enum Collected {
    /// No Official Desktop database in any candidate folder.
    NoSource,
    /// One was there and couldn't be read or verified cleanly.
    Failed(String),
    Bundle { dir: PathBuf, bundle: Box<SeedBundle> },
}

impl Collected {
    /// The boot packet the database will hold once this is committed into a new profile: the
    /// same entries `boot_record::rebuild` writes from the rows. None when there is nothing to seed.
    pub(crate) fn packet(&self) -> Option<crate::BootPacket> {
        let Collected::Bundle { bundle, .. } = self else { return None };
        let mut cache = crate::cache::Cache { complete: true, ..Default::default() };
        for (name, key, value) in bundle.global_rows.iter().chain(&bundle.window_rows) {
            // An existing row wins in the commit (`ON CONFLICT DO NOTHING`): the first of a key here.
            if matches!(cache.get(name, key), crate::Lookup::Ready(None)) {
                cache.set(name, key.clone(), Some(value.as_str().into()));
            }
        }
        for (key, value) in bundle.global_docs.iter().chain(&bundle.window_docs) {
            cache.docs.entry(key.clone()).or_insert_with(|| crate::cache::Doc::Present(value.as_str().into()));
        }
        for (id, frame) in &bundle.frames {
            cache.windows.entry(id.clone()).or_insert(*frame);
        }
        Some(boot_record::from_cache(&cache))
    }
}

/// Reads what the seed takes from the newest Official Desktop database among `dirs`: in place
/// when nobody is writing it, else (or when it was written to during the read) from a verified
/// copy under `scratch_parent`.
pub(crate) fn collect(dirs: &[PathBuf], scratch_parent: &Path) -> Collected {
    let mut failed = String::new();
    for in_place in [true, false] {
        let (dir, snap) = match pick_dir(dirs, scratch_parent, in_place) {
            PickResult::None => return Collected::NoSource,
            PickResult::Failed(why) => return Collected::Failed(why),
            PickResult::Picked(d, s) => (d, s),
        };
        let bundle = collect_bundle(&dir, &snap);
        let held = snap.held();
        // Drop (and delete a scratch copy) before anything is written into Rune's DB.
        drop(snap);
        match bundle {
            Ok(bundle) if held => return Collected::Bundle { dir, bundle: Box::new(bundle) },
            Err(why) if held => return Collected::Failed(why),
            _ => failed = format!("{}: drafts.sqlite was written to while it was read", dir.display()),
        }
    }
    Collected::Failed(failed)
}

/// Writes what [`collect`] read inside ONE atomic transaction (migrated rows, drafts, blobs,
/// frames, boot record, and `settle_flags` commit together).
pub(crate) fn commit(conn: &mut Connection, collected: Collected, settle_flags: u32, now_ms: i64) -> SeedOutcome {
    let (dir, bundle) = match collected {
        Collected::NoSource => {
            let _ = (|| -> rusqlite::Result<()> {
                let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
                tx.execute("DELETE FROM state WHERE name = ?1 AND key = ?2", [META_NS, ATTEMPTS_KEY])?;
                schema::set_flags(&tx, settle_flags)?;
                tx.commit()
            })();
            return SeedOutcome::NoSource;
        }
        Collected::Failed(why) => return record_attempt(conn, settle_flags, now_ms, why),
        Collected::Bundle { dir, bundle } => (dir, *bundle),
    };

    let is_retry = prior_attempts(conn) > 0;
    let commit = (|| -> rusqlite::Result<(Vec<(String, String, String)>, bool)> {
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if is_retry {
            clear_blank_retry_defaults(&tx, &bundle)?;
        }
        // If this is a retry and Rune already holds user windows (user opened tabs on Launch 1),
        // skip Official's window rows, frames, and per-window draft docs/blobs so no orphan
        // `opencode.window.*` rows bloat the boot record or pin blobs forever.
        let import_windows = !is_retry
            || tx
                .query_row("SELECT 1 FROM state WHERE name = 'opencode.settings' AND key = 'windowIds'", [], |_| Ok(()))
                .optional()?
                .is_none();

        let mut inserted = Vec::new();
        for (key, value) in &bundle.global_docs {
            let (stored, chunks) = sql::encode_text(value);
            for (cid, cdata) in &chunks {
                sql::insert_blob(&tx, cid, cdata, now_ms)?;
            }
            tx.execute("INSERT INTO document (key, value) VALUES (?1, ?2) ON CONFLICT DO NOTHING", (key, stored))?;
        }
        for (name, key, value) in &bundle.global_rows {
            if tx.execute("INSERT INTO state (name, key, value, updated_at) VALUES (?1, ?2, ?3, ?4) ON CONFLICT DO NOTHING", (name, key, value, now_ms))? > 0 {
                inserted.push((name.clone(), key.clone(), value.clone()));
            }
        }
        if import_windows {
            for (id, data) in &bundle.window_blobs {
                sql::insert_blob(&tx, id, data, now_ms)?;
            }
            for (key, value) in &bundle.window_docs {
                let (stored, chunks) = sql::encode_text(value);
                for (cid, cdata) in &chunks {
                    sql::insert_blob(&tx, cid, cdata, now_ms)?;
                }
                tx.execute("INSERT INTO document (key, value) VALUES (?1, ?2) ON CONFLICT DO NOTHING", (key, stored))?;
            }
            for (name, key, value) in &bundle.window_rows {
                if tx.execute("INSERT INTO state (name, key, value, updated_at) VALUES (?1, ?2, ?3, ?4) ON CONFLICT DO NOTHING", (name, key, value, now_ms))? > 0 {
                    inserted.push((name.clone(), key.clone(), value.clone()));
                }
            }
            for (id, frame) in &bundle.frames {
                if tx.query_row("SELECT 1 FROM window_state WHERE id = ?1", [id], |_| Ok(())).optional()?.is_none() {
                    sql::put_window(&tx, id, Some(frame), now_ms)?;
                }
            }
        }
        tx.execute("DELETE FROM state WHERE name = ?1 AND key = ?2", [META_NS, ATTEMPTS_KEY])?;
        boot_record::rebuild(&tx)?;
        schema::set_flags(&tx, settle_flags)?;

        // Opt-in crash-recovery proof hook (strictly requires RUNE_DIRS_ROOT so it can never fire
        // on a real user profile): terminates before tx.commit() so SQLite rolls back atomically.
        if std::env::var_os("RUNE_DIRS_ROOT").is_some_and(|v| !v.is_empty())
            && std::env::var_os("RUNE_TEST_SEED_CRASH_BEFORE_COMMIT").is_some_and(|v| v == "1")
        {
            std::process::exit(99);
        }

        tx.commit()?;
        Ok((inserted, import_windows))
    })();

    match commit {
        Ok((inserted, imported_windows)) => {
            let dropped = if imported_windows { bundle.dropped_drafts } else { 0 };
            let mut report = bundle.notes;
            report.push(format!(
                "seeded {} state rows, {} docs, {} blobs, {} frames ({} windows, imported_windows={}, {} dropped drafts) from {}",
                inserted.len(),
                bundle.global_docs.len() + if imported_windows { bundle.window_docs.len() } else { 0 },
                if imported_windows { bundle.window_blobs.len() } else { 0 },
                if imported_windows { bundle.frames.len() } else { 0 },
                bundle.window_count,
                imported_windows,
                dropped,
                dir.display()
            ));
            SeedOutcome::Seeded { rows: inserted, dropped_drafts: dropped, source_dir: dir, report }
        }
        Err(e) => record_attempt(conn, settle_flags, now_ms, format!("commit failed: {e}")),
    }
}
