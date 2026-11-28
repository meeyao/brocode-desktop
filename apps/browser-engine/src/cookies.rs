//! Profile cookie store management (`CefCookieManager`), persistence probe, "Sign out everywhere",
//! and loop-free two-way remote cookie mirroring (`link-machine.ts` parity).
//!
//! - Uses Chromium's real OS-backed encrypted cookie store (DPAPI on Windows, Keychain on macOS,
//!   Secret Service / KWallet on Linux — `--use-mock-keychain` is removed).
//! - Excludes the short-lived internal probe cookie (`cookie-check.opencode.invalid`) from all
//!   user-visible listings and remote mirrors.
//! - Keeps remote server partitions unlinked by default (`share_sign_ins = false`). When enabled,
//!   reconciles `Default` <-> each `RemoteServer` partition using `reconcile_cookie` (`link-machine.ts`),
//!   never mirroring `IsolatedPreview` contexts. Turning sharing off clears remote partition cookies.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use cef::rc::Rc;
use cef::{
    Basetime, CefString, CompletionCallback, Cookie, CookieManager, CookiePriority, CookieSameSite,
    CookieVisitor, DeleteCookiesCallback, ImplCompletionCallback, ImplCookieManager,
    ImplCookieVisitor, ImplDeleteCookiesCallback, ImplRequestContext, ImplSetCookieCallback,
    RequestContext, SetCookieCallback, WrapCompletionCallback, WrapCookieVisitor,
    WrapDeleteCookiesCallback, WrapSetCookieCallback, wrap_completion_callback,
    wrap_cookie_visitor, wrap_delete_cookies_callback, wrap_set_cookie_callback,
};

use crate::context::ContextPool;
use crate::engine::sign_in_gate::{HeldNav, SignInGates};
use crate::output::Output;
use crate::wire::{CookieSameSite as WireSameSite, EngineMsg, WireCookie};

pub const PROBE_COOKIE_HOST: &str = "cookie-check.opencode.invalid";
pub const PROBE_COOKIE_NAME: &str = "opencode_cookie_check";

/// Seconds between 1601-01-01 (Chromium's `base::Time` epoch) and 1970-01-01 (Unix epoch).
const WINDOWS_EPOCH_OFFSET_SECS: f64 = 11_644_473_600.0;
const MIRROR_POLL_INTERVAL: Duration = Duration::from_millis(1500);
const MAX_IMPORT_COOKIES: usize = 50_000;
const MAX_COOKIE_HOST: usize = 255;
const MAX_COOKIE_NAME: usize = 4096;
const MAX_COOKIE_VALUE: usize = 8192;
const MAX_COOKIE_PATH: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkSide {
    Profile,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct CookieKey {
    host: String,
    path: String,
    name: String,
}

impl CookieKey {
    fn from_cookie(c: &WireCookie) -> Self {
        Self {
            host: c.host.clone(),
            path: c.path.clone(),
            name: c.name.clone(),
        }
    }
}

fn is_probe_cookie(c: &WireCookie) -> bool {
    c.host.trim_start_matches('.').eq_ignore_ascii_case(PROBE_COOKIE_HOST)
        && c.name == PROBE_COOKIE_NAME
}

fn now_unix_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64())
}

pub fn cookie_url(cookie: &WireCookie) -> String {
    let bare = cookie.host.trim_start_matches('.');
    let bracketed = if bare.contains(':') && !bare.starts_with('[') {
        format!("[{bare}]")
    } else {
        bare.to_owned()
    };
    let scheme = if cookie.secure { "https" } else { "http" };
    let path = if cookie.path.starts_with('/') {
        cookie.path.as_str()
    } else {
        "/"
    };
    format!("{scheme}://{bracketed}{path}")
}

fn to_cef_cookie(cookie: &WireCookie, now_secs: f64) -> Option<(String, Cookie)> {
    let bare = cookie.host.trim_start_matches('.');
    if bare.is_empty()
        || bare.len() > MAX_COOKIE_HOST
        || cookie.name.len() > MAX_COOKIE_NAME
        || cookie.value.len() > MAX_COOKIE_VALUE
        || cookie.path.len() > MAX_COOKIE_PATH
        || (cookie.name.is_empty() && cookie.value.is_empty())
    {
        return None;
    }
    if !valid_cookie_host(&cookie.host)
        || cookie.name.chars().any(|c| matches!(c, '\0' | '\r' | '\n'))
        || cookie.path.chars().any(|c| matches!(c, '\0' | '\r' | '\n'))
    {
        return None;
    }
    if let Some(exp) = cookie.expires
        && (!exp.is_finite() || exp <= now_secs)
    {
        return None;
    }
    let path = if cookie.path.starts_with('/') {
        cookie.path.as_str()
    } else {
        "/"
    };
    // Host-only cookies omit `domain` so Chromium binds them to the URL's host; `__Host-` cookies
    // must remain host-only per RFC 6265bis (`cookie.ts:58`).
    let domain = if cookie.host.starts_with('.') && !cookie.name.starts_with("__Host-") {
        CefString::from(cookie.host.as_str())
    } else {
        CefString::default()
    };
    let (has_expires, expires) = match cookie.expires {
        Some(sec) => {
            let micros = ((sec + WINDOWS_EPOCH_OFFSET_SECS) * 1_000_000.0).round() as i64;
            (1, Basetime { val: micros })
        }
        None => (0, Basetime { val: 0 }),
    };
    let same_site = match cookie.same_site {
        WireSameSite::Unspecified => CookieSameSite::UNSPECIFIED,
        WireSameSite::NoRestriction => CookieSameSite::NO_RESTRICTION,
        WireSameSite::Lax => CookieSameSite::LAX_MODE,
        WireSameSite::Strict => CookieSameSite::STRICT_MODE,
    };
    let cef_cookie = Cookie {
        size: std::mem::size_of::<cef::sys::_cef_cookie_t>(),
        name: CefString::from(cookie.name.as_str()),
        value: CefString::from(cookie.value.as_str()),
        domain,
        path: CefString::from(path),
        secure: i32::from(cookie.secure),
        httponly: i32::from(cookie.http_only),
        creation: Basetime { val: 0 },
        last_access: Basetime { val: 0 },
        has_expires,
        expires,
        same_site,
        priority: CookiePriority::MEDIUM,
    };
    let url = cookie_url(cookie);
    // The URL Chromium binds the cookie to must name exactly this host: no user info, no port.
    let parsed = url::Url::parse(&url).ok()?;
    let same_host = match parsed.host()? {
        url::Host::Domain(d) => d.eq_ignore_ascii_case(bare),
        url::Host::Ipv4(ip) => bare.parse::<std::net::Ipv4Addr>().is_ok_and(|b| b == ip),
        url::Host::Ipv6(ip) => bare.trim_start_matches('[').trim_end_matches(']').parse::<std::net::Ipv6Addr>().is_ok_and(|b| b == ip),
    };
    if !parsed.username().is_empty() || parsed.password().is_some() || parsed.port().is_some() || !same_host {
        return None;
    }
    Some((url, cef_cookie))
}

/// A cookie host a browser would store (`browser_pane::cookies::format::valid_cookie_host`, kept in
/// step): a plain hostname or an IP literal, at most one leading dot; never `user@host`, `?`, `#`,
/// `%`, a port, a stray `:`, controls or spaces.
fn valid_cookie_host(host: &str) -> bool {
    let bare = host.strip_prefix('.').unwrap_or(host);
    if bare.is_empty() || bare.len() > 253 {
        return false;
    }
    if let Some(inner) = bare.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
        return inner.parse::<std::net::Ipv6Addr>().is_ok();
    }
    if bare.contains(':') {
        return bare.parse::<std::net::Ipv6Addr>().is_ok();
    }
    bare.split('.').all(|label| {
        !label.is_empty() && label.len() <= 63 && label.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    })
}

fn from_cef_cookie(c: &Cookie) -> WireCookie {
    let expires = if c.has_expires != 0 && c.expires.val > 0 {
        Some((c.expires.val as f64) / 1_000_000.0 - WINDOWS_EPOCH_OFFSET_SECS)
    } else {
        None
    };
    let same_site = if c.same_site == CookieSameSite::NO_RESTRICTION {
        WireSameSite::NoRestriction
    } else if c.same_site == CookieSameSite::LAX_MODE {
        WireSameSite::Lax
    } else if c.same_site == CookieSameSite::STRICT_MODE {
        WireSameSite::Strict
    } else {
        WireSameSite::Unspecified
    };
    let path = c.path.to_string();
    WireCookie {
        host: c.domain.to_string(),
        name: c.name.to_string(),
        value: c.value.to_string(),
        path: if path.is_empty() { "/".to_owned() } else { path },
        secure: c.secure != 0,
        http_only: c.httponly != 0,
        expires,
        same_site,
    }
}

fn same_cookie(a: Option<&WireCookie>, b: Option<&WireCookie>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            a.value == b.value
                && a.secure == b.secure
                && a.http_only == b.http_only
                && a.same_site == b.same_site
                && a.expires.map(|x| x.round() as i64) == b.expires.map(|x| x.round() as i64)
        }
        _ => false,
    }
}

struct CookieStep {
    base: Option<WireCookie>,
    write: Option<(LinkSide, Option<WireCookie>)>,
}

/// Exact port of upstream `link-machine.ts::reconcileCookie`.
fn reconcile_cookie(
    base: Option<&WireCookie>,
    profile: Option<&WireCookie>,
    other: Option<&WireCookie>,
    changed: LinkSide,
) -> CookieStep {
    if same_cookie(profile, other) {
        return CookieStep {
            base: profile.cloned(),
            write: None,
        };
    }
    let profile_moved = !same_cookie(profile, base);
    let other_moved = !same_cookie(other, base);
    let from = if profile_moved && other_moved {
        changed
    } else if profile_moved {
        LinkSide::Profile
    } else {
        LinkSide::Other
    };
    let cookie = match from {
        LinkSide::Profile => profile.cloned(),
        LinkSide::Other => other.cloned(),
    };
    let to = match from {
        LinkSide::Profile => LinkSide::Other,
        LinkSide::Other => LinkSide::Profile,
    };
    CookieStep {
        base: cookie.clone(),
        write: Some((to, cookie)),
    }
}

// --- Zero-cookie-safe CEF visitor & completion callbacks ---

struct VisitDone {
    cookies: Mutex<Vec<WireCookie>>,
    include_probe: bool,
    on_done: Mutex<Option<Box<dyn FnOnce(Vec<WireCookie>) + Send>>>,
}

impl Drop for VisitDone {
    fn drop(&mut self) {
        if let Some(cb) = self.on_done.lock().unwrap().take() {
            let cookies = std::mem::take(&mut *self.cookies.lock().unwrap());
            cb(cookies);
        }
    }
}

wrap_cookie_visitor! {
    struct CollectCookiesVisitor {
        state: Arc<VisitDone>,
    }

    impl CookieVisitor {
        fn visit(
            &self,
            cookie: Option<&Cookie>,
            _count: std::os::raw::c_int,
            _total: std::os::raw::c_int,
            _delete_cookie: Option<&mut std::os::raw::c_int>,
        ) -> std::os::raw::c_int {
            if let Some(c) = cookie {
                let wire = from_cef_cookie(c);
                if self.state.include_probe || !is_probe_cookie(&wire) {
                    self.state.cookies.lock().unwrap().push(wire);
                }
            }
            1
        }
    }
}

pub(crate) fn visit_all<F>(mgr: &CookieManager, include_probe: bool, on_done: F)
where
    F: FnOnce(Vec<WireCookie>) + Send + 'static,
{
    let state = Arc::new(VisitDone {
        cookies: Mutex::new(Vec::new()),
        include_probe,
        on_done: Mutex::new(Some(Box::new(on_done))),
    });
    let mut visitor = CollectCookiesVisitor::new(state);
    if mgr.visit_all_cookies(Some(&mut visitor)) == 0 {
        // `visitor` drops immediately and triggers `VisitDone::drop` with an empty list.
    }
}

struct CompletionOnce {
    cb: Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

impl Drop for CompletionOnce {
    fn drop(&mut self) {
        if let Some(cb) = self.cb.lock().unwrap().take() {
            cb();
        }
    }
}

wrap_completion_callback! {
    struct OnceCompletion {
        inner: Arc<CompletionOnce>,
    }

    impl CompletionCallback {
        fn on_complete(&self) {
            if let Some(cb) = self.inner.cb.lock().unwrap().take() {
                cb();
            }
        }
    }
}

pub(crate) fn completion_cb<F: FnOnce() + Send + 'static>(cb: F) -> CompletionCallback {
    OnceCompletion::new(Arc::new(CompletionOnce {
        cb: Mutex::new(Some(Box::new(cb))),
    }))
}

struct SetOnce {
    cb: Mutex<Option<Box<dyn FnOnce(bool) + Send>>>,
}

impl Drop for SetOnce {
    fn drop(&mut self) {
        if let Some(cb) = self.cb.lock().unwrap().take() {
            cb(false);
        }
    }
}

wrap_set_cookie_callback! {
    struct OnceSetCookie {
        inner: Arc<SetOnce>,
    }

    impl SetCookieCallback {
        fn on_complete(&self, success: std::os::raw::c_int) {
            if let Some(cb) = self.inner.cb.lock().unwrap().take() {
                cb(success != 0);
            }
        }
    }
}

fn set_cookie_cb<F: FnOnce(bool) + Send + 'static>(cb: F) -> SetCookieCallback {
    OnceSetCookie::new(Arc::new(SetOnce {
        cb: Mutex::new(Some(Box::new(cb))),
    }))
}

struct DeleteOnce {
    cb: Mutex<Option<Box<dyn FnOnce(i32) + Send>>>,
}

impl Drop for DeleteOnce {
    fn drop(&mut self) {
        if let Some(cb) = self.cb.lock().unwrap().take() {
            cb(-1);
        }
    }
}

wrap_delete_cookies_callback! {
    struct OnceDeleteCookies {
        inner: Arc<DeleteOnce>,
    }

    impl DeleteCookiesCallback {
        fn on_complete(&self, num_deleted: std::os::raw::c_int) {
            if let Some(cb) = self.inner.cb.lock().unwrap().take() {
                cb(num_deleted);
            }
        }
    }
}

fn delete_cookies_cb<F: FnOnce(i32) + Send + 'static>(cb: F) -> DeleteCookiesCallback {
    OnceDeleteCookies::new(Arc::new(DeleteOnce {
        cb: Mutex::new(Some(Box::new(cb))),
    }))
}

// --- Controller & Mirror ---

/// Why a mirror ended (`MirrorPartition::cancel`): still mirroring, sharing turned off, or signed out
/// everywhere. A reconcile in flight reads it before every read and write, as official's
/// `state.stopped` (`profile/link.ts:51,55`), and once its last write lands it removes what it
/// raced in (`stop(forget)` waits for the queues first, `link.ts:104-115`).
const LIVE: u8 = 0;
const STOPPED: u8 = 1;
const SIGNED_OUT: u8 = 2;

struct MirrorPartition {
    base: Arc<Mutex<HashMap<CookieKey, WireCookie>>>,
    in_flight: Arc<AtomicBool>,
    /// Set once the partition's first profile -> remote sign-in copy ends (finished, failed, or cancelled).
    initial_settled: Arc<AtomicBool>,
    pending_side: Arc<Mutex<Option<LinkSide>>>,
    cancel: Arc<AtomicU8>,
}

impl MirrorPartition {
    fn new() -> Self {
        Self {
            base: Arc::new(Mutex::new(HashMap::new())),
            in_flight: Arc::new(AtomicBool::new(false)),
            initial_settled: Arc::new(AtomicBool::new(false)),
            pending_side: Arc::new(Mutex::new(None)),
            cancel: Arc::new(AtomicU8::new(LIVE)),
        }
    }
}

/// Marks a partition's initial profile -> remote sign-in copy settled on drop (whether the copy
/// finished, wrote zero cookies, failed, or was cancelled) and wakes the engine loop immediately
/// so held HTTP/HTTPS navigations release without stalling (`UP-151`, official `profile/link.ts`).
struct InitialCopyGuard {
    settled: Arc<AtomicBool>,
    pump: Sender<crate::Event>,
}

impl Drop for InitialCopyGuard {
    fn drop(&mut self) {
        if !self.settled.swap(true, Ordering::AcqRel) {
            let _ = self.pump.send(crate::Event::Pump(0));
        }
    }
}

/// A cookie for this machine's own loopback (`localhost`, `*.localhost`, `127.0.0.0/8`, `::1`):
/// on a remote server's partition it belongs to that other machine, so it never crosses the mirror
/// either way (stricter than official's `link.ts`, which mirrors every key).
fn is_loopback_host(host: &str) -> bool {
    // `.localhost.` and `[::ffff:127.0.0.1]` are the same hosts as `localhost` and `127.0.0.1`.
    let h = host.trim_start_matches('.').trim_end_matches('.').trim_start_matches('[').trim_end_matches(']').to_ascii_lowercase();
    h == "localhost"
        || h.ends_with(".localhost")
        || match h.parse::<std::net::IpAddr>() {
            Ok(std::net::IpAddr::V6(v6)) => v6.is_loopback() || v6.to_ipv4_mapped().is_some_and(|v4| v4.is_loopback()),
            Ok(ip) => ip.is_loopback(),
            Err(_) => false,
        }
}

/// Counts what still acts on a remote partition from an ended mirror (a batch of writes in flight,
/// a wipe queued): while it is above zero no new mirror starts there, so an old batch's wipe can
/// never land after a new mirror seeded the partition and read as the remote signing out of
/// everything (which would delete the person's own sign-ins from the profile).
struct Draining {
    count: Arc<AtomicUsize>,
    pump: Sender<crate::Event>,
}

impl Draining {
    fn new(count: Arc<AtomicUsize>, pump: Sender<crate::Event>) -> Self {
        Self { count, pump }
    }

    fn start(&self) -> DrainGuard {
        self.count.fetch_add(1, Ordering::AcqRel);
        DrainGuard {
            count: self.count.clone(),
            pump: self.pump.clone(),
        }
    }
}

/// Ends one draining action when dropped (after its callback ran, or with it if it never runs),
/// waking the engine loop when draining reaches zero so any waiting initial copy starts immediately.
struct DrainGuard {
    count: Arc<AtomicUsize>,
    pump: Sender<crate::Event>,
}

impl Drop for DrainGuard {
    fn drop(&mut self) {
        if self.count.fetch_sub(1, Ordering::AcqRel) == 1 {
            let _ = self.pump.send(crate::Event::Pump(0));
        }
    }
}

pub struct CookieController {
    share_sign_ins: bool,
    mirrors: HashMap<String, MirrorPartition>,
    /// Per remote partition: actions of ended mirrors still running (`Draining`).
    draining: HashMap<String, Arc<AtomicUsize>>,
    /// Per-partition readiness gate holding initial remote HTTP/HTTPS navigations until the
    /// initial async sign-in copy settles (`UP-151`, official `pane.ts:889-891`).
    gates: SignInGates,
    /// Navigations released from `gates` ready to be dispatched by `EngineManager`.
    released_loads: Vec<HeldNav>,
    dirty_all_profile: Arc<AtomicBool>,
    last_periodic: Instant,
    pump: Sender<crate::Event>,
}

impl CookieController {
    pub fn new(pump: Sender<crate::Event>) -> Self {
        Self {
            share_sign_ins: false,
            mirrors: HashMap::new(),
            draining: HashMap::new(),
            gates: SignInGates::default(),
            released_loads: Vec::new(),
            dirty_all_profile: Arc::new(AtomicBool::new(false)),
            last_periodic: Instant::now(),
            pump,
        }
    }

    /// Ensures the initial profile -> remote sign-in copy has been armed and started for
    /// `partition` when sharing is on (called when a `RemoteServer` tab opens).
    pub fn ensure_initial_copy(&mut self, contexts: &ContextPool, partition: &str) {
        if !self.share_sign_ins {
            return;
        }
        let partition = partition.trim();
        let Some(ctx) = contexts.remote_context(partition) else {
            return;
        };
        if !self.mirrors.contains_key(partition) {
            self.gates.begin_copy(partition, Instant::now());
        }
        self.reconcile_partition(partition, ctx, LinkSide::Profile, None);
    }

    /// Holds an HTTP/HTTPS navigation on `partition` while its initial sign-in copy is still in
    /// flight; non-HTTP URLs and already-settled partitions return `false` immediately.
    pub fn gate_navigation(&mut self, partition: &str, nav: HeldNav) -> bool {
        let partition = partition.trim();
        self.sync_partition_settled(partition);
        self.gates.gate_navigation(partition, nav)
    }

    /// Cancels any queued sign-in navigation for `tab_id` without stopping the partition's copy.
    pub fn cancel_tab_gate(&mut self, tab_id: u64) -> bool {
        self.released_loads.retain(|nav| nav.tab_id != tab_id);
        self.gates.cancel_tab(tab_id)
    }

    /// Whether `tab_id` currently has a navigation queued in a partition's sign-in gate.
    pub fn is_tab_gated(&self, tab_id: u64) -> bool {
        self.gates.is_tab_held(tab_id)
    }

    /// Drains navigations released when a partition's initial copy finished, failed, timed out,
    /// or sharing was turned off.
    pub fn take_released_loads(&mut self) -> Vec<HeldNav> {
        std::mem::take(&mut self.released_loads)
    }

    fn sync_partition_settled(&mut self, partition: &str) {
        if self
            .mirrors
            .get(partition)
            .is_some_and(|m| m.initial_settled.load(Ordering::Acquire))
            && self.gates.is_copying(partition)
        {
            self.released_loads.extend(self.gates.settle(partition));
        }
    }

    fn settle_gate_if_unstarted(&mut self, partition: &str) {
        if !self.mirrors.contains_key(partition) && self.gates.is_copying(partition) {
            self.released_loads.extend(self.gates.settle(partition));
            if !self.released_loads.is_empty() {
                let _ = self.pump.send(crate::Event::Pump(0));
            }
        }
    }


    fn resolve_manager(contexts: &ContextPool, partition: Option<&str>) -> Option<CookieManager> {
        match partition {
            None => cef::cookie_manager_get_global_manager(None),
            Some(p) => contexts.remote_context(p).and_then(|ctx| ctx.cookie_manager(None)),
        }
    }

    pub fn list(
        &self,
        contexts: &ContextPool,
        partition: Option<&str>,
        request_id: u64,
        out_tx: Output,
    ) {
        let Some(mgr) = Self::resolve_manager(contexts, partition) else {
            let _ = out_tx.send(EngineMsg::ProfileCookies {
                request_id,
                cookies: Vec::new(),
            });
            return;
        };
        visit_all(&mgr, false, move |cookies| {
            let _ = out_tx.send(EngineMsg::ProfileCookies { request_id, cookies });
        });
    }

    pub fn set_batch(
        &self,
        contexts: &ContextPool,
        partition: Option<&str>,
        cookies: Vec<WireCookie>,
        request_id: u64,
        out_tx: Output,
    ) {
        let Some(mgr) = Self::resolve_manager(contexts, partition) else {
            let _ = out_tx.send(EngineMsg::ProfileCookiesImported {
                request_id,
                imported: 0,
                failed: cookies.len() as u32,
            });
            return;
        };
        let now = now_unix_secs();
        let overflow = cookies.len().saturating_sub(MAX_IMPORT_COOKIES) as u32;
        let mut prepared = Vec::with_capacity(cookies.len().min(MAX_IMPORT_COOKIES));
        let mut pre_failed = overflow;
        for c in cookies.iter().take(MAX_IMPORT_COOKIES) {
            if is_probe_cookie(c) {
                continue;
            }
            if let Some(pair) = to_cef_cookie(c, now) {
                prepared.push(pair);
            } else {
                pre_failed = pre_failed.saturating_add(1);
            }
        }
        if prepared.is_empty() {
            let _ = out_tx.send(EngineMsg::ProfileCookiesImported {
                request_id,
                imported: 0,
                failed: pre_failed,
            });
            return;
        }
        let remaining = Arc::new(AtomicUsize::new(prepared.len()));
        let imported = Arc::new(AtomicU32::new(0));
        let failed = Arc::new(AtomicU32::new(pre_failed));
        let mark_mirror = partition.is_none() && self.share_sign_ins;
        let dirty_flag = self.dirty_all_profile.clone();
        let pump = self.pump.clone();

        for (url, cef_cookie) in prepared {
            let rem = remaining.clone();
            let imp = imported.clone();
            let fail = failed.clone();
            let mgr_done = mgr.clone();
            let out_done = out_tx.clone();
            let dirty_done = dirty_flag.clone();
            let pump_done = pump.clone();
            let mut cb = set_cookie_cb(move |ok| {
                if ok {
                    imp.fetch_add(1, Ordering::Relaxed);
                } else {
                    fail.fetch_add(1, Ordering::Relaxed);
                }
                if rem.fetch_sub(1, Ordering::AcqRel) == 1 {
                    let imp_total = imp.load(Ordering::Relaxed);
                    let fail_total = fail.load(Ordering::Relaxed);
                    let mut flush = completion_cb(move || {
                        if mark_mirror {
                            dirty_done.store(true, Ordering::Release);
                            let _ = pump_done.send(crate::Event::Pump(0));
                        }
                        let _ = out_done.send(EngineMsg::ProfileCookiesImported {
                            request_id,
                            imported: imp_total,
                            failed: fail_total,
                        });
                    });
                    if mgr_done.flush_store(Some(&mut flush)) == 0 {
                        // Drop of `flush` fires `CompletionOnce::drop`.
                    }
                }
            });
            let url_cef = CefString::from(url.as_str());
            if mgr.set_cookie(Some(&url_cef), Some(&cef_cookie), Some(&mut cb)) == 0 {
                // Drop of `cb` triggers `SetOnce::drop` (`ok = false`).
            }
        }
    }

    pub fn delete_one(
        &self,
        contexts: &ContextPool,
        partition: Option<&str>,
        url: &str,
        name: &str,
        request_id: u64,
        out_tx: Output,
    ) {
        let lower = url.trim().to_ascii_lowercase();
        if !(lower.starts_with("http://") || lower.starts_with("https://")) {
            let _ = out_tx.send(EngineMsg::ProfileActionDone { request_id, ok: false });
            return;
        }
        let Some(mgr) = Self::resolve_manager(contexts, partition) else {
            let _ = out_tx.send(EngineMsg::ProfileActionDone { request_id, ok: false });
            return;
        };
        let mark_mirror = partition.is_none() && self.share_sign_ins;
        let dirty_flag = self.dirty_all_profile.clone();
        let pump = self.pump.clone();
        let mgr_done = mgr.clone();
        let mut del_cb = delete_cookies_cb(move |deleted| {
            let ok = deleted >= 0;
            let mut flush = completion_cb(move || {
                if mark_mirror {
                    dirty_flag.store(true, Ordering::Release);
                    let _ = pump.send(crate::Event::Pump(0));
                }
                let _ = out_tx.send(EngineMsg::ProfileActionDone { request_id, ok });
            });
            let _ = mgr_done.flush_store(Some(&mut flush));
        });
        let url_cef = CefString::from(url);
        let name_cef = CefString::from(name);
        let _ = mgr.delete_cookies(Some(&url_cef), Some(&name_cef), Some(&mut del_cb));
    }

    /// Writes the short-lived persistence probe cookie (`cookie-check.opencode.invalid`) and
    /// flushes the default cookie store so the host can inspect disk persistence (`probe.ts`).
    pub fn probe_persistence(&self, cache_dir: &str, request_id: u64, out_tx: Output) {
        let Some(mgr) = cef::cookie_manager_get_global_manager(None) else {
            let _ = out_tx.send(EngineMsg::ProfileProbeReady {
                request_id,
                written: false,
                cache_dir: cache_dir.to_owned(),
            });
            return;
        };
        let probe = WireCookie {
            host: PROBE_COOKIE_HOST.to_owned(),
            name: PROBE_COOKIE_NAME.to_owned(),
            value: "1".to_owned(),
            path: "/".to_owned(),
            secure: true,
            http_only: false,
            expires: Some(now_unix_secs() + 600.0),
            same_site: WireSameSite::Lax,
        };
        let Some((url, cef_cookie)) = to_cef_cookie(&probe, now_unix_secs()) else {
            let _ = out_tx.send(EngineMsg::ProfileProbeReady {
                request_id,
                written: false,
                cache_dir: cache_dir.to_owned(),
            });
            return;
        };
        let cache = cache_dir.to_owned();
        let mgr_done = mgr.clone();
        let mut cb = set_cookie_cb(move |ok| {
            if !ok {
                let _ = out_tx.send(EngineMsg::ProfileProbeReady {
                    request_id,
                    written: false,
                    cache_dir: cache,
                });
                return;
            }
            let mut flush = completion_cb(move || {
                let _ = out_tx.send(EngineMsg::ProfileProbeReady {
                    request_id,
                    written: true,
                    cache_dir: cache,
                });
            });
            let _ = mgr_done.flush_store(Some(&mut flush));
        });
        let url_cef = CefString::from(url.as_str());
        let _ = mgr.set_cookie(Some(&url_cef), Some(&cef_cookie), Some(&mut cb));
    }

    /// "Sign out everywhere": deletes all cookies, HTTP auth credentials, and cert exceptions on
    /// both the default profile and every active remote server partition (`profile/index.ts:509-524`).
    /// Note: open tabs also receive CDP `Storage.clearDataForOrigin` + `Network.clearBrowserCache`
    /// from `EngineManager` right beside this call.
    pub fn clear_all(&mut self, contexts: &ContextPool, done: Box<dyn FnOnce(bool) + Send>) {
        // End every mirror so a reconcile in flight writes nothing more and removes what it raced in;
        // the next reconcile starts a fresh one.
        for entry in self.mirrors.values() {
            entry.cancel.store(SIGNED_OUT, Ordering::Release);
        }
        self.mirrors.clear();
        let released = self.gates.settle_all();
        if !released.is_empty() {
            self.released_loads.extend(released);
            let _ = self.pump.send(crate::Event::Pump(0));
        }
        let remotes = contexts.remote_partitions();
        let global = cef::request_context_get_global_context();
        let Some(mgr) = cef::cookie_manager_get_global_manager(None) else {
            return done(false);
        };
        // `done` once every partition's sign-ins and cookies are gone and flushed: each remote's
        // HTTP sign-ins and cookies, the shared profile's HTTP sign-ins and cookies.
        let join = Join::new(2 * remotes.len() + 2, done);
        for (partition, ctx) in remotes {
            let auth = join.part();
            let mut auth_cb = completion_cb(move || auth(true));
            ctx.clear_http_auth_credentials(Some(&mut auth_cb));
            let part = join.part();
            let Some(mgr) = ctx.cookie_manager(None) else {
                part(false);
                continue;
            };
            // No new mirror starts on this partition until the wipe and its flush are done.
            let guard = Draining::new(self.draining.entry(partition.clone()).or_default().clone(), self.pump.clone()).start();
            let mgr_f = mgr.clone();
            let mut del_cb = delete_cookies_cb(move |deleted| {
                let mut flushed = completion_cb(move || {
                    drop(guard);
                    part(deleted >= 0);
                });
                let _ = mgr_f.flush_store(Some(&mut flushed));
            });
            let _ = mgr.delete_cookies(None, None, Some(&mut del_cb));
        }
        let auth = join.part();
        match global {
            Some(global_ctx) => {
                let mut auth_cb = completion_cb(move || auth(true));
                global_ctx.clear_http_auth_credentials(Some(&mut auth_cb));
            }
            None => auth(false),
        }
        let part = join.part();
        let mgr_done = mgr.clone();
        let mut del_cb = delete_cookies_cb(move |deleted| {
            let mut flush = completion_cb(move || part(deleted >= 0));
            let _ = mgr_done.flush_store(Some(&mut flush));
        });
        let _ = mgr.delete_cookies(None, None, Some(&mut del_cb));
    }
    /// Toggles two-way cookie sharing with remote server partitions (`profile/index.ts:643-647`).
    /// Turning sharing off stops mirroring AND removes shared cookies from every remote partition.
    pub fn set_share_sign_ins(&mut self, enabled: bool, contexts: &ContextPool) {
        if self.share_sign_ins == enabled {
            return;
        }
        self.share_sign_ins = enabled;
        if enabled {
            for (partition, ctx) in contexts.remote_partitions() {
                if !self.mirrors.contains_key(&partition) {
                    self.gates.begin_copy(&partition, Instant::now());
                }
                self.reconcile_partition(&partition, ctx, LinkSide::Profile, None);
            }
            return;
        }
        // Disabling: end every mirror (a reconcile in flight writes nothing more and, once its last
        // write lands, wipes the remote partition again), release any waiting navigations so
        // turning sharing off never stalls tabs, then wipe every remote partition's cookies and
        // HTTP sign-ins now.
        for entry in self.mirrors.values() {
            entry.cancel.store(STOPPED, Ordering::Release);
        }
        self.mirrors.clear();
        let released = self.gates.settle_all();
        if !released.is_empty() {
            self.released_loads.extend(released);
            let _ = self.pump.send(crate::Event::Pump(0));
        }
        for (partition, ctx) in contexts.remote_partitions() {
            let mut auth_cb = completion_cb(|| {});
            ctx.clear_http_auth_credentials(Some(&mut auth_cb));
            if let Some(mgr) = ctx.cookie_manager(None) {
                // No new mirror starts on this partition until the wipe and its flush are done.
                let guard = Draining::new(self.draining.entry(partition.clone()).or_default().clone(), self.pump.clone()).start();
                let mgr_f = mgr.clone();
                let mut del_cb = delete_cookies_cb(move |_| {
                    let mut flushed = completion_cb(move || drop(guard));
                    let _ = mgr_f.flush_store(Some(&mut flushed));
                });
                let _ = mgr.delete_cookies(None, None, Some(&mut del_cb));
            }
        }
    }

    pub fn on_partition_dropped(&mut self, partition: &str) {
        let partition = partition.trim();
        if let Some(entry) = self.mirrors.remove(partition) {
            entry.cancel.store(STOPPED, Ordering::Release);
        }
        self.gates.drop_partition(partition);
    }

    pub fn on_partition_dirty(&mut self, contexts: &ContextPool, partition: &str, side: LinkSide) {
        if !self.share_sign_ins {
            return;
        }
        if let Some(ctx) = contexts.remote_context(partition) {
            self.reconcile_partition(partition, ctx, side, None);
        }
    }

    pub fn on_profile_dirty(&mut self, contexts: &ContextPool) {
        if !self.share_sign_ins {
            return;
        }
        for (partition, ctx) in contexts.remote_partitions() {
            self.reconcile_partition(&partition, ctx, LinkSide::Profile, None);
        }
    }

    /// Explicit sync requested by host (or tests), firing `ProfileActionDone` once done.
    pub fn sync_partition_now(
        &mut self,
        contexts: &ContextPool,
        partition: &str,
        changed: LinkSide,
        request_id: u64,
        out_tx: Output,
    ) {
        if !self.share_sign_ins {
            let _ = out_tx.send(EngineMsg::ProfileActionDone { request_id, ok: false });
            return;
        }
        let Some(ctx) = contexts.remote_context(partition) else {
            let _ = out_tx.send(EngineMsg::ProfileActionDone { request_id, ok: false });
            return;
        };
        self.reconcile_partition(partition, ctx, changed, Some((request_id, out_tx)));
    }

    /// Periodic & deferred-dirty mirror reconciliation driven by `EngineManager::poll_picks`.
    pub fn poll(&mut self, contexts: &ContextPool) {
        let settled_partitions: Vec<String> = self
            .mirrors
            .iter()
            .filter_map(|(p, entry)| {
                (entry.initial_settled.load(Ordering::Acquire) && self.gates.is_copying(p))
                    .then(|| p.clone())
            })
            .collect();
        for partition in settled_partitions {
            self.released_loads.extend(self.gates.settle(&partition));
        }
        for (partition, timed_out) in self.gates.tick(Instant::now()) {
            eprintln!(
                "cookies: initial sign-in copy timed out for partition {partition}; releasing {} held navigation(s)",
                timed_out.len()
            );
            self.released_loads.extend(timed_out);
        }
        if !self.share_sign_ins {
            self.dirty_all_profile.store(false, Ordering::Relaxed);
            self.released_loads.extend(self.gates.settle_all());
            return;
        }
        if self.dirty_all_profile.swap(false, Ordering::AcqRel) {
            self.on_profile_dirty(contexts);
            return;
        }
        // Reconcile any partitions waiting for a prior wipe to drain, queued while a previous sync
        // was in flight, or periodically every 1.5 s to pick up JS `document.cookie` / XHR writes.
        let periodic = self.last_periodic.elapsed() >= MIRROR_POLL_INTERVAL;
        if periodic {
            self.last_periodic = Instant::now();
        }
        for (partition, ctx) in contexts.remote_partitions() {
            let waiting_start =
                self.gates.is_copying(&partition) && !self.mirrors.contains_key(&partition);
            let pending = self
                .mirrors
                .get(&partition)
                .and_then(|e| e.pending_side.lock().unwrap().take());
            if waiting_start {
                self.reconcile_partition(&partition, ctx, LinkSide::Profile, None);
            } else if let Some(side) = pending {
                self.reconcile_partition(&partition, ctx, side, None);
            } else if periodic {
                self.reconcile_partition(&partition, ctx, LinkSide::Other, None);
            }
        }
    }

    fn reconcile_partition(
        &mut self,
        partition: &str,
        remote_ctx: RequestContext,
        changed: LinkSide,
        notify: Option<(u64, Output)>,
    ) {
        let partition = partition.trim();
        let Some(profile_mgr) = cef::cookie_manager_get_global_manager(None) else {
            self.settle_gate_if_unstarted(partition);
            if let Some((request_id, out_tx)) = notify {
                let _ = out_tx.send(EngineMsg::ProfileActionDone { request_id, ok: false });
            }
            return;
        };
        let Some(remote_mgr) = remote_ctx.cookie_manager(None) else {
            self.settle_gate_if_unstarted(partition);
            if let Some((request_id, out_tx)) = notify {
                let _ = out_tx.send(EngineMsg::ProfileActionDone { request_id, ok: false });
            }
            return;
        };
        // A new mirror waits until every action of an ended one on this partition is done (`Draining`).
        let drain = self.draining.entry(partition.to_owned()).or_default().clone();
        if !self.mirrors.contains_key(partition) && drain.load(Ordering::Acquire) > 0 {
            if let Some((request_id, out_tx)) = notify {
                let _ = out_tx.send(EngineMsg::ProfileActionDone { request_id, ok: true });
            }
            return;
        }
        let is_new = !self.mirrors.contains_key(partition);
        let entry = self
            .mirrors
            .entry(partition.to_owned())
            .or_insert_with(MirrorPartition::new);
        if is_new {
            self.gates.begin_copy(partition, Instant::now());
        }
        if entry
            .in_flight
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            *entry.pending_side.lock().unwrap() = Some(changed);
            if let Some((request_id, out_tx)) = notify {
                let _ = out_tx.send(EngineMsg::ProfileActionDone { request_id, ok: true });
            }
            return;
        }
        let base_arc = entry.base.clone();
        let in_flight = entry.in_flight.clone();
        let cancel = entry.cancel.clone();
        let initial_guard = (!entry.initial_settled.load(Ordering::Acquire)).then(|| {
            Arc::new(InitialCopyGuard {
                settled: entry.initial_settled.clone(),
                pump: self.pump.clone(),
            })
        });
        // Held by this batch until its last callback: an ended mirror's batch keeps the partition draining.
        let batch = Arc::new(Draining::new(drain.clone(), self.pump.clone()).start());
        let pump = self.pump.clone();
        let profile_mgr_w = profile_mgr.clone();
        let remote_mgr_r = remote_mgr.clone();
        let remote_mgr_w = remote_mgr.clone();

        visit_all(&profile_mgr, false, move |profile_list| {
            visit_all(&remote_mgr_r, false, move |remote_list| {
                let batch = batch;
                let initial_guard = initial_guard;
                if cancel.load(Ordering::Acquire) != LIVE {
                    in_flight.store(false, Ordering::Release);
                    if let Some((request_id, out_tx)) = notify {
                        let _ = out_tx.send(EngineMsg::ProfileActionDone { request_id, ok: true });
                    }
                    return;
                }
                let mut profile_map = HashMap::with_capacity(profile_list.len());
                for c in profile_list {
                    profile_map.insert(CookieKey::from_cookie(&c), c);
                }
                let mut remote_map = HashMap::with_capacity(remote_list.len());
                for c in remote_list {
                    remote_map.insert(CookieKey::from_cookie(&c), c);
                }
                let mut base = base_arc.lock().unwrap();
                let mut keys: HashSet<CookieKey> = HashSet::with_capacity(
                    base.len() + profile_map.len() + remote_map.len(),
                );
                keys.extend(base.keys().cloned());
                keys.extend(profile_map.keys().cloned());
                keys.extend(remote_map.keys().cloned());

                let mut ops: Vec<(LinkSide, CookieKey, Option<WireCookie>, bool)> = Vec::new();
                for key in keys {
                    if is_loopback_host(&key.host) {
                        continue;
                    }
                    let step = reconcile_cookie(
                        base.get(&key),
                        profile_map.get(&key),
                        remote_map.get(&key),
                        changed,
                    );
                    if let Some(b) = step.base {
                        base.insert(key.clone(), b);
                    } else {
                        base.remove(&key);
                    }
                    if let Some((to, val)) = step.write {
                        let target_secure = match to {
                            LinkSide::Profile => profile_map.get(&key),
                            LinkSide::Other => remote_map.get(&key),
                        }
                        .is_some_and(|c| c.secure);
                        ops.push((to, key, val, target_secure));
                    }
                }
                drop(base);

                if ops.is_empty() {
                    in_flight.store(false, Ordering::Release);
                    if let Some((request_id, out_tx)) = notify {
                        let _ = out_tx.send(EngineMsg::ProfileActionDone { request_id, ok: true });
                    }
                    return;
                }

                let now = now_unix_secs();
                let rem = Arc::new(AtomicUsize::new(ops.len()));
                // What this batch wrote into the profile, removed again if the person signs out
                // everywhere while it is in flight.
                let profile_writes: Arc<Mutex<Vec<(String, String)>>> = Arc::new(Mutex::new(Vec::new()));
                let notify_arc = Arc::new(Mutex::new(notify));
                for (to, key, maybe_cookie, target_secure) in ops {
                    let target = match to {
                        LinkSide::Profile => &profile_mgr_w,
                        LinkSide::Other => &remote_mgr_w,
                    };
                    let rem_c = rem.clone();
                    let in_flight_c = in_flight.clone();
                    let notify_c = notify_arc.clone();
                    let pump_c = pump.clone();
                    let p_flush = profile_mgr_w.clone();
                    let r_flush = remote_mgr_w.clone();
                    let cancel_c = cancel.clone();
                    let written_c = profile_writes.clone();
                    let batch_c = batch.clone();
                    let initial_c = initial_guard.clone();
                    let finish_one = move || {
                        if rem_c.fetch_sub(1, Ordering::AcqRel) == 1 {
                            // Every write of this batch has landed: one that raced the end of the
                            // mirror is undone now, after it, never before.
                            let ended = cancel_c.load(Ordering::Acquire);
                            if ended != LIVE {
                                let _ = r_flush.delete_cookies(None, None, None);
                            }
                            if ended == SIGNED_OUT {
                                for (url, name) in written_c.lock().unwrap().drain(..) {
                                    let url_cef = CefString::from(url.as_str());
                                    let name_cef = CefString::from(name.as_str());
                                    let _ = p_flush.delete_cookies(Some(&url_cef), Some(&name_cef), None);
                                }
                            }
                            let _ = p_flush.flush_store(None);
                            // The partition stops draining only once this last flush (after any wipe
                            // above, which the cookie store runs first) is done.
                            let batch_done = batch_c.clone();
                            let initial_done = initial_c.clone();
                            let mut cb = completion_cb(move || {
                                drop(batch_done);
                                drop(initial_done);
                                in_flight_c.store(false, Ordering::Release);
                                let _ = pump_c.send(crate::Event::Pump(0));
                                if let Some((request_id, out_tx)) = notify_c.lock().unwrap().take()
                                {
                                    let _ = out_tx.send(EngineMsg::ProfileActionDone {
                                        request_id,
                                        ok: true,
                                    });
                                }
                            });
                            let _ = r_flush.flush_store(Some(&mut cb));
                        }
                    };

                    if cancel.load(Ordering::Acquire) != LIVE {
                        finish_one();
                        continue;
                    }
                    if let Some(cookie) = maybe_cookie {
                        if let Some((url, cef_cookie)) = to_cef_cookie(&cookie, now) {
                            if matches!(to, LinkSide::Profile) {
                                profile_writes.lock().unwrap().push((url.clone(), cookie.name.clone()));
                            }
                            let mut cb = set_cookie_cb(move |_| finish_one());
                            let url_cef = CefString::from(url.as_str());
                            let _ = target.set_cookie(Some(&url_cef), Some(&cef_cookie), Some(&mut cb));
                        } else {
                            finish_one();
                        }
                    } else {
                        let dummy = WireCookie {
                            host: key.host.clone(),
                            name: key.name.clone(),
                            value: String::new(),
                            path: key.path.clone(),
                            secure: target_secure,
                            http_only: false,
                            expires: None,
                            same_site: WireSameSite::Unspecified,
                        };
                        let url_del = cookie_url(&dummy);
                        let mut cb = delete_cookies_cb(move |_| finish_one());
                        let url_cef = CefString::from(url_del.as_str());
                        let name_cef = CefString::from(key.name.as_str());
                        let _ = target.delete_cookies(Some(&url_cef), Some(&name_cef), Some(&mut cb));
                    }
                }
            });
        });
    }
}

/// Runs `done` once, when the last of its parts ends, with whether every part succeeded. A part
/// dropped without running counts as failed (`completion_cb`'s drop runs it).
pub(crate) struct Join {
    left: AtomicUsize,
    ok: AtomicBool,
    done: Mutex<Option<Box<dyn FnOnce(bool) + Send>>>,
}

impl Join {
    pub(crate) fn new(parts: usize, done: Box<dyn FnOnce(bool) + Send>) -> Arc<Join> {
        Arc::new(Join { left: AtomicUsize::new(parts), ok: AtomicBool::new(true), done: Mutex::new(Some(done)) })
    }
}

pub(crate) trait JoinPart {
    fn part(&self) -> Box<dyn FnOnce(bool) + Send>;
}

impl JoinPart for Arc<Join> {
    fn part(&self) -> Box<dyn FnOnce(bool) + Send> {
        let join = self.clone();
        Box::new(move |ok| {
            if !ok {
                join.ok.store(false, Ordering::Release);
            }
            if join.left.fetch_sub(1, Ordering::AcqRel) == 1
                && let Some(done) = join.done.lock().unwrap().take()
            {
                done(join.ok.load(Ordering::Acquire));
            }
        })
    }
}
