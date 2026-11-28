//! Chrome MV3 extension host over a private, in-process anonymous CDP pipe.
//!
//! Architecture & trust boundaries:
//! - Starts Chromium with `--disable-extensions` (0 extensions) or
//!   `--disable-extensions-except=<inert 100-byte stub manifests>` (>=1 extension), permanently
//!   blocking Windows Registry and macOS/Linux external JSON loaders while keeping
//!   `cef::initialize()` at ~170 ms.
//! - Full parallel hardware-accelerated SHA-256 tree verification runs concurrently with
//!   `cef::initialize()`; verified extensions parse `ExtensionManifest` from the exact hashed
//!   bytes (zero re-read TOCTOU) and upgrade via `Extensions.loadUnpacked` (DNR filters first).
//! - Private anonymous OS pipe (`pipe.rs`: `bInheritHandle = FALSE` on Windows, verified vacant
//!   `FD_CLOEXEC` FDs 3/4 on Unix) with bounded inbound/outbound queues and non-blocking writes.
//! - Every `Runtime.bindingCalled` requires verified default world (`isDefault == true`,
//!   `type == "default"`, `origin == chrome-extension://<ext_id>`, and main frame on pages),
//!   and `Page.frameNavigated` immediately detaches any page that navigates off-origin.
//! - Every shimmed RPC method and event subscription enforces an exact allow-list, MV3
//!   permission gates, agent-tab protection, and per-extension rate limits on the Rust side.

#![deny(unsafe_code)]

mod cdp;
pub mod context_menus;
pub mod integrity;
pub mod manifest;
pub mod pipe;
mod png;
mod rpc;
pub mod sw_wake;
mod tab_lifecycle;
pub mod web_nav;
use tab_lifecycle::ExtTabLifecycle;

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use self::integrity::{StartupExtensions, VerifiedExtension, verify_extension_dir_and_stage};
use self::manifest::{
    ExtensionManifest, extension_label, extension_page_url, is_extension_scheme, is_private_url,
};
use self::pipe::{CdpPipe, PipeInbound};

pub(super) const WINDOW_ID: i32 = 1;
pub(super) const WINDOW_ID_NONE: i32 = -1;
pub(super) const WINDOW_ID_CURRENT: i32 = -2;
const SHIM_TEMPLATE: &str = include_str!("shim.js");

/// Sliding-window per-extension rate limiter (blocks tab bombs and icon/resize floods).
#[derive(Clone, Debug, Default)]
pub(super) struct RateWindow {
    stamps: VecDeque<Instant>,
}

impl RateWindow {
    pub(super) fn allow(&mut self, now: Instant, window: Duration, max: usize) -> bool {
        while self
            .stamps
            .front()
            .is_some_and(|t| now.saturating_duration_since(*t) >= window)
        {
            self.stamps.pop_front();
        }
        if self.stamps.len() >= max {
            return false;
        }
        self.stamps.push_back(now);
        true
    }
}

/// Effects the extension host asks `EngineManager` to perform on CEF browsers or the wire.
#[derive(Debug)]
pub enum ExtEffect {
    OpenTab { ext_id: String, url: String, active: bool, reply_to: Option<(String, i64, String)> },
    NavigateTab { engine_tab_id: u64, url: String },
    FocusTab { engine_tab_id: u64 },
    CloseTab { engine_tab_id: u64 },
    ReloadTab { engine_tab_id: u64, ignore_cache: bool },
    OpenSeedProbe { url: String },
    CloseSeedProbe,
    ActionUpdated(ExtensionActionSnapshot),
    ExtensionsSynced(Vec<ExtensionActionSnapshot>),
    FiltersReady,
    IntegrityFailed { ext_id: String },
    PopupResized { ext_id: String, width: u32, height: u32 },
    OpenPopup { ext_id: String, url: String },
    ExtensionUninstalled { ext_id: String, ok: bool, error: Option<String> },
    ExtensionReloaded { ext_id: String, ok: bool, error: Option<String> },
    NotificationShow { ext_id: String, ext_name: String, notif_id: String, body: String },
    NotificationClear { ext_id: String, notif_id: String },
    /// Wakes a Chromium-suspended MV3 service worker via `ServiceWorker.startWorker` so queued
    /// user-initiated events (`action.onClicked`, `contextMenus.onClicked`, `notifications.*`)
    /// deliver reliably after >30 s idle (`worker-events.ts:48`).
    WakeExtensionWorker { ext_id: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExtensionActionSnapshot {
    pub ext_id: String,
    pub name: String,
    pub version: String,
    pub title: String,
    pub popup: Option<String>,
    pub options: Option<String>,
    pub icon: Option<String>,
    pub badge_text: String,
    pub badge_color: Option<String>,
    pub badge_text_color: Option<String>,
    pub enabled: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ShimMetrics {
    pub sw_boots: u64,
    pub sw_attach_to_resume_us: Vec<u64>,
    pub sw_shim_eval_us: Vec<u64>,
    pub page_boots: u64,
    pub page_attach_to_resume_us: Vec<u64>,
    pub page_shim_eval_us: Vec<u64>,
    pub rpc_calls: u64,
    pub sw_restarts: u64,
    #[serde(default)]
    pub sw_uncaught_exceptions: u64,
    #[serde(default)]
    pub sw_exception_samples: Vec<String>,
    #[serde(default)]
    pub sw_wakes: u64,
}

#[derive(Clone, Debug)]
pub(super) struct ExtTab {
    pub engine_tab_id: u64,
    pub url: String,
    pub title: String,
    pub loading: bool,
    pub active: bool,
    pub agent_driven: bool,
    pub width: i32,
    pub height: i32,
    pub muted: bool,
    pub audible: bool,
    pub lifecycle: ExtTabLifecycle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SessionKind {
    ServiceWorker,
    ExtensionPage,
    PopupPage,
    SeedProbe,
    /// Extension dedicated/shared worker or extension iframe: receives ONLY `Fetch.enable`
    /// (`file://*` blocker) and NEVER `__ocExtCall` or `shim.js`.
    FileGuardOnly,
}

#[derive(Clone, Debug)]
pub(super) struct ExecutionContextMeta {
    pub origin: String,
    pub is_default: bool,
    pub ctx_type: String,
    pub frame_id: Option<String>,
}

#[derive(Debug)]
pub(super) struct AttachedSession {
    pub ext_id: String,
    pub target_id: String,
    pub kind: SessionKind,
    pub url: String,
    pub chromium_tab_id: Option<i32>,
    pub main_frame_id: Option<String>,
    pub last_popup_size: Option<(u32, u32)>,
    pub attach_started: Instant,
    pub pause_started: Option<Instant>,
    pub pending_shim_eval_id: Option<i64>,
    pub was_paused: bool,
    /// True when this `FileGuardOnly` session is an extension `iframe` (so crash-reload re-applies
    /// `Page.enable` + `IFRAME_LOCKDOWN_JS`, READER-7 #592 R2).
    pub is_iframe: bool,
    /// Set to `true` if ANY pre-resume setup command replies with an error; forces fail-closed
    /// hold even if `Fetch.enable` succeeds (READER-7 #592 R1).
    pub setup_failed: bool,
    /// Number of pre-resume setup commands still awaiting Chromium's reply for this session.
    pub setup_pending: u8,
    /// Set to `Some(send_resume)` once `Fetch.enable` replies cleanly; consumed when
    /// `setup_pending == 0 && !setup_failed`.
    pub resume_when_ready: Option<bool>,
    /// Becomes `true` strictly when `Fetch.enable` AND every pre-resume setup command reply with
    /// no error. Stays `false` forever if any setup command or `Fetch.enable` errors or never
    /// replies, so duplicate SW attaches can never bypass a fail-closed hold.
    pub fetch_confirmed: bool,
    pub shimmed: bool,
}

#[derive(Clone, Debug, Default)]
pub(super) struct ActionValues {
    pub title: Option<String>,
    pub popup: Option<String>,
    pub badge_text: Option<String>,
    pub badge_color: Option<String>,
    pub badge_text_color: Option<String>,
    pub icon: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct ActionState {
    pub all: ActionValues,
    pub tabs: HashMap<i32, ActionValues>,
}

impl ActionState {
    pub(super) fn for_tab(&self, tab_id: Option<i32>) -> ActionValues {
        let mut out = self.all.clone();
        if let Some(tid) = tab_id
            && let Some(ov) = self.tabs.get(&tid)
        {
            if ov.title.is_some() { out.title = ov.title.clone(); }
            if ov.popup.is_some() { out.popup = ov.popup.clone(); }
            if ov.badge_text.is_some() { out.badge_text = ov.badge_text.clone(); }
            if ov.badge_color.is_some() { out.badge_color = ov.badge_color.clone(); }
            if ov.badge_text_color.is_some() { out.badge_text_color = ov.badge_text_color.clone(); }
            if ov.icon.is_some() { out.icon = ov.icon.clone(); }
            if ov.enabled.is_some() { out.enabled = ov.enabled; }
        }
        out
    }

    pub(super) fn apply(&mut self, tab_id: Option<i32>, f: impl FnOnce(&mut ActionValues)) {
        match tab_id {
            None => f(&mut self.all),
            Some(tid) => {
                if self.tabs.contains_key(&tid) || self.tabs.len() < 256 {
                    f(self.tabs.entry(tid).or_default());
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct LoadedExt {
    pub dir: PathBuf,
    /// When the extension was staged into a per-stage `_rt/<ext_id>-<seq>` mirror (because its
    /// manifest declared permissions stripped from Chromium C++), holds that mirror path so
    /// uninstall, failed post-load verify, or reload replacement cleans it up promptly (F1).
    pub staged_dir: Option<PathBuf>,
    pub sha256: String,
    pub manifest: ExtensionManifest,
    pub is_unpacked: bool,
    pub loaded: bool,
    pub boot_count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ExpectedTarget {
    WebTab,
    Popup { ext_id: String },
    ExtensionPage { ext_id: String },
    SeedProbe { ext_id: String },
    IsolatedTab,
}

pub(super) struct PendingRpc {
    pub session_id: String,
    pub ext_id: String,
    pub req_id: i64,
    pub name: String,
    pub args: Vec<Value>,
}

pub(super) struct ProbeInFlight {
    pub probe_id: i64,
    pub sid: String,
    pub sent_at: Instant,
    pub candidates: Vec<i32>,
}

pub struct ExtensionHost {
    pub(super) pipe: CdpPipe,
    inbound_rx: Receiver<PipeInbound>,
    wake_pending: Arc<AtomicBool>,
    next_cdp_id: i64,
    pub(super) extensions: HashMap<String, LoadedExt>,
    pub(super) verified_startup: HashMap<String, VerifiedExtension>,
    pub(super) stub_dirs_by_id: HashMap<String, PathBuf>,
    pub(super) stub_root: Option<PathBuf>,
    pub(super) sessions: HashMap<String, AttachedSession>,
    /// Tracks `(session_id, executionContextId) -> ExecutionContextMeta` (origin, isDefault,
    /// type, frameId) so `Runtime.bindingCalled` only accepts verified default-world calls.
    pub(super) execution_contexts: HashMap<(String, i64), ExecutionContextMeta>,
    pub(super) subscribers: HashMap<String, HashMap<String, HashSet<String>>>,
    pub(super) tabs: Vec<ExtTab>,
    pub(super) actions: HashMap<String, ActionState>,
    pub(super) context_menus: Arc<std::sync::Mutex<context_menus::ExtensionContextMenuCatalog>>,
    pub(super) web_nav: web_nav::WebNavTracker,
    pub(super) expected_targets: VecDeque<ExpectedTarget>,
    pub(super) seed_session_id: Option<i32>,
    pub(super) seed_probe_started: bool,
    pub(super) seed_eval_id: Option<i64>,
    /// Per-extension `engine_tab_id -> chromium_tab_id` map so one low-privilege extension can
    /// never poison another extension's Chromium tab ID mappings (READER-12 #6).
    pub(super) ext_tab_ids: HashMap<String, HashMap<u64, i32>>,
    pub(super) ext_last_known_session_id: HashMap<String, i32>,
    /// Per extension, the Chromium id of the tab last announced through `tabs.onActivated`.
    pub(super) announced_active: HashMap<String, i32>,
    pub(super) next_probe_id: i64,
    pub(super) pending_rpcs: Vec<PendingRpc>,
    pub(super) probe_in_flight: HashMap<String, ProbeInFlight>,
    pub(super) failed_probe_sessions: HashSet<String>,
    pub(super) pending_startup_loads: HashMap<i64, String>,
    pub(super) pending_filter_ids: HashSet<String>,
    pub(super) filters_ready_emitted: bool,
    pub(super) queued_clicks: Vec<String>,
    pub(super) pending_uninstall_upgrade: HashMap<i64, String>,
    pub(super) pending_stub_cleanup: HashMap<i64, String>,
    pub(super) pending_uninstall_cmd: HashMap<i64, String>,
    pub(super) pending_reload_cmd: HashMap<i64, String>,
    /// Pre-resume setup CDP command IDs mapped to their session ID so any setup error marks
    /// `sess.setup_failed = true` and blocks resume when `Fetch.enable` replies (READER-7 R1).
    pub(super) pending_setup_cmds: HashMap<i64, String>,
    /// Tracks `Fetch.enable` command IDs waiting for Chromium's reply (`UpdateSubresourceLoaderFactories`
    /// completed) before sending `Runtime.runIfWaitingForDebugger` (`review-rd274.md` Round-2 §1c).
    /// Value is `(session_id, send_resume_when_confirmed)`.
    pub(super) pending_fetch_resume: HashMap<i64, (String, bool)>,
    /// Sessions whose `Fetch.enable` confirmed cleanly while their extension was still awaiting
    /// post-load `verify_extension_file_access_disabled` (`ext.loaded == false`, READER-7 F2).
    /// Value is `(session_id, send_resume_when_loaded)`. Resumed strictly after
    /// `verify_extension_file_access_disabled` passes and `ext.loaded = true`.
    pub(super) pending_loaded_resume: Vec<(String, bool)>,
    /// Duplicate SW `Target.attachedToTarget` sessions held quiet while the primary SW session is
    /// still waiting on `Fetch.enable` or post-load verification (`ext.loaded`), keyed by `targetId`.
    /// Released only after BOTH confirm (fail-closed if either fails; avoids waitForDebugger deadlock).
    pub(super) deferred_dup_sw_detaches: HashMap<String, Vec<(String, bool)>>,
    sw_wake: sw_wake::SwWakeState,
    pub(super) pipe_closed: bool,
    pub(super) open_tab_rates: HashMap<String, RateWindow>,
    pub(super) action_rates: HashMap<String, RateWindow>,
    pub(super) popup_resize_rates: HashMap<String, RateWindow>,
    pub(super) notification_rates: HashMap<String, RateWindow>,
    pub(super) notifications: HashMap<String, Vec<String>>,
    pub(super) notifications_enabled: bool,
    pub(super) window_created: bool,
    pub(super) window_focused: bool,
    pub(super) window_width: i32,
    pub(super) window_height: i32,
    pub metrics: ShimMetrics,
}

impl ExtensionHost {
    pub fn new(
        pipe: CdpPipe,
        inbound_rx: Receiver<PipeInbound>,
        wake_pending: Arc<AtomicBool>,
        stub_dirs_by_id: HashMap<String, PathBuf>,
        stub_root: Option<PathBuf>,
        menu_storage_dir: Option<PathBuf>,
    ) -> Self {
        let listener_storage_dir = menu_storage_dir
            .as_deref()
            .and_then(|d| d.parent().map(|p| p.join("opencode-ext-listeners")));
        let catalog = match menu_storage_dir {
            Some(dir) => context_menus::ExtensionContextMenuCatalog::with_storage_dir(dir),
            None => context_menus::ExtensionContextMenuCatalog::default(),
        };
        Self {
            pipe,
            inbound_rx,
            wake_pending,
            next_cdp_id: 2, // id=1 was pre-written Target.setAutoAttach
            extensions: HashMap::new(),
            verified_startup: HashMap::new(),
            stub_dirs_by_id,
            stub_root,
            sessions: HashMap::new(),
            execution_contexts: HashMap::new(),
            subscribers: HashMap::new(),
            tabs: Vec::new(),
            actions: HashMap::new(),
            context_menus: Arc::new(std::sync::Mutex::new(catalog)),
            web_nav: web_nav::WebNavTracker::default(),
            expected_targets: VecDeque::new(),
            seed_session_id: None,
            seed_probe_started: false,
            seed_eval_id: None,
            ext_tab_ids: HashMap::new(),
            ext_last_known_session_id: HashMap::new(),
            announced_active: HashMap::new(),
            next_probe_id: 1,
            pending_rpcs: Vec::new(),
            probe_in_flight: HashMap::new(),
            failed_probe_sessions: HashSet::new(),
            pending_startup_loads: HashMap::new(),
            pending_filter_ids: HashSet::new(),
            filters_ready_emitted: false,
            queued_clicks: Vec::new(),
            pending_uninstall_upgrade: HashMap::new(),
            pending_stub_cleanup: HashMap::new(),
            pending_uninstall_cmd: HashMap::new(),
            pending_reload_cmd: HashMap::new(),
            pending_setup_cmds: HashMap::new(),
            pending_fetch_resume: HashMap::new(),
            pending_loaded_resume: Vec::new(),
            deferred_dup_sw_detaches: HashMap::new(),
            sw_wake: sw_wake::SwWakeState::with_storage_dir(listener_storage_dir),
            pipe_closed: false,
            open_tab_rates: HashMap::new(),
            action_rates: HashMap::new(),
            popup_resize_rates: HashMap::new(),
            notification_rates: HashMap::new(),
            notifications: HashMap::new(),
            // Off until the app says Settings > Notifications allows them, so `getPermissionLevel`
            // never answers "granted" in the moment before `SetNotificationsEnabled` arrives.
            notifications_enabled: false,
            window_created: false,
            window_focused: false,
            window_width: 1280,
            window_height: 800,
            metrics: ShimMetrics::default(),
        }
    }

    /// Drains up to 64 inbound CDP pipe messages per pump tick plus checks probe timeouts.
    pub fn drain_inbound(&mut self) -> Vec<ExtEffect> {
        self.wake_pending.store(false, Ordering::Release);
        let mut effects = Vec::new();
        for _ in 0..64 {
            match self.inbound_rx.try_recv() {
                Ok(PipeInbound::Message(val)) => effects.extend(self.on_cdp_message(val)),
                Ok(PipeInbound::FrameDropped) => effects.extend(self.on_frame_dropped()),
                Ok(PipeInbound::Closed) => {
                    if !self.pipe_closed {
                        self.pipe_closed = true;
                        effects.extend(self.on_pipe_closed());
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    if !self.pipe_closed {
                        self.pipe_closed = true;
                        effects.extend(self.on_pipe_closed());
                    }
                    break;
                }
            }
        }
        effects.extend(self.tick_timeouts());
        effects
    }

    /// Delivers the background parallel SHA-256 verification outcome once `cef::initialize()`
    /// finishes: upgrades & uninstalls inert stubs for tampered extensions, and asynchronously
    /// upgrades verified extensions via `Extensions.loadUnpacked` (DNR filter extensions first).
    pub fn on_startup_verified(&mut self, startup: StartupExtensions) -> Vec<ExtEffect> {
        let mut effects = Vec::new();
        for bad_id in startup.rejected_ids {
            self.sw_wake.remove_extension(&bad_id, true);
            self.uninstall_inert_stub(&bad_id);
            effects.push(ExtEffect::IntegrityFailed { ext_id: bad_id });
        }

        // Load DNR filter extensions first so ad-blockers become active ahead of large extensions.
        let mut ordered = startup.verified;
        ordered.sort_by_key(|item| !item.manifest.is_filter_extension());

        for item in ordered {
            let is_filter = item.manifest.is_filter_extension();
            let id = item.id.clone();
            let staged_dir = (item.load_dir != item.dir).then(|| item.load_dir.clone());
            let Some(path_str) = crate::context::canonical_extension_load_path(&item.load_dir) else {
                if let Some(ref staged) = staged_dir {
                    integrity::cleanup_staged_dir(staged);
                }
                self.sw_wake.remove_extension(&id, true);
                self.uninstall_inert_stub(&id);
                effects.push(ExtEffect::IntegrityFailed { ext_id: id });
                continue;
            };
            // Fail closed if Chromium refuses to revoke `ALLOW_FILE_ACCESS` before load
            // (hashes the exact single `path_str` passed to `loadUnpacked`; `review-rd274.md` Round-3 §1).
            if !crate::context::disallow_extension_file_access(&id, &path_str) {
                if let Some(ref staged) = staged_dir {
                    integrity::cleanup_staged_dir(staged);
                }
                self.sw_wake.remove_extension(&id, true);
                self.uninstall_inert_stub(&id);
                effects.push(ExtEffect::IntegrityFailed { ext_id: id });
                continue;
            }
            if is_filter {
                self.pending_filter_ids.insert(id.clone());
            }
            self.verified_startup.insert(id.clone(), item.clone());
            self.register_extension(id.clone(), item.dir, staged_dir, item.sha256, item.manifest, false, false);
            let cmd_id = self.send_cdp("Extensions.loadUnpacked", json!({ "path": path_str }), None);
            self.pending_startup_loads.insert(cmd_id, id);
        }

        if self.pending_filter_ids.is_empty() && !self.filters_ready_emitted {
            self.filters_ready_emitted = true;
            effects.push(ExtEffect::FiltersReady);
        }
        effects
    }

    /// Upgrades a `kCommandLine` inert stub dir to `kUnpacked` so `Extensions.uninstall` removes
    /// it cleanly (Chromium rejects `Extensions.uninstall` directly on `kCommandLine` extensions).
    pub(super) fn uninstall_inert_stub(&mut self, bad_id: &str) {
        if let Some(stub_dir) = self.stub_dirs_by_id.get(bad_id).cloned() {
            if let Some(path_str) = crate::context::canonical_extension_load_path(&stub_dir) {
                let _ = crate::context::disallow_extension_file_access(bad_id, &path_str);
                let cid = self.send_cdp("Extensions.loadUnpacked", json!({ "path": path_str }), None);
                self.pending_stub_cleanup.insert(cid, bad_id.to_owned());
            } else {
                let _ = self.send_cdp("Extensions.uninstall", json!({ "id": bad_id }), None);
            }
        } else {
            let _ = self.send_cdp("Extensions.uninstall", json!({ "id": bad_id }), None);
        }
    }

    /// How many extensions are installed (Clear browsing data keeps their service workers).
    pub fn installed_count(&self) -> usize {
        self.extensions.len()
    }

    pub fn filters_ready(&self) -> bool {
        self.filters_ready_emitted
    }

    pub fn close_pipe(&self) {
        self.pipe.close();
    }

    pub fn list_actions(&self) -> Vec<ExtensionActionSnapshot> {
        self.extensions
            .keys()
            .filter_map(|id| self.action_snapshot(id))
            .collect()
    }

    pub fn has_clipboard_write(&self, ext_id: &str) -> bool {
        self.extensions.get(ext_id).is_some_and(|e| e.manifest.has_permission("clipboardWrite"))
    }

    pub fn clipboard_write_ext_ids(&self) -> std::collections::HashSet<String> {
        self.extensions
            .iter()
            .filter(|(_, e)| e.manifest.has_permission("clipboardWrite"))
            .map(|(id, _)| id.clone())
            .collect()
    }

    pub fn registered_ext_ids(&self) -> std::collections::HashSet<String> {
        self.extensions.keys().cloned().collect()
    }

    pub fn action_snapshot(&self, ext_id: &str) -> Option<ExtensionActionSnapshot> {
        let ext = self.extensions.get(ext_id)?;
        let active_tid = self.active_chromium_tab_id_for(ext_id);
        let vals = self.actions.get(ext_id).map(|s| s.for_tab(active_tid)).unwrap_or_default();
        let def_title = ext
            .manifest
            .action
            .as_ref()
            .and_then(|a| a.default_title.clone())
            .unwrap_or_else(|| ext.manifest.name.clone());
        let def_popup = ext.manifest.action.as_ref().and_then(|a| a.default_popup.clone());
        let popup = vals.popup.or(def_popup).as_deref().and_then(|f| extension_page_url(ext_id, f));
        let options = ext.manifest.options_page.as_deref().and_then(|f| extension_page_url(ext_id, f));
        Some(ExtensionActionSnapshot {
            ext_id: ext_id.to_owned(),
            name: ext.manifest.name.clone(),
            version: ext.manifest.version.clone(),
            title: extension_label(vals.title.as_deref().unwrap_or(&def_title)),
            popup,
            options,
            icon: vals.icon.or_else(|| ext.manifest.icon_data_url.clone()),
            badge_text: extension_label(vals.badge_text.as_deref().unwrap_or("")),
            badge_color: vals.badge_color,
            badge_text_color: vals.badge_text_color,
            enabled: vals.enabled.unwrap_or(true),
        })
    }

    pub fn click_extension(&mut self, ext_id: &str) -> Vec<ExtEffect> {
        if self.is_uninstalling(ext_id) {
            return Vec::new();
        }
        let Some(ext_entry) = self.extensions.get(ext_id) else {
            return Vec::new();
        };
        if !ext_entry.loaded {
            if !self.queued_clicks.iter().any(|id| id == ext_id) {
                self.queued_clicks.push(ext_id.to_owned());
            }
            return Vec::new();
        }
        // As official's `click`: the tab the click is about counts as activated before the popup
        // opens or the extension hears the click.
        self.announce_active_tab_for(ext_id);
        let Some(snap) = self.action_snapshot(ext_id) else {
            return Vec::new();
        };
        if !snap.enabled {
            return Vec::new();
        }
        if let Some(popup_url) = snap.popup {
            return vec![ExtEffect::OpenPopup { ext_id: ext_id.to_owned(), url: popup_url }];
        }
        let active_tab = self
            .tabs
            .iter()
            .find(|t| t.active)
            .or_else(|| self.tabs.iter().rev().find(|t| !t.agent_driven));
        if let Some(tab) = active_tab.filter(|t| !t.agent_driven).cloned()
            && !is_private_url(&tab.url)
            && let Some(ext) = self.extensions.get(ext_id).cloned()
        {
            // Reveal active tab URL/title on toolbar click ONLY if the extension has `activeTab`
            // (and never reveal another extension's `chrome-extension://` page; `review-rd274.md` §6).
            let reveal_via_active_tab =
                ext.manifest.has_permission("activeTab") && !is_extension_scheme(&tab.url);
            let info = self.tab_json(ext_id, &tab, &ext.manifest, reveal_via_active_tab);
            if let Some(wake) = self.emit_or_wake_event("action.onClicked", vec![info], ext_id) {
                return vec![wake];
            }
        }
        Vec::new()
    }

    /// Runtime disable / uninstall with ZERO engine restart.
    pub fn uninstall_extension(&mut self, ext_id: &str) -> Vec<ExtEffect> {
        // Immediately drop any queued clicks, wake events & listener records so a disable-in-flight
        // extension can never be woken or receive a late click while uninstalling.
        self.queued_clicks.retain(|id| id != ext_id);
        self.sw_wake.remove_extension(ext_id, true);
        let Some(ext) = self.extensions.get(ext_id).cloned() else {
            return vec![ExtEffect::ExtensionUninstalled {
                ext_id: ext_id.to_owned(),
                ok: false,
                error: Some("Extension not loaded".into()),
            }];
        };
        if ext.is_unpacked {
            let id = self.send_cdp("Extensions.uninstall", json!({ "id": ext_id }), None);
            self.pending_uninstall_cmd.insert(id, ext_id.to_owned());
        } else {
            let Some((_, load_dir)) = verify_extension_dir_and_stage(
                &ext.dir,
                ext_id,
                &ext.sha256,
                self.stub_root.as_deref(),
            ) else {
                return vec![ExtEffect::ExtensionUninstalled {
                    ext_id: ext_id.to_owned(),
                    ok: false,
                    error: Some("Extension files changed on disk; restart required.".into()),
                }];
            };
            let staged_dir = (load_dir != ext.dir).then(|| load_dir.clone());
            let Some(path_str) = crate::context::canonical_extension_load_path(&load_dir) else {
                if let Some(ref staged) = staged_dir {
                    integrity::cleanup_staged_dir(staged);
                }
                return vec![ExtEffect::ExtensionUninstalled {
                    ext_id: ext_id.to_owned(),
                    ok: false,
                    error: Some("Could not resolve extension path.".into()),
                }];
            };
            if !crate::context::disallow_extension_file_access(ext_id, &path_str) {
                if let Some(ref staged) = staged_dir {
                    integrity::cleanup_staged_dir(staged);
                }
                return vec![ExtEffect::ExtensionUninstalled {
                    ext_id: ext_id.to_owned(),
                    ok: false,
                    error: Some("Could not disable file:// access before uninstall upgrade.".into()),
                }];
            }
            if let Some(cur) = self.extensions.get_mut(ext_id) {
                if let Some(old) = cur.staged_dir.replace(staged_dir.unwrap_or_default())
                    && !old.as_os_str().is_empty()
                {
                    integrity::cleanup_staged_dir(&old);
                }
                if cur.staged_dir.as_ref().is_some_and(|p| p.as_os_str().is_empty()) {
                    cur.staged_dir = None;
                }
            }
            let id = self.send_cdp("Extensions.loadUnpacked", json!({ "path": path_str }), None);
            self.pending_uninstall_upgrade.insert(id, ext_id.to_owned());
        }
        Vec::new()
    }

    /// Runtime reload / re-enable of a previously verified startup-exempt extension by ID.
    /// Re-verifies SHA-256 tree digest & parses manifest from hashed bytes, pre-registers
    /// `ext_id` BEFORE `Extensions.loadUnpacked` so `Target.attachedToTarget` never detaches the
    /// reloaded service worker unshimmed (`review-rd274.md` §7).
    pub fn reload_extension(&mut self, ext_id: &str) -> Vec<ExtEffect> {
        let Some(item) = self.verified_startup.get(ext_id).cloned() else {
            return vec![ExtEffect::ExtensionReloaded {
                ext_id: ext_id.to_owned(),
                ok: false,
                error: Some("Extension was not exempted at engine startup; restart required.".into()),
            }];
        };
        let Some((manifest, load_dir)) = verify_extension_dir_and_stage(
            &item.dir,
            &item.id,
            &item.sha256,
            self.stub_root.as_deref(),
        ) else {
            self.sw_wake.remove_extension(ext_id, true);
            return vec![ExtEffect::ExtensionReloaded {
                ext_id: ext_id.to_owned(),
                ok: false,
                error: Some("Extension integrity digest mismatch.".into()),
            }];
        };
        let staged_dir = (load_dir != item.dir).then(|| load_dir.clone());
        let Some(path_str) = crate::context::canonical_extension_load_path(&load_dir) else {
            if let Some(ref staged) = staged_dir {
                integrity::cleanup_staged_dir(staged);
            }
            return vec![ExtEffect::ExtensionReloaded {
                ext_id: ext_id.to_owned(),
                ok: false,
                error: Some("Could not resolve extension path.".into()),
            }];
        };
        if !crate::context::disallow_extension_file_access(&item.id, &path_str) {
            if let Some(ref staged) = staged_dir {
                integrity::cleanup_staged_dir(staged);
            }
            return vec![ExtEffect::ExtensionReloaded {
                ext_id: ext_id.to_owned(),
                ok: false,
                error: Some("Could not disable file:// access for extension.".into()),
            }];
        }
        self.register_extension(item.id.clone(), item.dir, staged_dir, item.sha256, manifest, true, false);
        let id = self.send_cdp("Extensions.loadUnpacked", json!({ "path": path_str }), None);
        self.pending_reload_cmd.insert(id, item.id);
        Vec::new()
    }

    pub fn expect_web_tab(&mut self) {
        self.expected_targets.push_back(ExpectedTarget::WebTab);
    }

    pub fn expect_isolated_tab(&mut self) {
        self.expected_targets.push_back(ExpectedTarget::IsolatedTab);
    }

    pub fn expect_popup(&mut self, ext_id: &str) {
        self.expected_targets.push_back(ExpectedTarget::Popup { ext_id: ext_id.to_owned() });
    }

    pub fn expect_extension_page(&mut self, ext_id: &str) {
        self.expected_targets.push_back(ExpectedTarget::ExtensionPage { ext_id: ext_id.to_owned() });
    }

    /// Pops the last pushed expected target if `cef::browser_host_create_browser_sync` returned
    /// `None` or `browser.host()` failed, keeping the target queue synchronized.
    pub fn cancel_expected_target(&mut self) {
        let _ = self.expected_targets.pop_back();
    }

    pub fn set_tab_agent_driven(&mut self, engine_tab_id: u64, agent_driven: bool) {
        let Some(tab) = self.tabs.iter_mut().find(|t| t.engine_tab_id == engine_tab_id) else {
            return;
        };
        let was = tab.agent_driven;
        tab.agent_driven = agent_driven;
        if !was && agent_driven {
            let removed: Vec<(String, i32)> = self
                .ext_tab_ids
                .iter_mut()
                .filter_map(|(ext_id, map)| map.remove(&engine_tab_id).map(|cid| (ext_id.clone(), cid)))
                .collect();
            let closing = self.visible_tabs().next().is_none();
            for (ext_id, cid) in removed {
                self.emit_event(
                    "tabs.onRemoved",
                    vec![json!(cid), json!({ "windowId": WINDOW_ID, "isWindowClosing": closing })],
                    Some(&ext_id),
                );
            }
            self.maybe_emit_window_removed();
        }
    }

    pub fn on_tab_opened(
        &mut self,
        engine_tab_id: u64,
        url: &str,
        width: i32,
        height: i32,
        active: bool,
        agent_driven: bool,
    ) -> Vec<ExtEffect> {
        let visible_active = active && !agent_driven;
        if visible_active {
            for t in &mut self.tabs {
                t.active = false;
            }
        }
        let woke_tab = if let Some(existing) = self.tabs.iter_mut().find(|t| t.engine_tab_id == engine_tab_id) {
            let was_discarded = existing.lifecycle.is_discarded();
            existing.url = url.to_owned();
            existing.loading = true;
            existing.active = visible_active;
            existing.agent_driven = agent_driven;
            existing.width = width;
            existing.height = height;
            existing.audible = false;
            existing.lifecycle = ExtTabLifecycle::Live;
            was_discarded.then(|| existing.clone())
        } else {
            self.tabs.push(ExtTab {
                engine_tab_id,
                url: url.to_owned(),
                title: String::new(),
                loading: true,
                active: visible_active,
                agent_driven,
                width,
                height,
                muted: false,
                audible: false,
                lifecycle: ExtTabLifecycle::Live,
            });
            None
        };
        if let Some(tab) = woke_tab {
            if !tab.agent_driven {
                self.emit_tab_updated(&tab, json!({ "discarded": false }), false);
            }
            // Clear stale Chromium tab ID now that a new browser was spawned so probing binds the fresh ID.
            for map in self.ext_tab_ids.values_mut() {
                map.remove(&engine_tab_id);
            }
        }
        if !agent_driven {
            if !self.window_created {
                if width > 0 && height > 0 {
                    self.window_width = width;
                    self.window_height = height;
                }
                self.window_created = true;
                if active {
                    self.window_focused = true;
                }
                self.emit_window_event("windows.onCreated");
                if active {
                    self.emit_event("windows.onFocusChanged", vec![json!(WINDOW_ID)], None);
                }
            } else if active {
                self.emit_window_focus(true);
            }
        }
        let effects = self.maybe_start_seed_probe();
        self.trigger_tab_id_probes_all();
        effects
    }

    pub fn on_tab_closed(&mut self, engine_tab_id: u64) {
        self.web_nav.remove_tab(engine_tab_id);
        if let Some(pos) = self.tabs.iter().position(|t| t.engine_tab_id == engine_tab_id) {
            let tab = self.tabs.remove(pos);
            let removed: Vec<(String, i32)> = self
                .ext_tab_ids
                .iter_mut()
                .filter_map(|(ext_id, map)| map.remove(&engine_tab_id).map(|cid| (ext_id.clone(), cid)))
                .collect();
            if !tab.agent_driven {
                let closing = self.visible_tabs().next().is_none();
                for (ext_id, cid) in removed {
                    self.emit_event(
                        "tabs.onRemoved",
                        vec![json!(cid), json!({ "windowId": WINDOW_ID, "isWindowClosing": closing })],
                        Some(&ext_id),
                    );
                }
                self.maybe_emit_window_removed();
            }
        }
    }

    pub fn on_tab_focused(&mut self, engine_tab_id: u64) {
        let mut newly_active_visible = false;
        let mut has_visible = false;
        for t in &mut self.tabs {
            let is_this = t.engine_tab_id == engine_tab_id;
            if is_this && !t.active && !t.agent_driven {
                newly_active_visible = true;
            }
            if is_this && !t.agent_driven {
                has_visible = true;
            }
            t.active = is_this;
        }
        if has_visible && self.window_created {
            self.emit_window_focus(true);
        }
        if newly_active_visible {
            let pairs: Vec<(String, i32)> = self
                .ext_tab_ids
                .iter()
                .filter_map(|(ext_id, map)| map.get(&engine_tab_id).copied().map(|cid| (ext_id.clone(), cid)))
                .collect();
            for (ext_id, cid) in pairs {
                self.broadcast_active_tab_for(&ext_id, cid);
            }
        }
        if has_visible {
            self.announce_active_tab_all();
        }
    }

    /// Tells `ext_id` which visible tab is the active one, once per activation, through
    /// `tabs.onActivated`: official does when a pane tab takes focus and when the user picks the
    /// extension in the toolbar menu (`extension-api/tabs.ts` `activate`), the first tab included.
    /// A tab's Chromium id is known here only after a probe bound it, and the extension may start
    /// listening after that, so the announcement waits for both instead of being dropped: an
    /// extension that tracks the selected tab from this event (PayPal Honey) otherwise never learns
    /// it and its popup stays blank.
    pub(super) fn announce_active_tab_for(&mut self, ext_id: &str) {
        let Some(cid) = self.active_chromium_tab_id_for(ext_id) else {
            return;
        };
        if self.announced_active.get(ext_id) == Some(&cid) {
            return;
        }
        let listening = self
            .subscribers
            .get("tabs.onActivated")
            .and_then(|by_ext| by_ext.get(ext_id))
            .is_some_and(|sids| !sids.is_empty());
        if !listening {
            return;
        }
        self.announced_active.insert(ext_id.to_owned(), cid);
        self.emit_event(
            "tabs.onActivated",
            vec![json!({ "tabId": cid, "windowId": WINDOW_ID })],
            Some(ext_id),
        );
    }

    fn announce_active_tab_all(&mut self) {
        let ext_ids: Vec<String> = self.ext_tab_ids.keys().cloned().collect();
        for ext_id in ext_ids {
            self.announce_active_tab_for(&ext_id);
        }
    }

    pub fn on_window_blurred(&mut self) {
        if self.window_created {
            self.emit_window_focus(false);
        }
    }

    pub fn on_tab_resized(&mut self, engine_tab_id: u64, width: i32, height: i32) {
        if width <= 0 || height <= 0 {
            return;
        }
        let Some(tab) = self.tabs.iter_mut().find(|t| t.engine_tab_id == engine_tab_id) else {
            return;
        };
        tab.width = width;
        tab.height = height;
        if tab.active && !tab.agent_driven && (self.window_width != width || self.window_height != height) {
            self.window_width = width;
            self.window_height = height;
            if self.window_created {
                self.emit_window_event("windows.onBoundsChanged");
            }
        }
    }

    fn maybe_emit_window_removed(&mut self) {
        if self.window_created && self.visible_tabs().next().is_none() {
            self.emit_window_focus(false);
            self.window_created = false;
            self.emit_event("windows.onRemoved", vec![json!(WINDOW_ID)], None);
        }
    }

    fn emit_window_focus(&mut self, focused: bool) {
        if self.window_focused == focused {
            return;
        }
        self.window_focused = focused;
        let id = if focused { WINDOW_ID } else { WINDOW_ID_NONE };
        self.emit_event("windows.onFocusChanged", vec![json!(id)], None);
    }

    fn emit_window_event(&mut self, event: &str) {
        let subscribed: Vec<(String, ExtensionManifest)> = self
            .subscribers
            .get(event)
            .map(|by_ext| {
                by_ext
                    .keys()
                    .filter_map(|eid| self.extensions.get(eid).map(|e| (eid.clone(), e.manifest.clone())))
                    .collect()
            })
            .unwrap_or_default();
        for (ext_id, manifest) in subscribed {
            let win = self.window_json(&ext_id, &manifest, false);
            self.emit_event(event, vec![win], Some(&ext_id));
        }
    }

    pub fn on_tab_url_changed(&mut self, engine_tab_id: u64, url: &str) {
        let Some(tab) = self.tabs.iter_mut().find(|t| t.engine_tab_id == engine_tab_id) else {
            return;
        };
        if tab.url == url {
            return;
        }
        tab.url = url.to_owned();
        if tab.agent_driven {
            return;
        }
        let tab_clone = tab.clone();
        if !is_private_url(url) {
            self.emit_tab_updated(&tab_clone, json!({ "url": url }), true);
        }
    }

    pub fn on_tab_title_changed(&mut self, engine_tab_id: u64, title: &str) {
        let Some(tab) = self.tabs.iter_mut().find(|t| t.engine_tab_id == engine_tab_id) else {
            return;
        };
        if tab.title == title {
            return;
        }
        tab.title = title.to_owned();
        if tab.agent_driven {
            return;
        }
        let tab_clone = tab.clone();
        if !is_private_url(&tab_clone.url) {
            self.emit_tab_updated(&tab_clone, json!({ "title": title }), true);
        }
    }

    pub fn on_tab_loading_changed(&mut self, engine_tab_id: u64, loading: bool) {
        let Some(tab) = self.tabs.iter_mut().find(|t| t.engine_tab_id == engine_tab_id) else {
            return;
        };
        if tab.loading == loading {
            return;
        }
        tab.loading = loading;
        if tab.agent_driven {
            return;
        }
        let tab_clone = tab.clone();
        let status = if loading { "loading" } else { "complete" };
        self.emit_tab_updated(&tab_clone, json!({ "status": status }), false);
    }

    /// Processes a CEF frame navigation signal and emits full `chrome.webNavigation` events
    /// (main frame + subframes) to subscribed extensions (`web-navigation.ts:28-126`).
    pub fn on_web_navigation(&mut self, sig: web_nav::WebNavSignal) {
        let tab_id = sig.tab_id;
        let Some(tab) = self.tabs.iter().find(|t| t.engine_tab_id == tab_id && !t.agent_driven) else {
            return;
        };
        let tab_url = tab.url.clone();
        let events = self.web_nav.ingest(&tab_url, sig, now_ms());
        if events.is_empty() {
            return;
        }
        let pairs: Vec<(String, i32)> = self
            .ext_tab_ids
            .iter()
            .filter_map(|(ext_id, map)| map.get(&tab_id).copied().map(|cid| (ext_id.clone(), cid)))
            .collect();
        for (ev_name, base_details) in events {
            for (ext_id, cid) in &pairs {
                let mut details = base_details.clone();
                if let Some(obj) = details.as_object_mut() {
                    obj.insert("tabId".into(), json!(cid));
                }
                self.emit_event(ev_name, vec![details], Some(ext_id));
            }
        }
    }

    pub fn finish_tab_create(&mut self, sid: &str, req_id: i64, ext_id: &str, engine_tab_id: u64) {
        if let (Some(ext), Some(tab)) = (
            self.extensions.get(ext_id),
            self.tabs.iter().find(|t| t.engine_tab_id == engine_tab_id && !t.agent_driven),
        ) {
            let val = self.tab_json(ext_id, tab, &ext.manifest, false);
            self.reply_ok(sid, req_id, val);
        } else {
            self.reply_ok(sid, req_id, Value::Null);
        }
    }

    pub fn fail_tab_create(&mut self, sid: &str, req_id: i64, error: &str) {
        self.reply_err(sid, req_id, error);
    }

    pub(super) fn emit_tab_updated(&mut self, tab: &ExtTab, change: Value, needs_access: bool) {
        if tab.agent_driven {
            return;
        }
        let exts: Vec<(String, ExtensionManifest, i32)> = self
            .extensions
            .iter()
            .filter_map(|(id, e)| {
                let cid = self.ext_chromium_tab_id(id, tab.engine_tab_id)?;
                Some((id.clone(), e.manifest.clone(), cid))
            })
            .collect();
        for (ext_id, manifest, cid) in exts {
            if needs_access && !manifest.reveals_tab(&tab.url) {
                continue;
            }
            let info = self.tab_json(&ext_id, tab, &manifest, false);
            self.emit_event("tabs.onUpdated", vec![json!(cid), change.clone(), info], Some(&ext_id));
        }
    }

    pub(super) fn is_uninstalling(&self, ext_id: &str) -> bool {
        self.pending_uninstall_upgrade.values().any(|id| id == ext_id)
            || self.pending_uninstall_cmd.values().any(|id| id == ext_id)
    }

    pub fn is_loaded_extension(&self, ext_id: &str) -> bool {
        self.extensions.get(ext_id).is_some_and(|e| e.loaded) && !self.is_uninstalling(ext_id)
    }

    fn emit_event(&mut self, event: &str, args: Vec<Value>, only_ext: Option<&str>) {
        let Some(by_ext) = self.subscribers.get(event) else {
            return;
        };
        let targets: Vec<String> = by_ext
            .iter()
            .filter(|(eid, _)| only_ext.is_none_or(|wanted| wanted == eid.as_str()))
            .flat_map(|(_, sids)| sids.iter().cloned())
            .collect();
        let msg = json!({ "kind": "event", "name": event, "args": args });
        for sid in targets {
            self.push_to_session(&sid, &msg);
        }
    }

    /// Delivers a user-initiated event to `ext_id` immediately when a live subscriber exists, or
    /// queues it (bounded to 16 entries / 64 KiB / 10 s TTL) ONLY if `ext_id` is currently loaded
    /// AND its service worker is known to listen for `event`, returning `WakeExtensionWorker`.
    fn emit_or_wake_event(&mut self, event: &str, args: Vec<Value>, ext_id: &str) -> Option<ExtEffect> {
        if !self.is_loaded_extension(ext_id) {
            return None;
        }
        let has_live = self
            .subscribers
            .get(event)
            .and_then(|by_ext| by_ext.get(ext_id))
            .is_some_and(|sids| !sids.is_empty());
        if has_live {
            self.emit_event(event, args, Some(ext_id));
            return None;
        }
        if self.sw_wake.enqueue_if_listening(ext_id, event, args) {
            self.metrics.sw_wakes += 1;
            Some(ExtEffect::WakeExtensionWorker { ext_id: ext_id.to_owned() })
        } else {
            None
        }
    }

    pub(super) fn broadcast_active_tab_for(&mut self, ext_id: &str, cid: i32) {
        let sids: Vec<String> = self
            .sessions
            .iter()
            .filter(|(_, s)| {
                s.ext_id == ext_id
                    && s.shimmed
                    && !matches!(s.kind, SessionKind::SeedProbe | SessionKind::FileGuardOnly)
            })
            .map(|(sid, _)| sid.clone())
            .collect();
        let msg = json!({ "kind": "active_tab", "tabId": cid });
        for sid in sids {
            self.push_to_session(&sid, &msg);
        }
    }

    pub(super) fn reply_ok(&mut self, sid: &str, req_id: i64, value: Value) {
        self.push_to_session(sid, &json!({ "kind": "reply", "reqId": req_id, "ok": true, "value": value }));
    }

    pub(super) fn reply_err(&mut self, sid: &str, req_id: i64, error: &str) {
        self.push_to_session(sid, &json!({ "kind": "reply", "reqId": req_id, "ok": false, "error": error }));
    }

    pub(super) fn push_to_session(&mut self, sid: &str, msg: &Value) {
        let expr = format!("globalThis.__ocExtRecv?.({msg})");
        let id = self.alloc_id();
        self.pipe.send_rpc(&json!({
            "id": id,
            "sessionId": sid,
            "method": "Runtime.evaluate",
            "params": { "expression": expr }
        }));
    }

    pub(super) fn visible_tabs(&self) -> impl Iterator<Item = &ExtTab> {
        self.tabs.iter().filter(|t| !t.agent_driven)
    }

    pub(super) fn ext_chromium_tab_id(&self, ext_id: &str, engine_tab_id: u64) -> Option<i32> {
        self.ext_tab_ids.get(ext_id)?.get(&engine_tab_id).copied()
    }

    pub(super) fn find_visible_tab_by_ext_cid(&self, ext_id: &str, cid: i32) -> Option<&ExtTab> {
        let map = self.ext_tab_ids.get(ext_id)?;
        let (&engine_tab_id, _) = map.iter().find(|&(_, &v)| v == cid)?;
        self.tabs.iter().find(|t| t.engine_tab_id == engine_tab_id && !t.agent_driven)
    }

    pub(super) fn find_tab_pos_by_ext_cid(&self, ext_id: &str, cid: i32) -> Option<usize> {
        let map = self.ext_tab_ids.get(ext_id)?;
        let (&engine_tab_id, _) = map.iter().find(|&(_, &v)| v == cid)?;
        self.tabs.iter().position(|t| t.engine_tab_id == engine_tab_id)
    }

    pub(super) fn active_chromium_tab_id_for(&self, ext_id: &str) -> Option<i32> {
        let tab = self.tabs.iter().find(|t| t.active && !t.agent_driven)?;
        self.ext_chromium_tab_id(ext_id, tab.engine_tab_id)
    }

    pub(super) fn has_unbound_visible_tab_for(&self, ext_id: &str) -> bool {
        let map = self.ext_tab_ids.get(ext_id);
        self.visible_tabs()
            .filter(|t| !t.lifecycle.is_discarded())
            .any(|t| map.is_none_or(|m| !m.contains_key(&t.engine_tab_id)))
    }

    pub(super) fn register_extension(
        &mut self,
        id: String,
        dir: PathBuf,
        staged_dir: Option<PathBuf>,
        sha256: String,
        manifest: ExtensionManifest,
        is_unpacked: bool,
        loaded: bool,
    ) {
        self.context_menus.lock().unwrap().configure_extension(
            &id,
            &manifest.name,
            &manifest.version,
            manifest.has_permission("contextMenus"),
        );
        self.sw_wake.configure_extension(&id, &manifest.version);
        if let Some(prev) = self.extensions.insert(
            id,
            LoadedExt { dir, staged_dir, sha256, manifest, is_unpacked, loaded, boot_count: 0 },
        ) && let Some(old_staged) = prev.staged_dir
        {
            integrity::cleanup_staged_dir(&old_staged);
        }
    }

    pub fn context_menu_catalog(&self) -> Arc<std::sync::Mutex<context_menus::ExtensionContextMenuCatalog>> {
        self.context_menus.clone()
    }

    /// Dispatches a user click on a page context-menu extension item (`context-menus.ts:206-233`).
    pub fn on_context_menu_clicked(
        &mut self,
        engine_tab_id: u64,
        ext_id: &str,
        item_id: &str,
        target: &context_menus::MenuTarget,
    ) -> Vec<ExtEffect> {
        if !self.is_loaded_extension(ext_id) {
            return Vec::new();
        }
        let Some(ext) = self.extensions.get(ext_id).cloned() else {
            return Vec::new();
        };
        if !ext.manifest.has_permission("contextMenus") {
            return Vec::new();
        }
        let Some(tab) = self.tabs.iter().find(|t| t.engine_tab_id == engine_tab_id && !t.agent_driven).cloned() else {
            return Vec::new();
        };
        if is_private_url(&tab.url) || is_extension_scheme(&tab.url) || target.has_private_url() {
            return Vec::new();
        }
        let Some(info) = self
            .context_menus
            .lock()
            .unwrap()
            .choose_and_build_click_info(ext_id, item_id, target)
        else {
            return Vec::new();
        };
        let reveal_via_active_tab =
            ext.manifest.has_permission("activeTab") && target.page_url == tab.url && !is_extension_scheme(&tab.url);
        let tab_info = self.tab_json(ext_id, &tab, &ext.manifest, reveal_via_active_tab);
        self.emit_or_wake_event("contextMenus.onClicked", vec![info, tab_info], ext_id)
            .into_iter()
            .collect()
    }

    pub(super) fn allow_open_tab(&mut self, ext_id: &str) -> bool {
        self.open_tab_rates
            .entry(ext_id.to_owned())
            .or_default()
            .allow(Instant::now(), Duration::from_secs(10), 8)
    }

    pub(super) fn allow_action_update(&mut self, ext_id: &str) -> bool {
        self.action_rates
            .entry(ext_id.to_owned())
            .or_default()
            .allow(Instant::now(), Duration::from_secs(1), 30)
    }

    pub(super) fn allow_popup_resize(&mut self, ext_id: &str) -> bool {
        self.popup_resize_rates
            .entry(ext_id.to_owned())
            .or_default()
            .allow(Instant::now(), Duration::from_secs(1), 30)
    }

    pub(super) fn allow_notification(&mut self, ext_id: &str) -> bool {
        self.notification_rates
            .entry(ext_id.to_owned())
            .or_default()
            .allow(Instant::now(), Duration::from_secs(10), 10)
    }

    pub fn set_notifications_enabled(&mut self, enabled: bool) {
        self.notifications_enabled = enabled;
    }

    /// Drops extension `ext_id`'s notifications, for the app to take its toasts down.
    pub(super) fn forget_notifications(&mut self, ext_id: &str) -> Vec<ExtEffect> {
        self.notifications
            .remove(ext_id)
            .unwrap_or_default()
            .into_iter()
            .map(|notif_id| ExtEffect::NotificationClear { ext_id: ext_id.to_owned(), notif_id })
            .collect()
    }

    /// Delivers a desktop OS notification click or dismissal back to the owning extension
    /// (`extension-api/notifications.ts:43-48`).
    pub fn on_notification_event(&mut self, ext_id: &str, notif_id: &str, clicked: bool, by_user: bool) -> Vec<ExtEffect> {
        if !self.is_loaded_extension(ext_id) {
            return Vec::new();
        }
        let Some(ext) = self.extensions.get(ext_id) else {
            return Vec::new();
        };
        if !ext.manifest.has_permission("notifications") {
            return Vec::new();
        }
        let existed = self
            .notifications
            .get_mut(ext_id)
            .is_some_and(|list| {
                let before = list.len();
                list.retain(|id| id != notif_id);
                list.len() != before
            });
        let mut wake = None;
        if clicked {
            wake = self.emit_or_wake_event("notifications.onClicked", vec![json!(notif_id)], ext_id);
        }
        if existed || clicked {
            let w2 = self.emit_or_wake_event("notifications.onClosed", vec![json!(notif_id), json!(by_user)], ext_id);
            if wake.is_none() {
                wake = w2;
            }
        }
        wake.into_iter().collect()
    }

    pub(super) fn alloc_id(&mut self) -> i64 {
        let id = self.next_cdp_id;
        self.next_cdp_id += 1;
        id
    }

    pub(super) fn send_cdp(&mut self, method: &str, params: Value, session_id: Option<&str>) -> i64 {
        let id = self.alloc_id();
        let mut payload = json!({ "id": id, "method": method, "params": params });
        if let Some(sid) = session_id {
            payload["sessionId"] = Value::String(sid.to_owned());
        }
        self.pipe.send(&payload);
        id
    }
}

pub(super) fn shim_source(is_popup: bool, declared_perms: &[String]) -> String {
    let perms_json = serde_json::to_string(declared_perms).unwrap_or_else(|_| "[]".into());
    SHIM_TEMPLATE
        .replace("__IS_POPUP__", if is_popup { "true" } else { "false" })
        .replace("__DECLARED_PERMS__", &perms_json)
}

fn now_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64() * 1000.0)
}
