//! Page downloads for both human users and agent-driven tabs (`notes/review-rd-engine.md` §4):
//!
//! - **Human downloads** (`!agent_driven`) and explicit right-click **"Save image as…"**
//!   (`SaveImage`) NEVER write to `opencode-page-downloads` and NEVER enter `BrowserTab.files`.
//!   Instead, the engine holds CEF's `BeforeDownloadCallback` and asks the host app
//!   (`EngineMsg::DownloadSaveRequest`) to present a native OS Save dialog (`HostMsg::AnswerDownloadSave`),
//!   rejecting any destination inside the engine profile or download staging directories.
//! - **Agent downloads** (`agent_driven && !SaveImage`) go into private staging folders (`0o700` on
//!   Unix), capped at 5 MiB (`MAX_FILE_BYTES`) and 64 per tab (`MAX_TAB_FILES`), deleted on tab
//!   close and engine exit:
//!   - `Default` context -> `<profile>/opencode-page-downloads/<tab>/<file_id>/<safe_name>`
//!   - Isolated contexts (`RemoteServer`, `Private`) -> `<lasting-profile>-isolated-downloads/pid-<pid>/<tab>/<file_id>/<safe_name>`
//!     (strictly off the shared profile disk; lazily created, locked, and swept on exit/restart).
//! - **`Preview` and `Artifact` tabs** are completely blocked from downloading.
//! - **URL policy & executable guards** run in both `can_download` and `on_before_download`
//!   (covering `start_download` and redirects), stripping Unicode bidi overrides, bounding names,
//!   blocking executable downloads on agent tabs & `SaveImage`, and neutralizing suggested names
//!   for human Save dialogs with `.download`.
//! - **Per-tab-instance lifetimes (`tab_gen` + `tab_alive`)** replace the permanent `closed` tab-ID
//!   set, preventing `tab_id` reuse collisions across reconnects / Linux `ReopenTabs` and pruning
//!   `files` entries on `close_tab`.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cef::rc::Rc as _;
use cef::{
    BeforeDownloadCallback, Browser, CefString, DownloadHandler, DownloadInterruptReason,
    DownloadItem, DownloadItemCallback, ImplBeforeDownloadCallback, ImplBrowser,
    ImplDownloadHandler, ImplDownloadItem, ImplDownloadItemCallback, ImplFrame,
    WrapDownloadHandler, wrap_download_handler,
};

use crate::output::Output;
use crate::prompts::TabPrompts;
use crate::wire::{DownloadFile, DownloadState, EngineMsg};

/// The default-context agent downloads folder name in the engine's profile.
pub const DIR_NAME: &str = "opencode-page-downloads";
/// Marker proving the engine created `opencode-page-downloads`.
const MARKER: &str = ".opencode-page-downloads";
/// Marker proving the engine created an isolated downloads `pid-<pid>` folder.
const ISOLATED_MARKER: &str = ".opencode-isolated-downloads";
/// Desktop's limit on a file the agent's tools take (`plugin-browser/src/rpc.ts`).
pub const MAX_FILE_BYTES: u64 = 5 * 1024 * 1024;
/// A tab's agent files at most: a page that keeps starting downloads stops at this many.
pub const MAX_TAB_FILES: usize = 64;
/// Maximum concurrent pending OS Save dialogs per tab instance.
const MAX_PENDING_SAVES_PER_TAB: usize = 4;
/// Maximum concurrent pending OS Save dialogs across the entire engine.
const MAX_PENDING_SAVES_TOTAL: usize = 16;
/// Maximum queued `SaveImage` URL markers per tab instance.
const MAX_PENDING_SAVE_IMAGES: usize = 8;
/// How long a right-click `SaveImage` marker remains valid before expiring.
const SAVE_IMAGE_TTL: Duration = Duration::from_secs(30);

/// Profile and sibling isolated-downloads roots handed from `CefRuntime` to `Downloads::open`.
#[derive(Clone, Debug)]
pub struct DownloadRoots {
    pub profile: PathBuf,
    pub lasting_profile: PathBuf,
    pub isolated_base: PathBuf,
}

struct IsolatedStore {
    base: PathBuf,
    dir: Option<PathBuf>,
    _lock: Option<File>,
}

struct PendingSave {
    tab_gen: u64,
    item_id: u32,
    tab_alive: Arc<AtomicBool>,
    callback: BeforeDownloadCallback,
}

/// Engine-wide download state across all tabs and `CefRequestContext`s.
pub struct Downloads {
    /// Forbidden root directories that human `AnswerDownloadSave` paths must never land inside.
    forbidden_roots: Vec<PathBuf>,
    /// `None` when `opencode-page-downloads` exists without our marker or cannot be created.
    default_dir: Option<PathBuf>,
    /// Lazily initialized sibling directory `<lasting>-isolated-downloads/pid-<pid>` for isolated
    /// agent tabs (`RemoteServer`, `Private`) so isolated downloads never touch `<profile>`.
    isolated: Mutex<IsolatedStore>,
    /// Whether the connected host advertised `Hello.download_save == true`.
    download_save_enabled: AtomicBool,
    next_file: AtomicU64,
    next_save_req: AtomicU64,
    next_tab_gen: AtomicU64,
    /// Agent downloads keyed by `(tab_id, tab_gen, chromium_item_id)`.
    files: Mutex<HashMap<(u64, u64, u32), DownloadFile>>,
    /// Active human-approved downloads keyed by `(tab_id, tab_gen, chromium_item_id)` so we cancel
    /// them on tab close / context suspend without exposing them in `files` or enforcing 5 MiB.
    user_saves: Mutex<HashSet<(u64, u64, u32)>>,
    /// Held human `BeforeDownloadCallback`s awaiting `HostMsg::AnswerDownloadSave`.
    pending_saves: Mutex<HashMap<(u64, u64), PendingSave>>,
    /// Verified right-click `SaveImage` URLs keyed by `(tab_id, tab_gen)`.
    pending_save_images: Mutex<HashMap<(u64, u64), VecDeque<(String, Instant)>>>,
}

impl Downloads {
    /// Initializes default agent downloads under `roots.profile` and sweeps dead isolated staging
    /// folders under `roots.isolated_base` without creating `isolated_base` until needed.
    pub fn open(roots: &DownloadRoots) -> Arc<Self> {
        let default_dir = roots.profile.join(DIR_NAME);
        remove_owned(&roots.profile);
        sweep_dead_isolated(&roots.isolated_base);
        let made = create_owned_dir(&default_dir, MARKER);
        let mut forbidden = vec![
            normalize_root(&roots.profile),
            normalize_root(&roots.lasting_profile),
            normalize_root(&roots.isolated_base),
            normalize_root(&crate::profile::scratch_dir(&roots.lasting_profile)),
        ];
        forbidden.retain(|p| !p.as_os_str().is_empty());
        Arc::new(Self {
            forbidden_roots: forbidden,
            default_dir: made.then_some(default_dir),
            isolated: Mutex::new(IsolatedStore {
                base: roots.isolated_base.clone(),
                dir: None,
                _lock: None,
            }),
            download_save_enabled: AtomicBool::new(false),
            next_file: AtomicU64::new(1),
            next_save_req: AtomicU64::new(1),
            next_tab_gen: AtomicU64::new(1),
            files: Mutex::new(HashMap::new()),
            user_saves: Mutex::new(HashSet::new()),
            pending_saves: Mutex::new(HashMap::new()),
            pending_save_images: Mutex::new(HashMap::new()),
        })
    }

    pub fn alloc_tab_gen(&self) -> u64 {
        self.next_tab_gen.fetch_add(1, Ordering::Relaxed)
    }

    /// Whether `tab_id` currently owns any staged agent download files on disk.
    pub fn has_tab_files(&self, tab_id: u64) -> bool {
        self.files.lock().unwrap().keys().any(|&(tid, _, _)| tid == tab_id)
    }

    pub fn set_download_save_enabled(&self, enabled: bool) {
        self.download_save_enabled.store(enabled, Ordering::Release);
    }

    pub fn download_save_enabled(&self) -> bool {
        self.download_save_enabled.load(Ordering::Acquire)
    }

    /// Records a verified right-click `SaveImage` URL so `on_before_download` always routes it to
    /// the human OS Save dialog even on an `agent_driven` tab.
    pub fn mark_pending_save_image(&self, tab_id: u64, tab_gen: u64, url: &str) {
        let now = Instant::now();
        let norm = strip_url_fragment(url);
        let mut map = self.pending_save_images.lock().unwrap();
        let queue = map.entry((tab_id, tab_gen)).or_default();
        while queue.front().is_some_and(|(_, at)| now.duration_since(*at) > SAVE_IMAGE_TTL) {
            queue.pop_front();
        }
        if queue.len() >= MAX_PENDING_SAVE_IMAGES {
            queue.pop_front();
        }
        queue.push_back((norm.to_owned(), now));
    }

    fn take_pending_save_image(&self, tab_id: u64, tab_gen: u64, orig_url: &str, url: &str) -> bool {
        let now = Instant::now();
        let orig = strip_url_fragment(orig_url);
        let cur = strip_url_fragment(url);
        let mut map = self.pending_save_images.lock().unwrap();
        let Some(queue) = map.get_mut(&(tab_id, tab_gen)) else {
            return false;
        };
        while queue.front().is_some_and(|(_, at)| now.duration_since(*at) > SAVE_IMAGE_TTL) {
            queue.pop_front();
        }
        let pos = queue
            .iter()
            .position(|(u, _)| (!orig.is_empty() && u == orig) || u == cur);
        let matched = pos.map(|i| queue.remove(i)).is_some();
        if queue.is_empty() {
            map.remove(&(tab_id, tab_gen));
        }
        matched
    }

    /// Lazily creates `<isolated_base>/pid-<pid>` with lock + marker and `0o700` permissions.
    fn ensure_isolated_dir(&self) -> Option<PathBuf> {
        let mut iso = self.isolated.lock().unwrap();
        if let Some(dir) = &iso.dir {
            return Some(dir.clone());
        }
        if !ensure_safe_dir(&iso.base) {
            return None;
        }
        let dir = iso.base.join(format!("pid-{}", std::process::id()));
        if dir.exists() && !is_owned_dir(&dir, ISOLATED_MARKER) {
            return None;
        }
        if std::fs::create_dir_all(&dir).is_err() {
            return None;
        }
        restrict_dir_mode(&dir);
        let lock = crate::profile::acquire_lock(&dir)?;
        if std::fs::write(dir.join(ISOLATED_MARKER), b"").is_err() {
            return None;
        }
        iso.dir = Some(dir.clone());
        iso._lock = Some(lock);
        Some(dir)
    }

    /// Resolves a held human Save dialog callback (`HostMsg::AnswerDownloadSave`). Returns whether
    /// it answered a held request: the download then has its place, or was cancelled.
    pub fn answer_save(&self, tab_id: u64, id: u64, path: Option<&str>, context_ready: bool) -> bool {
        let Some(pending) = self.pending_saves.lock().unwrap().remove(&(tab_id, id)) else {
            return false;
        };
        if !context_ready || !pending.tab_alive.load(Ordering::Acquire) {
            // Drop `pending.callback` without calling `.cont()` -> cancels in CEF.
            return true;
        }
        let Some(target) = path.and_then(|raw| self.validate_user_save_path(raw)) else {
            return true;
        };
        let Some(target_str) = target.to_str() else {
            return true;
        };
        self.user_saves.lock().unwrap().insert((tab_id, pending.tab_gen, pending.item_id));
        pending.callback.cont(Some(&CefString::from(target_str)), 0);
        true
    }

    fn validate_user_save_path(&self, raw: &str) -> Option<PathBuf> {
        if raw.is_empty() || raw.contains('\0') || browser_policy::is_unc_or_device_path(raw) {
            return None;
        }
        let path = Path::new(raw);
        if !path.is_absolute() {
            return None;
        }
        let file_name = path.file_name()?.to_str()?;
        if file_name.is_empty()
            || file_name == "."
            || file_name == ".."
            || file_name.chars().any(|c| browser_policy::is_bidi_control(c) || c.is_control())
        {
            return None;
        }
        #[cfg(windows)]
        if file_name.contains(':') {
            return None;
        }
        let stem = file_name.split('.').next().unwrap_or_default().trim_end();
        if browser_policy::is_windows_device_name(stem) {
            return None;
        }
        let parent = path.parent()?;
        let parent_meta = std::fs::metadata(parent).ok()?;
        if !parent_meta.is_dir() {
            return None;
        }
        let canon_parent = std::fs::canonicalize(parent).ok()?;
        let clean_parent = strip_verbatim_prefix(&canon_parent)?;
        for forbidden in &self.forbidden_roots {
            if path_within_os(&clean_parent, forbidden) {
                return None;
            }
        }
        let target = clean_parent.join(file_name);
        if let Ok(meta) = std::fs::symlink_metadata(&target)
            && (meta.is_dir() || is_reparse_or_symlink(&meta))
        {
            return None;
        }
        Some(target)
    }

    /// Cancels all held Save dialogs for `tab_id` (e.g. when a `RemoteServer` proxy suspends).
    pub fn cancel_pending_saves_for_tab(&self, tab_id: u64) {
        self.pending_saves.lock().unwrap().retain(|&(tid, _), _| tid != tab_id);
    }

    /// Cancels all held Save dialogs and disables `download_save` when the host UI disconnects.
    pub fn cancel_all_pending_saves(&self) {
        self.download_save_enabled.store(false, Ordering::Release);
        self.pending_saves.lock().unwrap().clear();
    }

    /// Tab instance `(tab_id, tab_gen)` closed: prunes its `files` entries, cancels its pending
    /// Save dialogs, removes its staged files, and removes empty `<tab_id>` staging folders.
    pub fn close_tab(&self, tab_id: u64, tab_gen: u64) {
        let mut to_remove: Vec<String> = Vec::new();
        self.files.lock().unwrap().retain(|&(tid, g, _), file| {
            if tid == tab_id && g == tab_gen {
                to_remove.push(file.path.clone());
                false
            } else {
                true
            }
        });
        for path in &to_remove {
            remove_file_folder(path);
        }
        self.user_saves
            .lock()
            .unwrap()
            .retain(|&(tid, g, _)| !(tid == tab_id && g == tab_gen));
        self.pending_saves
            .lock()
            .unwrap()
            .retain(|&(tid, _), p| !(tid == tab_id && p.tab_gen == tab_gen));
        self.pending_save_images.lock().unwrap().remove(&(tab_id, tab_gen));
        if let Some(dir) = &self.default_dir {
            let _ = std::fs::remove_dir(dir.join(tab_id.to_string()));
        }
        if let Some(dir) = self.isolated.lock().unwrap().dir.as_ref() {
            let _ = std::fs::remove_dir(dir.join(tab_id.to_string()));
        }
    }

    /// Drops every held CEF `BeforeDownloadCallback` and clears tracked state before `cef::shutdown`.
    pub fn close_all(&self) {
        self.download_save_enabled.store(false, Ordering::Release);
        self.pending_saves.lock().unwrap().clear();
        self.pending_save_images.lock().unwrap().clear();
        self.user_saves.lock().unwrap().clear();
        let drained: Vec<String> = self
            .files
            .lock()
            .unwrap()
            .drain()
            .map(|(_, f)| f.path)
            .collect();
        for path in &drained {
            remove_file_folder(path);
        }
        // Release the isolated-dir lock so `CefRuntime::drop` can delete `pid-<pid>` on Windows.
        let mut iso = self.isolated.lock().unwrap();
        iso._lock = None;
    }
}

/// Removes `profile`'s `opencode-page-downloads` folder only when it is not a symlink/reparse
/// point and holds our marker.
pub fn remove_owned(profile: &Path) {
    let dir = profile.join(DIR_NAME);
    if is_owned_dir(&dir, MARKER) {
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Removes this engine's `<isolated_base>/pid-<pid>` folder when owned, then prunes empty base.
pub fn remove_owned_isolated(isolated_base: &Path, pid: u32) {
    let dir = isolated_base.join(format!("pid-{pid}"));
    if is_owned_dir(&dir, ISOLATED_MARKER) {
        let _ = std::fs::remove_dir_all(&dir);
    }
    if std::fs::symlink_metadata(isolated_base).is_ok_and(|m| m.is_dir() && !is_reparse_or_symlink(&m)) {
        let _ = std::fs::remove_dir(isolated_base);
    }
}

fn sweep_dead_isolated(isolated_base: &Path) {
    if !std::fs::symlink_metadata(isolated_base).is_ok_and(|m| m.is_dir() && !is_reparse_or_symlink(&m)) {
        return;
    }
    let Ok(entries) = std::fs::read_dir(isolated_base) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let Some(pid) = name
            .to_str()
            .and_then(|n| n.strip_prefix("pid-"))
            .and_then(|p| p.parse::<i32>().ok())
        else {
            continue;
        };
        if is_owned_dir(&path, ISOLATED_MARKER) && crate::profile::is_pid_dead(&path, pid) {
            let _ = std::fs::remove_dir_all(&path);
        }
    }
    let _ = std::fs::remove_dir(isolated_base);
}

fn create_owned_dir(dir: &Path, marker: &str) -> bool {
    if dir.exists() {
        return false;
    }
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    restrict_dir_mode(dir);
    if !std::fs::symlink_metadata(dir).is_ok_and(|m| m.is_dir() && !is_reparse_or_symlink(&m)) {
        return false;
    }
    std::fs::write(dir.join(marker), b"").is_ok()
}

fn ensure_safe_dir(dir: &Path) -> bool {
    if dir.exists() {
        return std::fs::symlink_metadata(dir).is_ok_and(|m| m.is_dir() && !is_reparse_or_symlink(&m));
    }
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    restrict_dir_mode(dir);
    std::fs::symlink_metadata(dir).is_ok_and(|m| m.is_dir() && !is_reparse_or_symlink(&m))
}

fn is_owned_dir(dir: &Path, marker: &str) -> bool {
    let Ok(meta) = std::fs::symlink_metadata(dir) else {
        return false;
    };
    if !meta.is_dir() || is_reparse_or_symlink(&meta) {
        return false;
    }
    let Ok(marker_meta) = std::fs::symlink_metadata(dir.join(marker)) else {
        return false;
    };
    marker_meta.is_file() && !is_reparse_or_symlink(&marker_meta)
}

fn is_reparse_or_symlink(meta: &std::fs::Metadata) -> bool {
    if meta.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        if (meta.file_attributes() & 0x0400) != 0 {
            return true;
        }
    }
    false
}

#[cfg(unix)]
fn restrict_dir_mode(path: &Path) {
    use std::os::unix::fs::PermissionsExt as _;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700));
}

#[cfg(not(unix))]
fn restrict_dir_mode(_path: &Path) {}

fn strip_verbatim_prefix(canon: &Path) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let s = canon.to_str()?;
        if s.starts_with(r"\\?\UNC\") || s.starts_with(r"\\?\unc\") {
            return None;
        }
        if let Some(rest) = s.strip_prefix(r"\\?\") {
            return Some(PathBuf::from(rest));
        }
        Some(canon.to_path_buf())
    }
    #[cfg(not(windows))]
    {
        Some(canon.to_path_buf())
    }
}

/// Canonicalizes `dir` if it exists; otherwise canonicalizes its existing parent and rejoins the leaf.
fn normalize_root(dir: &Path) -> PathBuf {
    let raw = if let Ok(c) = std::fs::canonicalize(dir) {
        c
    } else if let (Some(parent), Some(name)) = (dir.parent(), dir.file_name())
        && let Ok(c_parent) = std::fs::canonicalize(parent)
    {
        c_parent.join(name)
    } else {
        dir.to_path_buf()
    };
    strip_verbatim_prefix(&raw).unwrap_or(raw)
}

fn path_within_os(candidate: &Path, root: &Path) -> bool {
    #[cfg(any(windows, target_os = "macos"))]
    {
        let mut cand = candidate.components();
        for rc in root.components() {
            let Some(cc) = cand.next() else {
                return false;
            };
            if !cc
                .as_os_str()
                .to_string_lossy()
                .eq_ignore_ascii_case(&rc.as_os_str().to_string_lossy())
            {
                return false;
            }
        }
        true
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        candidate.starts_with(root)
    }
}

/// Removes a staged file and its own `<file_id>` folder (`<dir>/<tab>/<file_id>`).
fn remove_file_folder(path: &str) {
    if path.is_empty() {
        return;
    }
    let path = Path::new(path);
    let _ = std::fs::remove_file(path);
    if let Some(folder) = path.parent() {
        let _ = std::fs::remove_dir(folder);
    }
}

fn strip_url_fragment(url: &str) -> &str {
    url.split('#').next().unwrap_or(url)
}

wrap_download_handler! {
    pub struct TabDownloadHandler {
        tab_id: u64,
        tab_gen: u64,
        tab_alive: Arc<AtomicBool>,
        block_downloads: bool,
        is_isolated: bool,
        context_ready: Arc<AtomicBool>,
        prompts: Arc<TabPrompts>,
        downloads: Arc<Downloads>,
        out_tx: Output,
    }

    impl DownloadHandler {
        fn can_download(&self, _browser: Option<&mut Browser>, url: Option<&CefString>, _method: Option<&CefString>) -> ::std::os::raw::c_int {
            if self.block_downloads
                || !self.tab_alive.load(Ordering::Acquire)
                || !self.context_ready.load(Ordering::Acquire)
            {
                return 0;
            }
            let url = url.map(CefString::to_string).unwrap_or_default();
            if browser_policy::extension_origin(&url) {
                return 0;
            }
            i32::from(browser_policy::download_url_allowed(&url))
        }

        // Always returns `1` (handler handled): calling `callback.cont()` proceeds, whereas
        // returning `1` without calling `callback.cont()` (or dropping the held callback) explicitly
        // cancels in both Alloy and Chrome styles.
        fn on_before_download(
            &self,
            browser: Option<&mut Browser>,
            item: Option<&mut DownloadItem>,
            suggested_name: Option<&CefString>,
            callback: Option<&mut BeforeDownloadCallback>,
        ) -> ::std::os::raw::c_int {
            if self.block_downloads
                || !self.tab_alive.load(Ordering::Acquire)
                || !self.context_ready.load(Ordering::Acquire)
            {
                return 1;
            }
            let (Some(item), Some(callback)) = (item, callback) else { return 1 };
            let url = CefString::from(&item.url()).to_string();
            let orig_url = CefString::from(&item.original_url()).to_string();
            // Re-check URL policy here because `BrowserHost::start_download` bypasses `can_download`
            // and HTTP downloads may redirect to a disallowed scheme or credentialed URL.
            if browser_policy::extension_origin(&url)
                || browser_policy::extension_origin(&orig_url)
                || !browser_policy::download_url_allowed(&url)
                || (!orig_url.is_empty() && !browser_policy::download_url_allowed(&orig_url))
            {
                return 1;
            }

            let raw_name = suggested_name.map(CefString::to_string).unwrap_or_default();
            let clean_name = browser_policy::sanitize_download_name(&raw_name);
            let mime = browser_policy::sanitize_download_mime(&CefString::from(&item.mime_type()).to_string());
            let dangerous = browser_policy::is_dangerous_download(&clean_name, &mime);
            let is_save_image = self.downloads.take_pending_save_image(self.tab_id, self.tab_gen, &orig_url, &url);
            // Official `policy.ts:166-177`: the agent owns a download only while it drives the tab;
            // the person's own input reclaims it (`TabPrompts::user_reclaimed`).
            let is_agent = self.prompts.agent_driven.load(Ordering::Acquire)
                && !self.prompts.user_reclaimed.load(Ordering::Acquire);

            // The person's downloads and right-click "Save image as…" go to the OS Save dialog
            // (official `chromium.ts:1055-1080`), never to agent-readable staging or `BrowserTab.files`.
            if !is_agent || is_save_image {
                if !self.downloads.download_save_enabled() {
                    return 1;
                }
                let mut pending = self.downloads.pending_saves.lock().unwrap();
                let tab_count = pending
                    .iter()
                    .filter(|((tid, _), p)| *tid == self.tab_id && p.tab_gen == self.tab_gen)
                    .count();
                if tab_count >= MAX_PENDING_SAVES_PER_TAB || pending.len() >= MAX_PENDING_SAVES_TOTAL {
                    return 1;
                }
                let req_id = self.downloads.next_save_req.fetch_add(1, Ordering::Relaxed);
                pending.insert(
                    (self.tab_id, req_id),
                    PendingSave {
                        tab_gen: self.tab_gen,
                        item_id: item.id(),
                        tab_alive: self.tab_alive.clone(),
                        callback: callback.clone(),
                    },
                );
                drop(pending);
                let _ = self.out_tx.send(EngineMsg::DownloadSaveRequest {
                    tab_id: self.tab_id,
                    id: req_id,
                    name: clean_name,
                    mime,
                    executable: dangerous,
                });
                return 1;
            }
            // Agent-driven tab download: stage in private per-context directory, capped & sanitized.
            let id = format!("file_{}", self.downloads.next_file.fetch_add(1, Ordering::Relaxed));
            let source_url = if !url.is_empty() {
                Some(url.clone())
            } else {
                browser
                    .and_then(|b| b.main_frame())
                    .map(|f| CefString::from(&f.url()).to_string())
                    .filter(|u| !u.is_empty())
            };
            let mut file = DownloadFile {
                id,
                name: clean_name.clone(),
                mime: mime.clone(),
                bytes: 0,
                state: DownloadState::Pending,
                path: String::new(),
                error: None,
                source_url,
            };
            let fail = |mut file: DownloadFile, why: &str| -> ::std::os::raw::c_int {
                file.state = DownloadState::Failed;
                file.error = Some(why.to_owned());
                self.send(file);
                1
            };
            if u64::try_from(item.total_bytes()).is_ok_and(|total| total > MAX_FILE_BYTES) {
                return fail(file, "too_large");
            }
            let held = self
                .downloads
                .files
                .lock()
                .unwrap()
                .keys()
                .filter(|(tab, g, _)| *tab == self.tab_id && *g == self.tab_gen)
                .count();
            if held >= MAX_TAB_FILES {
                return fail(file, "too_many");
            }
            let root_dir = if self.is_isolated {
                let Some(dir) = self.downloads.ensure_isolated_dir() else {
                    return fail(file, "file_failed");
                };
                dir
            } else {
                let Some(dir) = self.downloads.default_dir.clone() else {
                    return fail(file, "file_failed");
                };
                dir
            };
            let tab_dir = root_dir.join(self.tab_id.to_string());
            let folder = tab_dir.join(&file.id);
            if !ensure_safe_dir(&tab_dir) || !ensure_safe_dir(&folder) {
                return fail(file, "file_failed");
            }
            let path = folder.join(&clean_name);
            let Some(path_str) = path.to_str() else {
                let _ = std::fs::remove_dir(&folder);
                return fail(file, "file_failed");
            };
            file.path = path_str.to_owned();
            callback.cont(Some(&CefString::from(file.path.as_str())), 0);
            self.downloads
                .files
                .lock()
                .unwrap()
                .insert((self.tab_id, self.tab_gen, item.id()), file.clone());
            self.send(file);
            1
        }

        fn on_download_updated(&self, _browser: Option<&mut Browser>, item: Option<&mut DownloadItem>, callback: Option<&mut DownloadItemCallback>) {
            let alive = self.tab_alive.load(Ordering::Acquire);
            if self.block_downloads || !alive || !self.context_ready.load(Ordering::Acquire) {
                if let Some(callback) = callback {
                    callback.cancel();
                }
                if let Some(item) = item {
                    let key = (self.tab_id, self.tab_gen, item.id());
                    self.downloads.user_saves.lock().unwrap().remove(&key);
                    if let Some(mut file) = self.downloads.files.lock().unwrap().remove(&key) {
                        remove_file_folder(&file.path);
                        if alive && file.state == DownloadState::Pending {
                            file.state = DownloadState::Failed;
                            file.error = Some("canceled".to_owned());
                            self.send(file);
                        }
                    }
                }
                return;
            }
            let Some(item) = item else { return };
            let key = (self.tab_id, self.tab_gen, item.id());
            // Human saves stay invisible to `BrowserTab.files` and bypass the 5 MiB agent cap.
            {
                let mut saves = self.downloads.user_saves.lock().unwrap();
                if saves.contains(&key) {
                    if item.is_complete() != 0 || item.is_canceled() != 0 || item.is_interrupted() != 0 {
                        saves.remove(&key);
                    }
                    return;
                }
            }
            let mut files = self.downloads.files.lock().unwrap();
            let Some(file) = files.get_mut(&key) else { return };
            if file.state != DownloadState::Pending {
                return;
            }
            let bytes = u64::try_from(item.received_bytes()).unwrap_or_default();
            let step = bytes / (256 * 1024) != file.bytes / (256 * 1024);
            file.bytes = bytes;
            if bytes > MAX_FILE_BYTES {
                if let Some(callback) = callback {
                    callback.cancel();
                }
                file.state = DownloadState::Failed;
                file.error = Some("too_large".to_owned());
            } else if item.is_complete() != 0 {
                file.state = DownloadState::Completed;
            } else if item.is_canceled() != 0 {
                file.state = DownloadState::Failed;
                file.error = Some("canceled".to_owned());
            } else if item.is_interrupted() != 0 {
                file.state = DownloadState::Failed;
                file.error = Some(reason(item.interrupt_reason()));
            } else if !step {
                return;
            }
            let file = file.clone();
            drop(files);
            if file.state == DownloadState::Failed {
                remove_file_folder(&file.path);
            }
            self.send(file);
        }
    }
}

impl TabDownloadHandler {
    fn send(&self, file: DownloadFile) {
        let _ = self.out_tx.send(EngineMsg::Download { tab_id: self.tab_id, file });
    }
}

/// Chromium's interrupt reason, by its `DownloadInterruptReason` name in snake case.
fn reason(reason: DownloadInterruptReason) -> String {
    let debug = format!("{:?}", reason.as_ref());
    let name = debug.rsplit("CEF_DOWNLOAD_INTERRUPT_REASON_").next().unwrap_or(&debug);
    name.to_ascii_lowercase()
}
