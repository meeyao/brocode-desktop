//! Diagnostics: the app's memory, plugins' costs, the developer settings plugins read and set
//! (experiments, the animation clock, the tuning panel, memory diagnostics), and heap snapshot
//! captures. Memory diagnostics write one JSONL sample per interval from a thread of their own,
//! so they never wake the window.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;
use rune::time::Instant;

use mlua::{FromLua, IntoLua, Lua, MultiValue, Table, Value};
use rune_luau::{Completer, Data, Host};
use serde_json::{Value as Json, json};

use super::{async_method, json_to_lua, map, method, with_app};
use crate::App;
use crate::plugin_host::cx::{self, Dep, Request};
use crate::plugin_host::plugin::Half;
use crate::plugin_host::vm::Vmx;

/// How long the server may take to write its heap snapshot before the manifest records it as
/// failed (`main.ts`).
const SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const DEFAULT_INTERVAL: Duration = Duration::from_secs(30);

/// Local and beta builds: memory diagnostics and the motion tools. Matches `app::channel()` so
/// PR builds (`pr-*`, mapped to `Channel::Local`) do not disagree with `App.Channel`.
pub fn debug_build() -> bool {
    matches!(super::app::channel(), opencode_plugin_spec::Channel::Local | opencode_plugin_spec::Channel::Beta)
}

pub fn build(lua: &Lua, vmx: &Rc<Vmx>, t: &Table) -> mlua::Result<()> {
    let own = vmx.half.id.clone();
    method(lua, t, "Metrics", move |lua, plugin: Option<String>| {
        with_app("Diagnostics:Metrics", |app| {
            let id = plugin.unwrap_or_else(|| own.to_string());
            let Some(m) = app.plugins.metrics(&id).filter(|_| debug_build()) else { return Ok(Value::Nil) };
            let out = lua.create_table()?;
            out.raw_set("memoryKb", m.memory_kb)?;
            out.raw_set("renderUs", m.render_us)?;
            out.raw_set("memoHitRate", m.memo_hit_rate)?;
            Ok(Value::Table(out))
        })
    })?;
    method(lua, t, "Memory", |lua, ()| {
        if !debug_build() {
            return Ok(Value::Nil);
        }
        json_to_lua(lua, &serde_json::from_str(&crate::plugin_host::memory::report()).unwrap_or(Json::Null))
    })?;
    method(lua, t, "Plugins", |lua, ()| {
        with_app("Diagnostics:Plugins", |app| {
            let list = lua.create_table()?;
            for (_, p) in app.plugins.running() {
                list.raw_push(p.manifest.id.as_str())?;
            }
            Ok(list)
        })
    })?;
    method(lua, t, "Get", |lua, name: String| {
        cx::read(Dep::Dev(name.clone()));
        with_app("Diagnostics:Get", |app| get(app, &name).into_lua(lua))
    })?;
    let p = vmx.half.id.clone();
    method(lua, t, "Set", move |lua, (name, value): (String, Value)| {
        let value = Data::from_lua(value, lua)?;
        if name == "memoryLog" {
            let now_ms = with_app("Diagnostics:Set", |app| Ok(*app.now_ms))?;
            memory_log(matches!(value, Data::Bool(true)), now_ms);
            crate::plugin_host::bump();
            return Ok(());
        }
        if debug_build() {
            cx::ask(&p, Request::Dev { name, value });
        }
        Ok(())
    })?;
    async_method(lua, vmx, t, "HeapSnapshotAsync", "Diagnostics.HeapSnapshotAsync")
}

pub fn asyncs(host: &Host<'_>, t: &Table, _half: &Rc<Half>) -> mlua::Result<()> {
    t.raw_set("Diagnostics.HeapSnapshotAsync", host.async_function("Diagnostics:HeapSnapshotAsync", heap_snapshot)?)
}

/// What `Diagnostics:Get(name)` answers now.
pub fn get(app: &App, name: &str) -> Data {
    use session_view::knobs::{SOURCES_ENABLED, SOURCES_ICONS};
    let k = &app.diag.knobs;
    let on = |v: f32| v > 0.5;
    let word = |s: &str| Data::String(s.to_owned());
    match name {
        "sources.enabled" => word(if on(k.get(&SOURCES_ENABLED)) { "on" } else { "off" }),
        "sources.icons" => word(if on(k.get(&SOURCES_ICONS)) { "site" } else { "letters" }),
        _ if !debug_build() => Data::Nil,
        "speed" => Data::Number(f64::from(k.get(&crate::knobs::SPEED))),
        "slow" => Data::Bool(on(k.get(&crate::knobs::SLOW))),
        "knobs" => Data::Bool(k.open),
        "memoryLog" => word(LOG.with_borrow(|l| l.word())),
        "direction" => word(match opencode_i18n::direction() {
            opencode_i18n::Direction::Ltr => "ltr",
            opencode_i18n::Direction::Rtl => "rtl",
        }),
        "forceFocus" => Data::Bool(app.plugins.force_focus),
        _ => Data::Nil,
    }
}

/// [`Request::Dev`]: a developer tool asked for.
pub fn apply(app: &mut App, name: &str, value: Data, tx: &mut rune::Tx) {
    tx.instant();
    match (name, value) {
        ("speed", Data::Number(x)) => app.diag.knobs.set(&crate::knobs::SPEED, x as f32),
        ("speed", Data::Integer(x)) => app.diag.knobs.set(&crate::knobs::SPEED, x as f32),
        ("slow", Data::Bool(on)) => app.diag.knobs.set(&crate::knobs::SLOW, if on { 1.0 } else { 0.0 }),
        ("knobs", Data::Bool(open)) if open != app.diag.knobs.open => app.diag.knobs.toggle(),
        ("components", _) => tx.open_playground(),
        ("direction", Data::String(d)) => opencode_i18n::set_direction(if d == "rtl" { opencode_i18n::Direction::Rtl } else { opencode_i18n::Direction::Ltr }),
        ("forceFocus", Data::Bool(on)) => app.plugins.force_focus = on,
        _ => {}
    }
}

// ---- Memory diagnostics -----------------------------------------------------------------------

/// Memory diagnostics (`memory-machine.ts`): off, or writing a sample every interval to one
/// file, until stopped. The environment's interval keeps them on for the launch.
enum MemoryLog {
    Off,
    On { file: PathBuf, env: bool, stop: mpsc::Sender<()> },
}

impl MemoryLog {
    fn word(&self) -> &'static str {
        match self {
            MemoryLog::Off => "off",
            MemoryLog::On { env: true, .. } => "env",
            MemoryLog::On { env: false, .. } => "on",
        }
    }
}

thread_local! {
    static LOG: RefCell<MemoryLog> = const { RefCell::new(MemoryLog::Off) };
}

/// `OPENCODE_DESKTOP_MEMORY_LOG=<seconds>`, read once: invalid values disable diagnostics rather
/// than turning a bad interval into a hot timer.
fn env_interval() -> Option<Duration> {
    static ENV: std::sync::OnceLock<Option<Duration>> = std::sync::OnceLock::new();
    *ENV.get_or_init(|| {
        #[allow(clippy::disallowed_methods)]
        let v = std::env::var("OPENCODE_DESKTOP_MEMORY_LOG").ok()?;
        let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
        let well_formed = v.split_once('.').map_or(digits(&v), |(a, b)| digits(a) && digits(b));
        let seconds: f64 = v.parse().ok().filter(|_| well_formed)?;
        (1.0..=2_147_483.0).contains(&seconds).then(|| Duration::from_secs_f64(seconds))
    })
}

/// Starts memory diagnostics, or stops them unless the environment keeps them on.
pub fn memory_log(enabled: bool, now_ms: i64) {
    let env = env_interval();
    let want = enabled || env.is_some();
    LOG.with_borrow_mut(|log| match (std::mem::replace(log, MemoryLog::Off), want) {
        (MemoryLog::On { file, env, stop }, true) => *log = MemoryLog::On { file, env, stop },
        (MemoryLog::On { file, stop, .. }, false) => {
            stop.send(()).ok();
            crate::plugin_host::tasks::log(format!("memory diagnostics stopped: {}", file.display()));
        }
        (MemoryLog::Off, false) => {}
        (MemoryLog::Off, true) => {
            let Some(logs) = crate::dirs::data().map(|d| d.join("logs")) else { return };
            let file = logs.join(format!("memory-{}.jsonl", stamp(now_ms)));
            let interval = env.unwrap_or(DEFAULT_INTERVAL);
            crate::plugin_host::tasks::log(format!("memory diagnostics started: {} every {} ms", file.display(), interval.as_millis()));
            let (stop, stopped) = mpsc::channel();
            let path = file.clone();
            let base = (now_ms, Instant::now());
            std::thread::Builder::new()
                .name("memory-log".into())
                .spawn(move || {
                    while let Err(mpsc::RecvTimeoutError::Timeout) = stopped.recv_timeout(interval) {
                        let now = base.0 + base.1.elapsed().as_millis() as i64;
                        if let Err(e) = append(&path, now) {
                            crate::plugin_host::tasks::log(format!("memory diagnostics sample failed: {e}"));
                        }
                    }
                })
                .ok();
            *log = MemoryLog::On { file, env: env.is_some(), stop };
        }
    });
}

/// One sample: what the OS counts for the app's process and what its allocator holds (Desktop
/// records Electron's per-process metrics and each renderer's JavaScript heap).
fn append(file: &Path, now: i64) -> std::io::Result<()> {
    use std::io::Write as _;
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let memory: Json = serde_json::from_str(&crate::plugin_host::memory::report()).unwrap_or(Json::Null);
    let line = json!({ "timestamp": iso(now), "mainPid": std::process::id(), "nativeMemoryUnit": "bytes", "process": memory });
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(file)?;
    writeln!(f, "{line}")
}

// ---- Heap snapshots ---------------------------------------------------------------------------

/// `Diagnostics:HeapSnapshotAsync()` (`main.ts`, `server-heap.ts`): a new
/// `opencode-heap-<time>` folder in Downloads holding the app's own server's heap snapshot,
/// `memory.json` and `manifest.json`, written last so a script can wait for it. The app is
/// native, so its own process is measured rather than snapshotted.
fn heap_snapshot(_: &Lua, _: MultiValue, done: Completer) -> mlua::Result<()> {
    let started = cx::with_app(|app| *app.now_ms).ok_or_else(|| mlua::Error::runtime("Diagnostics:HeapSnapshotAsync needs a window"))?;
    let server = crate::servers::runtime::own_server();
    let failed = |e: String| map([("error", Data::String(e))]);
    let Some(directory) = crate::dirs::downloads().map(|d| d.join(format!("opencode-heap-{}", stamp(started)))) else {
        done.complete(failed("There is no downloads folder".into()));
        return Ok(());
    };
    // Snapshots hold prompts, file contents and credentials.
    if let Err(e) = private_dir(&directory) {
        done.complete(failed(e.to_string()));
        return Ok(());
    }
    crate::plugin_host::tasks::log(format!("capturing heap snapshots: {}", directory.display()));
    // Measurements first: writing a heap snapshot raises the memory of the process it serializes.
    let before: Json = serde_json::from_str(&crate::plugin_host::memory::report()).unwrap_or(Json::Null);
    let clock = Instant::now();
    crate::plugin_host::tasks::run(move || {
        let path = format!("/api/debug/heap-dump?directory={}", super::session::encode_uri(&directory.display().to_string()));
        let answer = match &server {
            Some(srv) => srv.call_within("POST", &path, None, SNAPSHOT_TIMEOUT),
            None => Err("There is no local server".to_owned()),
        };
        let server = match answer {
            Ok(v) if v.get("path").is_some() => v,
            Ok(v) => v["data"].clone(),
            Err(e) if e.contains("404") => json!({ "error": "The server does not support heap dumps. Update and restart it." }),
            Err(e) => json!({ "error": e }),
        };
        let now = started + clock.elapsed().as_millis() as i64;
        let dir = directory.display().to_string();
        done.complete(match write(&directory, &before, server, started, now) {
            Ok(None) => {
                crate::plugin_host::tasks::log(format!("heap snapshots captured: {dir} complete=true"));
                map([("path", Data::String(dir))])
            }
            Ok(Some(e)) => {
                crate::plugin_host::tasks::log(format!("heap snapshots captured: {dir} complete=false"));
                map([("path", Data::String(dir)), ("error", Data::String(e))])
            }
            Err(e) => {
                crate::plugin_host::tasks::log(format!("heap snapshot capture failed: {e}"));
                map([("error", Data::String(e.to_string()))])
            }
        });
    });
    Ok(())
}

fn private_dir(directory: &Path) -> std::io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(directory)
}

/// Writes `memory.json` and `manifest.json`; the error of the snapshot that failed, if one did.
fn write(dir: &Path, before: &Json, server: Json, started: i64, now: i64) -> std::io::Result<Option<String>> {
    let snapshot = match server["path"].as_str() {
        Some(raw_path) => {
            let verified = browser_policy::verify_local_path(raw_path).ok().filter(|v| v.is_file);
            let path = Path::new(raw_path);
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let bytes = verified.as_ref().and_then(|v| std::fs::metadata(&v.path).ok()).map(|m| m.len());
            // The server joins the directory and file name, so the same join recognizes a snapshot it wrote there.
            let place = if dir.join(&name) == path { json!({ "file": name }) } else { json!({ "path": path.display().to_string() }) };
            let mut entry = json!({ "process": "server", "pid": server["pid"], "bytes": bytes, "memory": server["memory"] });
            if let (Some(o), Some(p)) = (entry.as_object_mut(), place.as_object()) {
                o.extend(p.clone());
            }
            entry
        }
        None => json!({ "process": "server", "error": server["error"].as_str().unwrap_or("The server did not write its heap snapshot.") }),
    };
    let error = snapshot["error"].as_str().map(str::to_owned);
    let after: Json = serde_json::from_str(&crate::plugin_host::memory::report()).unwrap_or(Json::Null);
    let mibs = |v: &Json| -> Json {
        let Some(o) = v.as_object() else { return Json::Null };
        o.iter().filter_map(|(k, v)| Some((k.clone(), json!((v.as_f64()? / 1_048_576.0 * 10.0).round() / 10.0)))).collect::<serde_json::Map<_, _>>().into()
    };
    let app = before;
    write_json(dir, "memory.json", &json!({
        "unit": "MiB",
        "app": { "footprint": mibs(&json!({ "footprint": app["footprint"], "footprintPeak": app["footprintPeak"], "resident": app["resident"] })), "heap": mibs(&app["heap"]), "heapTags": mibs(&app["heap"]["tags"]) },
        "server": if error.is_none() { mibs(&server["memory"]) } else { Json::Null },
        "note": "app: the app's process as the OS counts it, and what its allocator held, before the server's snapshot. server: the server's process.memoryUsage().",
    }))?;
    // Written last, so a script can wait for it.
    write_json(dir, "manifest.json", &json!({
        "created": iso(now),
        "includeServer": true,
        "app": {
            "name": "Brocode",
            "version": crate::updater::build_version(),
            "channel": crate::updater::build_channel(),
            "durationSeconds": (now - started) as f64 / 1000.0,
            "platform": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
        },
        "files": {
            "memory.json": "Where the memory is, per process, in MiB. Start here.",
            "*.heapsnapshot": "Heap snapshots. Open them in the Memory panel of Chrome DevTools.",
        },
        "snapshots": [snapshot],
        "before": app,
        "after": after,
    }))?;
    Ok(error)
}

fn write_json(dir: &Path, name: &str, value: &Json) -> std::io::Result<()> {
    std::fs::write(dir.join(name), serde_json::to_string_pretty(value).unwrap_or_default())
}

/// `toISOString()` of `ms` since the epoch: `2026-11-08T12:34:56.789Z`.
fn iso(ms: i64) -> String {
    let (days, rem) = (ms.div_euclid(86_400_000), ms.rem_euclid(86_400_000));
    // Civil from days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z", rem / 3_600_000, rem / 60_000 % 60, rem / 1000 % 60, rem % 1000)
}

/// [`iso`] without `:` and `.`, for file names (`.replace(/[:.]/g, "")`).
fn stamp(ms: i64) -> String {
    iso(ms).replace([':', '.'], "")
}
