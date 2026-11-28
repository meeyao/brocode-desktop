//! `--launch-trace <file>`: one JSON line per run with the launch's and quit's marks, for
//! `harness\launch-bench.ts` and `perf\launch.bench.ts`. Every mark is nanoseconds since the
//! process was created (the OS's creation time, so the time before `main` counts too), from the
//! monotonic clock. Rune's own steps (`rune::marks`: window, GPU, renderer, fonts, first frame,
//! input ready, quit hooks, windows dropped) and the app's (`main.entered`, `state.restored`,
//! `plugins.started`, `server.answered`, `content.live`, `quit.saved`, `storage.flushed`, …)
//! share one list.
//!
//! `pixels.stable` is the last frame after `content.live` that changed what the window shows, once
//! nothing has changed for [`STILL`]: a fade still running keeps it back. "What it shows" is every
//! element's place, opacity, scale, text and loop phase, hashed per frame (a frame drawn again
//! with the same picture doesn't count), and the GPU taking the window over from the CPU's first
//! frame counts as a change (it re-renders). A launch whose window keeps moving (a spinner waiting
//! on a server) has none. `--launch-trace-why` prints, for each such frame, the elements that
//! changed (stderr), so a change nobody sees can be named.
//!
//! `--launch-trace-shifts` counts the launch's layout shifts from the first frame on (`shifts`).
//!
//! `--launch-trace-hold <ms>` keeps the sleeping built-in plugins from starting for `ms` after the
//! first turn that would start one ([`held`]): a picture of the window taken in that time shows what
//! the first frame drew from their records, and one taken after it what they draw themselves.
//!
//! `--launch-trace-quit <ms>` quits the app as its Quit command does, `ms` after the window
//! shows live content (or after the first frame, when no server answers within
//! [`CONTENT_WAIT`]), so a bench times the quit of a launch that settled.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use rune::time::Instant;

use rune::Tx;
use rune::marks::mark;

use crate::{App, Route};

/// How long a traced quit waits for live content before it quits anyway.
const CONTENT_WAIT: Duration = Duration::from_secs(10);

struct Boot {
    /// `main` entered, on the monotonic clock.
    at: Instant,
    /// Nanoseconds from the process's creation to `at`.
    since_created: u64,
    /// The process's creation time (FILETIME, 100 ns since 1601), so a bench can line its own
    /// clock up with the marks.
    created: u64,
    /// Page faults (soft and hard) before `main`'s first line: the loader and static initialisers.
    #[cfg(not(target_arch = "wasm32"))]
    faults: u32,
    /// The time-stamp counter at `at`, to learn its rate for [`process_cpu_ns`].
    #[cfg(target_os = "windows")]
    tsc: u64,
}

static BOOT: OnceLock<Boot> = OnceLock::new();
/// After the first drawn frame: the process's IO so far, and (`--launch-trace-threads`) its threads.
#[cfg(not(target_arch = "wasm32"))]
static FIRST_IO: OnceLock<(Io, Vec<(u32, String)>)> = OnceLock::new();

/// The process's IO counters (`GetProcessIoCounters`): every read, write and other operation,
/// files and devices alike.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Default)]
struct Io {
    read_ops: u64,
    read_bytes: u64,
    write_ops: u64,
    write_bytes: u64,
    other_ops: u64,
    faults: u32,
    /// CPU time of every thread so far (user and kernel), in ns.
    cpu_ns: u64,
}

#[cfg(not(target_arch = "wasm32"))]
impl Io {
    fn json(self) -> serde_json::Value {
        serde_json::json!({ "read_ops": self.read_ops, "read_bytes": self.read_bytes, "write_ops": self.write_ops, "write_bytes": self.write_bytes, "other_ops": self.other_ops, "cpu_ns": self.cpu_ns })
    }
}
/// When the first frame with live content drew: the process's IO and CPU so far.
#[cfg(not(target_arch = "wasm32"))]
static LIVE_IO: OnceLock<Io> = OnceLock::new();
static QUIT_SENT: AtomicBool = AtomicBool::new(false);
/// The last frame drawn after `content.live`, until the window has been still for [`STILL`].
static LAST_DRAWN: std::sync::Mutex<Option<Instant>> = std::sync::Mutex::new(None);
/// What each window's last frame showed, hashed (`drawn`), by window id.
static SHOWN: std::sync::Mutex<Option<std::collections::HashMap<String, u64>>> = std::sync::Mutex::new(None);
/// When it was: `pixels.stable`.
static STABLE: OnceLock<Instant> = OnceLock::new();
/// How long the window draws nothing before its last frame counts as the stable one.
const STILL: Duration = Duration::from_millis(500);
/// How long after content went live `pixels.stable` waits for the GPU to take over the window.
const HANDOVER_WAIT: Duration = Duration::from_secs(5);
/// How often a traced quit looks for content live before it's there.
const LIVE_LOOK: Duration = Duration::from_millis(250);
/// The wakes this module has asked for and not yet had: [`arm`].
static QUIT_WAKE: std::sync::Mutex<Option<Instant>> = std::sync::Mutex::new(None);
static SETTLE_WAKE: std::sync::Mutex<Option<Instant>> = std::sync::Mutex::new(None);

/// Asks for one wake at `at` unless one is already due no later. `poll` runs on every turn, and
/// every `wake_after` adds a deadline that stays until it's reached: asking on each turn made
/// each wake add another, so the timers alone woke the app 27–93 times a second.
fn arm(slot: &std::sync::Mutex<Option<Instant>>, at: Instant, tx: &mut Tx) {
    let now = Instant::now();
    let mut armed = slot.lock().unwrap_or_else(|e| e.into_inner());
    if armed.is_some_and(|t| t > now && t <= at) {
        return;
    }
    *armed = Some(at);
    tx.wake_after(at.saturating_duration_since(now));
}

/// First thing in `main`.
#[cfg(not(target_arch = "wasm32"))]
pub fn begin() {
    BOOT.get_or_init(|| {
        let at = Instant::now();
        let (created, now) = process_clock();
        Boot { at, since_created: now.saturating_sub(created).saturating_mul(100), created, faults: page_faults(), #[cfg(target_os = "windows")] tsc: tsc() }
    });
    mark("main.entered");
    #[cfg(target_os = "windows")]
    super::io_trace::start();
}

/// Whether this run writes a trace or prints its milestones.
pub fn on() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    // A traced quit needs `content.live` too: without it, it waits for the fallback.
    *ON.get_or_init(|| crate::arg("--launch-trace").is_some() || echoes() || quit_after().is_some())
}

/// `--launch-marks`: each launch milestone printed as a line once the app sees it
/// (`launch-mark <name> <ns since creation> at=<epoch ms, to the microsecond>`), for strobe's
/// `launch: { milestones }`. A line carries its mark's own time, so the marks from before the
/// first frame, which go out with it, are timed where they happened.
fn echoes() -> bool {
    static ECHO: OnceLock<bool> = OnceLock::new();
    *ECHO.get_or_init(|| crate::flag("--launch-marks"))
}

/// The milestones `--launch-marks` prints.
/// `content.rows` is the first frame with the open session's messages (the saved copy's, on a
/// later launch), `sessions.rows` the first with sessions to list, `session.read.received` the
/// server's answer with the open session's newest page. `server.answered` is the first reads
/// landing from a server that answered them; when no server could be reached or read, its place
/// in the list prints `server.failed` instead (`launch::first_reads`), and `content.live` never
/// follows while the window shows the server as away.
const MILESTONES: [&str; 11] = [
    "main.entered",
    "window.created",
    "frame.first.presented",
    "text.complete",
    "input.ready",
    "content.rows",
    "sessions.rows",
    "server.answered",
    "content.live",
    "session.read.received",
    "pixels.stable",
];

/// Prints the milestones reached since the last call.
fn echo() {
    // One bit per milestone printed.
    static PRINTED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let done = PRINTED.load(Ordering::Relaxed);
    if !echoes() || done == (1 << MILESTONES.len()) - 1 {
        return;
    }
    let marks = rune::marks::marks();
    // Nothing is printed or written before the first frame.
    if !marks.iter().any(|(n, _)| *n == "frame.first.presented") {
        return;
    }
    let mut printed = done;
    let mut lines = String::new();
    for (i, name) in MILESTONES.iter().enumerate() {
        if done & (1 << i) != 0 {
            continue;
        }
        let found = marks
            .iter()
            .find(|(n, _)| n == name || (*name == "server.answered" && *n == "server.failed"))
            .map(|(n, t)| (*n, *t))
            .or_else(|| if *name == "pixels.stable" { STABLE.get().map(|t| (*name, *t)) } else { None });
        let Some((name, t)) = found else { continue };
        let at = ns(t);
        // Epoch microseconds of the mark, printed as ms with three decimals for strobe's milestone
        // `at` group: FILETIME is 100 ns since 1601.
        let epoch_us = BOOT.get().map_or(0, |b| (b.created.saturating_sub(116_444_736_000_000_000) / 10).saturating_add(at / 1_000));
        lines.push_str(&format!("launch-mark {name} {at} at={}.{:03}\n", epoch_us / 1_000, epoch_us % 1_000));
        printed |= 1 << i;
    }
    PRINTED.store(printed, Ordering::Relaxed);
    if lines.is_empty() {
        return;
    }
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    out.write_all(lines.as_bytes()).ok();
    out.flush().ok();
    // A launch from the Dock, Finder or Explorer has no stdout: the same lines go to
    // `<logs>/launch-marks.log`. The first lines go out with the first frame, so this file is never
    // written before it.
    if let Some(dir) = crate::dirs::logs() {
        std::fs::create_dir_all(&dir).ok();
        if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("launch-marks.log")) {
            file.write_all(lines.as_bytes()).ok();
        }
    }
}

/// Nanoseconds from the process's creation to `t`.
fn ns(t: Instant) -> u64 {
    let Some(boot) = BOOT.get() else { return 0 };
    match t.checked_duration_since(boot.at) {
        Some(d) => boot.since_created.saturating_add(u64::try_from(d.as_nanos()).unwrap_or(u64::MAX)),
        None => boot.since_created.saturating_sub(u64::try_from((boot.at - t).as_nanos()).unwrap_or(u64::MAX)),
    }
}

/// After a frame drew: the first with the session's messages, and the first with live content
/// (the window's server answered and Home, a draft or the session with messages shows), then
/// each frame that changed what the window shows, until `pixels.stable`.
pub fn drawn(rt: &rune::Runtime<App>) {
    if !on() {
        return;
    }
    let s = &rt.state;
    super::shifts::drawn(rt, BOOT.get().map_or(0, |b| Instant::now().checked_duration_since(b.at).unwrap_or_default().as_millis()));
    #[cfg(not(target_arch = "wasm32"))]
    let _ = FIRST_IO.get_or_init(|| (io_counters(), crate::flag("--launch-trace-threads").then(threads).unwrap_or_default()));
    let rows = s.painted_session().map_or(0, |(_, rows)| rows);
    if rows > 0 {
        mark("content.rows");
    }
    // The first frame with words in the composer: a key a bench posts as the window shows has
    // landed in it (a restored draft has them from the first frame).
    if !s.composer.text.is_empty() {
        mark("composer.text");
    }
    // The first frame with sessions to list (Home's rows, the sidebar's): the saved list's on a
    // later launch, else the server's.
    if !s.sessions.is_empty() {
        mark("sessions.rows");
    }
    let shows = rows > 0 || !matches!(*s.route, Route::Session { .. });
    if shows && !crate::launch::waiting(s) && !crate::launch::offline(s) {
        mark("content.live");
        #[cfg(not(target_arch = "wasm32"))]
        let _ = LIVE_IO.get_or_init(io_counters);
    }
    if STABLE.get().is_none() && rune::marks::marks().iter().any(|(n, _)| *n == "content.live") {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        let why = crate::flag("--launch-trace-why");
        let mut each = std::collections::HashMap::new();
        for e in rt.elements() {
            let mut one = std::collections::hash_map::DefaultHasher::new();
            e.text.hash(&mut one);
            e.exiting.hash(&mut one);
            e.loop_phase.map(f64::to_bits).hash(&mut one);
            e.rect.iter().chain([e.opacity, e.scale, e.blur, e.radius, e.hover, e.press].iter()).for_each(|v| v.to_bits().hash(&mut one));
            // Colours too: a fill or text colour easing (activation, a theme) changes pixels in place.
            [e.fill].iter().chain(e.text_color.iter()).for_each(|k| [k.r, k.g, k.b, k.a].iter().for_each(|v| v.to_bits().hash(&mut one)));
            let one = one.finish();
            e.path.hash(&mut h);
            one.hash(&mut h);
            if why {
                each.insert(e.path.clone(), (one, e.rect, e.opacity, e.text.clone()));
            }
        }
        let changed = SHOWN.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Default::default).insert(s.window.id.to_string(), h.finish()) != Some(h.finish());
        if changed {
            *LAST_DRAWN.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
            // `--launch-trace-why`: which elements changed, to stderr, so a change no one sees can be named.
            if why {
                type Seen = std::collections::HashMap<String, (u64, [f32; 4], f32, Option<String>)>;
                static PREV: std::sync::Mutex<Option<std::collections::HashMap<String, Seen>>> = std::sync::Mutex::new(None);
                let mut prev = PREV.lock().unwrap_or_else(|e| e.into_inner());
                let prev = prev.get_or_insert_with(Default::default);
                if let Some(old) = prev.get(&s.window.id.to_string()) {
                    let mut lines: Vec<String> = each.iter().filter(|(p, v)| old.get(*p).is_none_or(|o| o.0 != v.0)).map(|(p, v)| format!("  {} {p} rect {:?} opacity {} text {:?}", if old.contains_key(p) { "changed" } else { "added" }, v.1, v.2, v.3.as_deref().map(|t| t.chars().take(40).collect::<String>()))).collect();
                    lines.extend(old.keys().filter(|p| !each.contains_key(*p)).map(|p| format!("  removed {p}")));
                    lines.sort();
                    let n = lines.len();
                    eprintln!("launch-why {} ms after main, window {}: {n} elements differ", BOOT.get().map_or(0, |b| Instant::now().checked_duration_since(b.at).unwrap_or_default().as_millis()), s.window.id);
                    for l in lines.into_iter().take(12) { eprintln!("{l}"); }
                }
                prev.insert(s.window.id.to_string(), each);
            }
        }
    }
    echo();
}

/// `pixels.stable`, once the window has drawn nothing for [`STILL`] after content went live;
/// asks to be polled again until then.
fn settle(tx: &mut Tx) {
    if STABLE.get().is_some() {
        return;
    }
    let Some(drawn) = *LAST_DRAWN.lock().unwrap_or_else(|e| e.into_inner()) else { return };
    // The GPU's first frame replaces the CPU's: a re-render that can change pixels the element
    // tree doesn't see. Until it has come (at most HANDOVER_WAIT after content went live), the
    // window isn't stable; once it has, it counts as a change.
    let marks = rune::marks::marks();
    let at = |name: &str| marks.iter().find(|(n, _)| *n == name).map(|(_, t)| *t);
    let handed = at("gpu.window.handed");
    let waited = at("content.live").is_some_and(|live| Instant::now().checked_duration_since(live).unwrap_or_default() >= HANDOVER_WAIT);
    if handed.is_none() && !waited {
        arm(&SETTLE_WAKE, Instant::now() + STILL, tx);
        return;
    }
    let last = handed.map_or(drawn, |h| h.max(drawn));
    let quiet = Instant::now().checked_duration_since(last).unwrap_or_default();
    if quiet >= STILL {
        STABLE.set(last).ok();
        return;
    }
    arm(&SETTLE_WAKE, last + STILL, tx);
}

/// `poll`: a traced quit, once, when it's due.
pub fn poll(tx: &mut Tx) {
    if on() {
        settle(tx);
    }
    echo();
    let Some(after) = quit_after() else { return };
    if QUIT_SENT.load(Ordering::Relaxed) {
        return;
    }
    let marks = rune::marks::marks();
    let at = |name: &str| marks.iter().find(|(n, _)| *n == name).map(|(_, t)| *t);
    let Some(first) = at("frame.first.presented") else {
        arm(&QUIT_WAKE, Instant::now() + Duration::from_millis(50), tx);
        return;
    };
    let live = at("content.live");
    let due = live.unwrap_or(first + CONTENT_WAIT) + after;
    let now = Instant::now();
    if now < due {
        // Live: one timer, for when the quit is due, so a traced run idles as an untraced one
        // does. Not live yet: look again a few times a second, since content may land before the
        // fallback and its frame may be the last turn for a while.
        arm(&QUIT_WAKE, if live.is_some() { due } else { due.min(now + LIVE_LOOK) }, tx);
        return;
    }
    if !QUIT_SENT.swap(true, Ordering::Relaxed) {
        mark("quit.requested");
        // `--launch-trace-relaunch`: the quit is a Restart (an update's relaunch), once: the copy it
        // starts (`RUNE_RELAUNCH_AFTER`) quits as a traced run does, appending its own line.
        if crate::flag("--launch-trace-relaunch") && std::env::var_os("RUNE_RELAUNCH_AFTER").is_none() {
            crate::background_notice::mark_relaunch();
            tx.relaunch();
        } else {
            tx.quit();
        }
    }
}

/// `--launch-trace-hold <ms>`: how much longer the sleeping plugins wait, counted from the first
/// call; `None` once it's over, or without the flag.
pub fn held() -> Option<Duration> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        static HOLD: OnceLock<Option<(std::time::Instant, Duration)>> = OnceLock::new();
        let (since, hold) = (*HOLD.get_or_init(|| crate::arg("--launch-trace-hold").and_then(|ms| ms.parse().ok()).map(|ms| (std::time::Instant::now(), Duration::from_millis(ms)))))?;
        hold.checked_sub(since.elapsed()).filter(|left| !left.is_zero())
    }
    #[cfg(target_arch = "wasm32")]
    {
        None
    }
}

fn quit_after() -> Option<Duration> {
    static AFTER: OnceLock<Option<Duration>> = OnceLock::new();
    *AFTER.get_or_init(|| crate::arg("--launch-trace-quit").and_then(|ms| ms.parse().ok()).map(Duration::from_millis))
}

/// The text renderer the run draws with, named for the trace (`text_rendering::poll`).
pub fn renderer(name: &'static str) {
    #[cfg(not(target_arch = "wasm32"))]
    RENDERER.set(name).ok();
    #[cfg(target_arch = "wasm32")]
    let _ = name;
}

#[cfg(not(target_arch = "wasm32"))]
static RENDERER: OnceLock<&'static str> = OnceLock::new();

/// Last thing in `main`: appends the run's line to the `--launch-trace` file.
#[cfg(not(target_arch = "wasm32"))]
pub fn finish(windows: usize) {
    let Some(path) = crate::arg("--launch-trace") else { return };
    mark("main.return");
    let Some(boot) = BOOT.get() else { return };
    let marks: serde_json::Map<String, serde_json::Value> = rune::marks::marks().into_iter().chain(STABLE.get().map(|t| ("pixels.stable", *t))).map(|(n, t)| (n.to_owned(), ns(t).into())).collect();
    let metrics: serde_json::Map<String, serde_json::Value> = rune::marks::metrics().into_iter().map(|(n, v)| (n.to_owned(), v.into())).collect();
    let notes: serde_json::Map<String, serde_json::Value> = rune::marks::notes().into_iter().map(|(n, v)| (n.to_owned(), v.into())).collect();
    let line = serde_json::json!({
        "v": 1,
        "pid": std::process::id(),
        "created_filetime": boot.created,
        "renderer": RENDERER.get().copied().unwrap_or("default"),
        "windows": windows,
        "ui_tid": ui_thread(),
        "page_faults": { "main": boot.faults, "first_frame": FIRST_IO.get().map(|(io, _)| io.faults), "exit": page_faults() },
        "io_first_frame": FIRST_IO.get().map(|(io, _)| io.json()),
        "io_live": LIVE_IO.get().map(|io| io.json()),
        "io_exit": io_counters().json(),
        "io": io_calls(&ns),
        "threads": FIRST_IO.get().map(|(_, t)| t.clone()).unwrap_or_default().into_iter().chain(if crate::flag("--launch-trace-threads") { threads() } else { Vec::new() }).map(|(id, name)| serde_json::json!([id, name])).collect::<Vec<_>>(),
        "marks_ns": marks,
        "metrics": metrics,
        "notes": notes,
        "shifts": super::shifts::json(&ns),
        "boot_record": crate::storage::boot_bytes().map(|(all, plugins)| serde_json::json!({ "bytes": all, "plugin_bytes": plugins })),
    });
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        writeln!(f, "{line}").ok();
    }
}

/// The process's creation time and now, as FILETIMEs.
#[cfg(target_os = "windows")]
#[allow(unsafe_code)]
fn process_clock() -> (u64, u64) {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::SystemInformation::GetSystemTimePreciseAsFileTime;
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
    let zero = || FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
    let (mut created, mut exited, mut kernel, mut user, mut now) = (zero(), zero(), zero(), zero(), zero());
    // SAFETY: the pseudo-handle of this process and five FILETIMEs this frame owns.
    unsafe {
        GetProcessTimes(GetCurrentProcess(), &mut created, &mut exited, &mut kernel, &mut user);
        GetSystemTimePreciseAsFileTime(&mut now);
    }
    let value = |t: FILETIME| (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime);
    (value(created), value(now))
}

/// `--launch-trace-io`'s calls, each with its time (ns since creation), thread, how long it took and
/// the app frames that made it.
#[cfg(target_os = "windows")]
fn io_calls(ns: &dyn Fn(Instant) -> u64) -> serde_json::Value {
    let calls = super::io_trace::take();
    let reads = (super::io_trace::READ_CALLS.load(Ordering::Relaxed), super::io_trace::READ_BYTES.load(Ordering::Relaxed));
    let (dropped, cost_ns) = super::io_trace::stats();
    serde_json::json!({
        "read_calls": reads.0,
        "read_bytes": reads.1,
        "dropped": dropped,
        "cost_ns_per_call": cost_ns,
        "calls": calls.into_iter().map(|(c, sites)| serde_json::json!({ "ns": ns(c.at), "took_ns": c.took_ns, "tid": c.tid, "op": c.op, "path": c.path, "ok": c.ok, "sites": sites })).collect::<Vec<_>>(),
    })
}

#[cfg(all(not(target_os = "windows"), not(target_arch = "wasm32")))]
fn io_calls(_: &dyn Fn(Instant) -> u64) -> serde_json::Value {
    serde_json::Value::Null
}

/// The UI thread's OS id, which ETW traces name threads by.
#[cfg(target_os = "windows")]
#[allow(unsafe_code)]
fn ui_thread() -> u32 {
    // SAFETY: no arguments; it reads the calling thread's id.
    unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() }
}

#[cfg(all(not(target_os = "windows"), not(target_arch = "wasm32")))]
fn ui_thread() -> u32 {
    0
}

#[cfg(target_os = "windows")]
#[allow(unsafe_code)]
fn io_counters() -> Io {
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessIoCounters, IO_COUNTERS};
    // SAFETY: an IO_COUNTERS this frame owns, filled for this process's pseudo-handle.
    let c = unsafe {
        let mut c: IO_COUNTERS = std::mem::zeroed();
        GetProcessIoCounters(GetCurrentProcess(), &mut c);
        c
    };
    Io { read_ops: c.ReadOperationCount, read_bytes: c.ReadTransferCount, write_ops: c.WriteOperationCount, write_bytes: c.WriteTransferCount, other_ops: c.OtherOperationCount, faults: page_faults(), cpu_ns: process_cpu_ns() }
}

#[cfg(all(not(target_os = "windows"), not(target_arch = "wasm32")))]
fn io_counters() -> Io {
    Io { cpu_ns: process_cpu_ns(), ..Io::default() }
}

/// This process's page faults so far, soft and hard (`GetProcessMemoryInfo`).
/// CPU time of every thread of this process so far, user and kernel, in ns. `GetProcessTimes`
/// counts in clock ticks (15,625 µs), too coarse for a launch: the process's cycle count
/// (`QueryProcessCycleTime`, time-stamp counter cycles) is exact, converted at the counter's rate
/// since [`begin`]. Ticks when the rate isn't known yet.
#[cfg(target_os = "windows")]
#[allow(unsafe_code)]
fn process_cpu_ns() -> u64 {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::WindowsProgramming::QueryProcessCycleTime;
    if let Some(boot) = BOOT.get() && boot.tsc != 0 {
        let elapsed = boot.at.elapsed().as_nanos() as f64;
        let per_ns = tsc().saturating_sub(boot.tsc) as f64 / elapsed.max(1.0);
        let mut cycles = 0u64;
        // SAFETY: a u64 this frame owns, for this process's pseudo-handle.
        if elapsed >= 1e6 && per_ns > 0.0 && unsafe { QueryProcessCycleTime(windows_sys::Win32::System::Threading::GetCurrentProcess(), &mut cycles) } != 0 {
            return (cycles as f64 / per_ns) as u64;
        }
    }
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
    let ft = |t: FILETIME| (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime);
    // SAFETY: four FILETIMEs this frame owns.
    unsafe {
        let (mut created, mut exited, mut kernel, mut user): (FILETIME, FILETIME, FILETIME, FILETIME) = std::mem::zeroed();
        if GetProcessTimes(GetCurrentProcess(), &mut created, &mut exited, &mut kernel, &mut user) == 0 {
            return 0;
        }
        (ft(kernel) + ft(user)).saturating_mul(100)
    }
}

/// CPU time of every thread of this process so far, user and system, in ns (`getrusage`).
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[allow(unsafe_code)]
fn process_cpu_ns() -> u64 {
    // SAFETY: an rusage this frame owns.
    let r = unsafe {
        let mut r: libc::rusage = std::mem::zeroed();
        if libc::getrusage(libc::RUSAGE_SELF, &mut r) != 0 {
            return 0;
        }
        r
    };
    let ns = |t: libc::timeval| (t.tv_sec as u64).saturating_mul(1_000_000_000).saturating_add(t.tv_usec as u64 * 1_000);
    ns(r.ru_utime).saturating_add(ns(r.ru_stime))
}

#[cfg(all(not(any(target_os = "windows", target_os = "macos", target_os = "linux")), not(target_arch = "wasm32")))]
fn process_cpu_ns() -> u64 {
    0
}

/// The time-stamp counter (x86-64 Windows, where the process's cycle count is in its cycles); 0 elsewhere.
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
#[allow(unsafe_code)]
fn tsc() -> u64 {
    // SAFETY: RDTSC reads a counter every x86-64 CPU has.
    unsafe { core::arch::x86_64::_rdtsc() }
}

#[cfg(all(target_os = "windows", not(target_arch = "x86_64")))]
fn tsc() -> u64 {
    0
}

#[cfg(target_os = "windows")]
#[allow(unsafe_code)]
fn page_faults() -> u32 {
    use windows_sys::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    // SAFETY: a PROCESS_MEMORY_COUNTERS this frame owns, sized for the call.
    unsafe {
        let mut c: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
        c.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb);
        c.PageFaultCount
    }
}

#[cfg(all(not(target_os = "windows"), not(target_arch = "wasm32")))]
fn page_faults() -> u32 {
    0
}

/// This process's live threads and their names (`SetThreadDescription`, which Rust's named threads
/// set), for naming the threads in an ETW trace.
#[cfg(target_os = "windows")]
#[allow(unsafe_code)]
fn threads() -> Vec<(u32, String)> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next};
    use windows_sys::Win32::System::Threading::{GetThreadDescription, OpenThread, THREAD_QUERY_LIMITED_INFORMATION};
    let pid = std::process::id();
    let mut out = Vec::new();
    // SAFETY: a thread snapshot walked with a THREADENTRY32 this frame owns; each thread handle and
    // description is released after it's read.
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
        if snap.is_null() || snap as isize == -1 {
            return out;
        }
        let mut e: THREADENTRY32 = std::mem::zeroed();
        e.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;
        let mut more = Thread32First(snap, &mut e) != 0;
        while more {
            if e.th32OwnerProcessID == pid {
                let h = OpenThread(THREAD_QUERY_LIMITED_INFORMATION, 0, e.th32ThreadID);
                let mut name = String::new();
                if !h.is_null() {
                    let mut text: windows_sys::core::PWSTR = std::ptr::null_mut();
                    if GetThreadDescription(h, &mut text) >= 0 && !text.is_null() {
                        let len = (0..).take_while(|&i| *text.add(i) != 0).count();
                        name = String::from_utf16_lossy(std::slice::from_raw_parts(text, len));
                        windows_sys::Win32::Foundation::LocalFree(text.cast());
                    }
                    CloseHandle(h);
                }
                out.push((e.th32ThreadID, name));
            }
            more = Thread32Next(snap, &mut e) != 0;
        }
        CloseHandle(snap);
    }
    out
}

#[cfg(all(not(target_os = "windows"), not(target_arch = "wasm32")))]
fn threads() -> Vec<(u32, String)> {
    Vec::new()
}

/// 100 ns intervals from 1601 (FILETIME's epoch) to 1970.
#[cfg(any(target_os = "macos", target_os = "linux"))]
const UNIX_IN_FILETIME: u64 = 116_444_736_000_000_000;

/// The wall clock now, as a FILETIME.
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[allow(unsafe_code)]
fn realtime_filetime() -> u64 {
    let mut t = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: a timespec this frame owns.
    unsafe { libc::clock_gettime(libc::CLOCK_REALTIME, &mut t) };
    UNIX_IN_FILETIME + t.tv_sec as u64 * 10_000_000 + t.tv_nsec as u64 / 100
}

/// The process's creation time (the kernel's, µs) and now, as FILETIMEs.
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn process_clock() -> (u64, u64) {
    // SAFETY: a zeroed plain-C struct, filled by the kernel for this process's own pid.
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
    // SAFETY: the struct above, its size, and this process's pid.
    let got = unsafe { libc::proc_pidinfo(std::process::id() as libc::c_int, libc::PROC_PIDTBSDINFO, 0, (&raw mut info).cast(), size) };
    let now = realtime_filetime();
    if got != size {
        return (0, 0);
    }
    (UNIX_IN_FILETIME + info.pbi_start_tvsec * 10_000_000 + info.pbi_start_tvusec * 10, now)
}

/// The process's creation time and now, as FILETIMEs: `/proc/self/stat`'s start time (clock
/// ticks after boot, 10 ms each on most kernels) against the boot clock.
#[cfg(target_os = "linux")]
#[allow(unsafe_code)]
fn process_clock() -> (u64, u64) {
    // A file read: only for a run that writes a trace.
    if !on() {
        return (0, 0);
    }
    let now = realtime_filetime();
    let Some(ticks) = std::fs::read_to_string("/proc/self/stat").ok().and_then(|s| s.rsplit_once(')').and_then(|(_, rest)| rest.split_whitespace().nth(19)?.parse::<u64>().ok())) else {
        return (0, 0);
    };
    // SAFETY: no arguments.
    let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) }.max(1) as u64;
    let mut boot = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: a timespec this frame owns.
    unsafe { libc::clock_gettime(libc::CLOCK_BOOTTIME, &mut boot) };
    let up = boot.tv_sec as u64 * 10_000_000 + boot.tv_nsec as u64 / 100;
    let age = up.saturating_sub(ticks * 10_000_000 / hz);
    (now.saturating_sub(age), now)
}

/// Elsewhere the marks count from `main`.
#[cfg(all(not(any(target_os = "windows", target_os = "macos", target_os = "linux")), not(target_arch = "wasm32")))]
fn process_clock() -> (u64, u64) {
    (0, 0)
}
