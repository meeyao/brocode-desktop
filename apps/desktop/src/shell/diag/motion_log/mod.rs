//! The live app's motion log: every drawn frame of real use is watched for the glitches the
//! motion gate looks for on fixtures, and each one seen is appended as one JSON line to
//! `motion.jsonl` in the app's log folder (`~/Library/Logs/brocode` on macOS; see
//! `crate::dirs`).
//!
//! It runs in debug builds, and in a release build started with `--motion-log` ([`wanted`]);
//! otherwise it makes nothing at all. It runs from `App::on_frame_drawn`, so it costs nothing while
//! the window is idle and never asks for a frame. The UI thread only takes Rune's light snapshot
//! (`Runtime::motion_elements`) and hands it to a thread of its own, which keeps the visible
//! elements in a ring capped in frames and bytes, and a cheap scan of the last four looks for a still edge that steps at least
//! 2 pt in one frame and stops, or a second element under one key (`#dup`). Only then does it
//! build a [`Recording`] of the ring and run the gate's own [`check`], which decides; a finding of
//! `jump`, `offset-jump`, `appear-then-jump` or `twice` on a frame with no user input is logged.
//! Flights (`Tx::launch`) are followed from launch to rest by [`flights`], which logs the ones
//! that start in the wrong place, head the wrong way, snap at the end or fly at the wrong size.

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{BuildHasherDefault, Hash, Hasher};
use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, SyncSender, TrySendError};
use rune::time::Instant;

use rune::{ElementInfo, MotionElement, NodeId, Runtime};
use serde_json::{Value, json};

use rune::motion_gate::{Recording, check, rule, short};
use crate::{App, Route};
pub use flights::Window;

mod flights;

const FRAMES: usize = 24;
const BYTES: usize = 4 << 20;
/// Frames of values a line carries.
const CONTEXT: usize = 20;
const EVENTS: usize = 16;
const ROTATE_AT: u64 = 4 << 20;
/// Lines per minute at most, so a glitch that repeats every frame can't flood the disk.
const PER_MINUTE: usize = 20;
/// Frames waiting for the watch thread at most.
const QUEUE: usize = 8;
/// Ids whose paths the watch thread keeps before it starts over.
const PATHS: usize = 50_000;
const AXES: [&str; 4] = ["x", "y", "w", "h"];
const LOGGED: [&str; 4] = ["jump", "offset-jump", "appear-then-jump", "twice"];

/// The server events the app has taken, for the log's context. Kept in `App`, written where live
/// events land.
#[derive(Default)]
pub struct Events {
    seq: u64,
    /// The latest event types, a run of one type counted once.
    recent: VecDeque<(String, u32)>,
}

impl Events {
    pub fn note(&mut self, ty: &str) {
        self.seq += 1;
        if let Some(last) = self.recent.back_mut().filter(|l| l.0 == ty) {
            last.1 += 1;
            return;
        }
        if self.recent.len() == EVENTS { self.recent.pop_front(); }
        self.recent.push_back((ty.to_owned(), 1));
    }

    pub(super) fn to_json(&self) -> Vec<Value> {
        self.recent.iter().map(|(ty, n)| json!({"type": ty, "count": n})).collect()
    }
}

struct Sample {
    ms: f64,
    input: bool,
    event: bool,
    load: bool,
    els: Vec<ElementInfo>,
    index: HashMap<u64, u32>,
    bytes: usize,
}

fn hash(path: &str) -> u64 {
    let mut h = DefaultHasher::new();
    path.hash(&mut h);
    h.finish()
}

fn area(r: &[f32; 4]) -> f32 {
    r[2].max(0.0) * r[3].max(0.0)
}

fn dup(e: &ElementInfo) -> bool {
    e.path.rsplit('/').next().is_some_and(|s| s.contains("#dup"))
}

fn shown(e: &ElementInfo) -> bool {
    e.opacity >= rule("faint") && area(&e.visible) >= 1.0
}

impl Sample {
    /// The visible elements, parents remapped to the nearest one kept, without the fields the
    /// checks don't read.
    fn take(els: &[ElementInfo], ms: f64, input: bool, event: bool, load: bool) -> Sample {
        let mut kept = vec![u32::MAX; els.len()];
        let mut out = Vec::new();
        let mut index = HashMap::new();
        let mut bytes = 0;
        for (i, e) in els.iter().enumerate() {
            if area(&e.visible) < 1.0 { continue; }
            let mut parent = e.parent;
            while let Some(p) = parent.filter(|&p| p < i) {
                if kept[p] != u32::MAX { break; }
                parent = els[p].parent;
            }
            let mut slim = e.clone();
            slim.parent = parent.filter(|&p| p < i).map(|p| kept[p] as usize);
            slim.label = None;
            slim.help = None;
            slim.selected = None;
            if let Some(t) = &mut slim.text
                && let Some((cut, _)) = t.char_indices().nth(80)
            {
                t.truncate(cut);
            }
            bytes += std::mem::size_of::<ElementInfo>() + slim.path.len() + slim.key.as_ref().map_or(0, String::len) + slim.text.as_ref().map_or(0, String::len);
            kept[i] = out.len() as u32;
            index.entry(hash(&slim.path)).or_insert(out.len() as u32);
            out.push(slim);
        }
        Sample { ms, input, event, load, els: out, index, bytes }
    }

    fn get(&self, path: &str) -> Option<&ElementInfo> {
        self.index.get(&hash(path)).map(|&i| &self.els[i as usize]).filter(|e| e.path == path)
    }
}

/// The ring and what it already reported. Pure: [`Watch::push`] returns the lines to write.
#[derive(Default)]
pub struct Watch {
    ring: VecDeque<Sample>,
    bytes: usize,
    event_seq: u64,
    dups: HashSet<String>,
    flights: flights::Flights,
}

impl Watch {
    /// One drawn frame: its elements, the window, whether user input landed in it, whether it
    /// landed a load in place (`Tx::settle`, as the gate's loads), the app's events and the
    /// wall-clock milliseconds. Returns a JSON line for each glitch it shows. Elements seen for
    /// the first time go to [`Watch::mount`] first.
    pub fn push(&mut self, els: &[ElementInfo], window: Window, input: bool, load: bool, events: &Events, ms: f64) -> Vec<Value> {
        let viewport = window.size;
        let event = events.seq != self.event_seq;
        self.event_seq = events.seq;
        let sample = Sample::take(els, ms, input, event, load);
        self.bytes += sample.bytes;
        self.ring.push_back(sample);
        while self.ring.len() > FRAMES || self.bytes > BYTES && self.ring.len() > 4 {
            let Some(old) = self.ring.pop_front() else { break };
            self.bytes -= old.bytes;
        }
        let mut lines = self.flights.step(&self.ring, window, events);
        let Some(latest) = self.ring.back() else { return lines };
        let dups: HashSet<String> = latest.els.iter().filter(|e| dup(e) && shown(e)).map(|e| short(&e.path)).collect();
        self.dups.retain(|d| dups.contains(d));
        let fresh: Vec<String> = dups.into_iter().filter(|d| !self.dups.contains(d)).collect();
        let jumped = self.jumped();
        if fresh.is_empty() && !jumped { return lines; }
        self.dups.extend(fresh.iter().cloned());
        let rec = self.recording(viewport);
        let at = self.ring.len().saturating_sub(2);
        lines.extend(check(&rec).into_iter()
            .filter(|f| LOGGED.contains(&f.kind))
            .filter(|f| if f.kind == "twice" { fresh.contains(&f.element) } else { jumped && f.frame == at && !self.ring[at].input })
            .map(|f| self.line(&rec, &f, events)));
        lines
    }

    /// An element drawn for the first time, which may be a flight's destination.
    pub fn mount(&mut self, path: &str, key: Option<&str>) {
        self.flights.mount(path, key);
    }

    /// The frame before the latest has an edge that was still, stepped at least the jump
    /// threshold, and stopped: the shape `check` reports as a jump. Most frames end here.
    fn jumped(&self) -> bool {
        let n = self.ring.len();
        if n < 4 || self.ring[n - 2].input { return false; }
        let (s0, s1, s2, s3) = (&self.ring[n - 4], &self.ring[n - 3], &self.ring[n - 2], &self.ring[n - 1]);
        let threshold = rule("jump");
        s2.els.iter().any(|e| {
            if e.scrolling || e.live_intent != 0 || e.virtual_extent || !shown(e) { return false; }
            let (Some(a), Some(b), Some(c)) = (s1.get(&e.path), s0.get(&e.path), s3.get(&e.path)) else { return false };
            (0..4).any(|x| (e.rect[x] - a.rect[x]).abs() >= threshold && (a.rect[x] - b.rect[x]).abs() < 0.5 && (c.rect[x] - e.rect[x]).abs() < 0.5)
        })
    }

    fn recording(&self, viewport: (f32, f32)) -> Recording {
        let mut rec = Recording::new(viewport, 120.0);
        for s in &self.ring {
            if s.input { rec.input("input"); } else if s.event { rec.input("event"); }
            if s.load { rec.loads.push(rec.frames); }
            rec.sample(&s.els);
        }
        rec
    }

    fn line(&self, rec: &Recording, f: &rune::motion_gate::Finding, events: &Events) -> Value {
        let track = rec.tracks.iter().find(|t| short(&t.path) == f.element && t.frames.iter().any(|s| s.0 == f.frame));
        let rect = |k: usize| track.and_then(|t| t.frames.iter().find(|s| s.0 == k)).map(|s| s.1);
        let deltas: Vec<Value> = match (f.frame.checked_sub(1).and_then(rect), rect(f.frame)) {
            (Some(a), Some(b)) => (0..4).filter(|&x| (b.rect[x] - a.rect[x]).abs() >= 0.5)
                .map(|x| json!({"axis": AXES[x], "from": a.rect[x], "to": b.rect[x], "delta": b.rect[x] - a.rect[x]}))
                .collect(),
            _ => Vec::new(),
        };
        let last = self.ring.back().map_or(0.0, |s| s.ms);
        let from = self.ring.len().saturating_sub(CONTEXT);
        let frames: Vec<Value> = self.ring.iter().enumerate().skip(from).map(|(k, s)| {
            let snap = rect(k);
            json!({
                "frame": k as i64 - f.frame as i64,
                "ms": ((s.ms - last) * 10.0).round() / 10.0,
                "input": s.input,
                "event": s.event,
                "rect": snap.map(|s| s.rect),
                "visible": snap.map(|s| s.vis),
                "opacity": snap.map(|s| s.op),
            })
        }).collect();
        let key = track.and_then(|t| self.ring.iter().rev().find_map(|s| s.get(&t.path)?.key.clone()));
        json!({
            "time": iso(last),
            "rule": f.kind,
            "keys": [key.unwrap_or_else(|| f.element.clone())],
            "element": f.element,
            "detail": f.detail,
            "deltas": deltas,
            "frames": frames,
            "events": events.to_json(),
        })
    }
}

/// One drawn frame as the UI thread hands it to the watch thread.
pub struct Frame {
    seq: u64,
    els: Vec<MotionElement>,
    /// Paths and keys of ids not sent before.
    paths: Vec<(NodeId, String, Option<String>)>,
    /// The paths sent before are forgotten.
    reset: bool,
    window: Window,
    input: bool,
    load: bool,
    /// The app's events, when they changed.
    events: Option<(u64, Vec<(String, u32)>)>,
    ms: f64,
}

/// Node ids are counters, so a multiply spreads them well enough, and costs less than SipHash
/// on every element of every frame.
#[derive(Default)]
pub struct IdHasher(u64);

impl Hasher for IdHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes { self.0 = (self.0 ^ u64::from(b)).wrapping_mul(0x100_0000_01b3); }
    }

    fn write_u64(&mut self, n: u64) {
        self.0 = (self.0 ^ n).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    }
}

/// The UI thread's half: the light snapshot of each drawn frame, and paths only the first time
/// an id shows.
#[derive(Default)]
pub struct Feed {
    seq: u64,
    /// Lists the watch thread is done with, so a frame doesn't allocate one.
    spare: Vec<Vec<MotionElement>>,
    sent: HashSet<NodeId, BuildHasherDefault<IdHasher>>,
    event_seq: u64,
}

impl Feed {
    pub fn frame(&mut self, rt: &Runtime<App>, input: bool, ms: f64) -> Frame {
        let mut els = self.spare.pop().unwrap_or_default();
        rt.motion_elements(&mut els);
        let reset = self.sent.len() > PATHS;
        if reset { self.sent.clear(); }
        let mut paths = Vec::new();
        for m in &els {
            if self.sent.insert(m.id) && let Some((path, key)) = rt.element_path(m.id) {
                paths.push((m.id, path.to_owned(), key.map(str::to_owned)));
            }
        }
        let events = &rt.state.diag.motion_events;
        let events = (events.seq != self.event_seq).then(|| (events.seq, events.recent.iter().cloned().collect()));
        self.event_seq = rt.state.diag.motion_events.seq;
        self.seq += 1;
        let at_end = match &*rt.state.route { Route::Session { id, .. } => rt.state.timeline_fx.ends.at_end(id), _ => false };
        let window = Window { size: rt.viewport, zoom: *rt.state.zoom, at_end };
        Frame { seq: self.seq, els, paths, reset, window, input, load: rt.settled, events, ms }
    }
}

/// The watch thread's half: the paths, the ring and the file.
pub struct Worker {
    seq: u64,
    paths: HashMap<NodeId, (String, Option<String>)>,
    events: Events,
    watch: Watch,
    path: PathBuf,
    file: LogFile,
    minute: (f64, usize),
}

/// The log file: opened at the first glitch and kept open, its size counted here, so a line costs
/// one append (no folder, size or open call each time).
enum LogFile {
    Closed,
    Open { file: std::io::BufWriter<std::fs::File>, len: u64 },
    /// It couldn't be opened: nothing more is tried this run.
    Failed,
}

impl Worker {
    fn new(path: PathBuf) -> Worker {
        Worker { seq: 0, paths: HashMap::new(), events: Events::default(), watch: Watch::default(), path, file: LogFile::Closed, minute: (0.0, 0) }
    }

    /// Takes one frame and returns the lines it shows. A frame dropped on the way empties the
    /// ring, so two frames apart never read as one step.
    pub fn take(&mut self, f: &mut Frame) -> Vec<Value> {
        if f.seq != self.seq + 1 { self.watch = Watch { dups: std::mem::take(&mut self.watch.dups), ..Watch::default() }; }
        self.seq = f.seq;
        if f.reset { self.paths.clear(); }
        for (_, path, key) in &f.paths { self.watch.mount(path, key.as_deref()); }
        self.paths.extend(f.paths.drain(..).map(|(id, path, key)| (id, (path, key))));
        if let Some((seq, recent)) = f.events.take() {
            self.events.seq = seq;
            self.events.recent = recent.into();
        }
        let els = self.infos(&f.els);
        self.watch.push(&els, f.window, f.input, f.load, &self.events, f.ms)
    }

    /// The visible elements as the checks read them, parents remapped to the nearest one kept.
    fn infos(&self, els: &[MotionElement]) -> Vec<ElementInfo> {
        let mut kept = vec![usize::MAX; els.len()];
        let mut out = Vec::new();
        for (i, m) in els.iter().enumerate() {
            let parent = m.parent.map(|p| p as usize).filter(|&p| p < i).map_or(usize::MAX, |p| kept[p]);
            if area(&m.visible) < 1.0 { kept[i] = parent; continue; }
            let Some((path, key)) = self.paths.get(&m.id) else { kept[i] = parent; continue };
            kept[i] = out.len();
            out.push(ElementInfo {
                inert: m.is(MotionElement::EXITING),
                exiting: m.is(MotionElement::EXITING),
                path: path.clone(),
                key: key.clone(),
                role: if m.is(MotionElement::TEXT) { "text" } else { "box" },
                text: (m.text_hash != 0).then(|| format!("{:016x}", m.text_hash)),
                rect: m.rect,
                target: m.rect,
                opacity: m.opacity,
                scale: m.scale,
                fill: if m.is(MotionElement::PAINTS) { rune::Color::WHITE } else { rune::Color::TRANSPARENT },
                parent: (parent != usize::MAX).then_some(parent),
                visible: m.visible,
                in_growing_row: m.is(MotionElement::GROWING),
                scrolling: m.is(MotionElement::SCROLLING),
                virtual_extent: m.is(MotionElement::VIRTUAL),
                live_intent: u32::from(m.is(MotionElement::LIVE)),
                loop_phase: (!m.loop_phase.is_nan()).then_some(f64::from(m.loop_phase)),
                ..ElementInfo::default()
            });
        }
        out
    }

    fn write(&mut self, line: &Value, ms: f64) {
        if ms - self.minute.0 > 60_000.0 { self.minute = (ms, 0); }
        if self.minute.1 >= PER_MINUTE { return; }
        self.minute.1 += 1;
        if matches!(self.file, LogFile::Open { len, .. } if len > ROTATE_AT) {
            self.file = LogFile::Closed;
            std::fs::rename(&self.path, self.path.with_extension("1.jsonl")).ok();
        }
        if matches!(self.file, LogFile::Closed) {
            if let Some(dir) = self.path.parent() { std::fs::create_dir_all(dir).ok(); }
            self.file = match std::fs::OpenOptions::new().create(true).append(true).open(&self.path) {
                Ok(f) => LogFile::Open { len: f.metadata().map_or(0, |m| m.len()), file: std::io::BufWriter::new(f) },
                Err(_) => LogFile::Failed,
            };
        }
        if let LogFile::Open { file, len } = &mut self.file {
            let text = format!("{line}\n");
            // Each line lands whole as it's found: a crash after it keeps it.
            if file.write_all(text.as_bytes()).and_then(|()| file.flush()).is_ok() { *len += text.len() as u64; }
        }
    }
}

/// Whether this run keeps the motion log: always in a debug build; in a release build only when
/// started with `--motion-log` or with `OPENCODE_MOTION_LOG=1` set (a user reporting a glitch;
/// Export logs packs `motion.jsonl`).
/// Off, it costs nothing: no buffer, no thread, no work per frame.
pub fn wanted() -> bool {
    static WANTED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *WANTED.get_or_init(|| cfg!(debug_assertions) || rune::extra_args().iter().any(|a| a == "--motion-log") || std::env::var_os("OPENCODE_MOTION_LOG").is_some_and(|v| v == "1"))
}

/// The live app's log: the feed on the UI thread, and a thread that watches and writes, started
/// on the first live frame. A frame the thread hasn't room for is dropped, never waited on.
pub enum Log {
    /// Not kept this run ([`wanted`]): every frame returns at once.
    Off,
    /// Kept, before the first live frame: nothing made yet.
    Waiting,
    /// The watch thread runs.
    On { feed: Feed, tx: SyncSender<Frame>, back: Receiver<Vec<MotionElement>>, last: Option<Instant> },
}

impl Default for Log {
    fn default() -> Self {
        if wanted() { Log::Waiting } else { Log::Off }
    }
}

impl Log {
    /// `App::on_frame_drawn`: live sessions only, never a fixture.
    pub fn frame(&mut self, rt: &Runtime<App>) {
        if matches!(self, Log::Off) || !rt.state.live() { return; }
        if matches!(self, Log::Waiting) {
            let (tx, rx) = std::sync::mpsc::sync_channel::<Frame>(QUEUE);
            let (done, back) = std::sync::mpsc::sync_channel(QUEUE + 1);
            let mut worker = Worker::new(crate::dirs::motion_log().unwrap_or_else(|| std::env::temp_dir().join("motion.jsonl")));
            if std::thread::Builder::new()
                .name("motion-log".into())
                .spawn(move || {
                    for mut f in rx {
                        for line in worker.take(&mut f) { worker.write(&line, f.ms); }
                        done.try_send(f.els).ok();
                    }
                })
                .is_ok()
            {
                *self = Log::On { feed: Feed::default(), tx, back, last: None };
            } else {
                *self = Log::Off;
                return;
            }
        }
        let Log::On { feed, tx, back, last } = self else { return };
        let now = Instant::now();
        let input = rune::last_input().is_some_and(|t| last.is_none_or(|last| t > last));
        *last = Some(now);
        static ANCHOR: std::sync::OnceLock<(Instant, f64)> = std::sync::OnceLock::new();
        let (at, wall) = *ANCHOR.get_or_init(|| (now, crate::wall_ms() as f64));
        let ms = wall + now.saturating_duration_since(at).as_secs_f64() * 1000.0;
        feed.spare.extend(back.try_iter());
        if let Err(TrySendError::Full(f) | TrySendError::Disconnected(f)) = tx.try_send(feed.frame(rt, input, ms)) {
            // What it carried for the first time goes again with the next one.
            for (id, ..) in &f.paths { feed.sent.remove(id); }
            if f.events.is_some() { feed.event_seq = 0; }
            feed.spare.push(f.els);
        }
    }
}

/// `2026-10-27T21:40:05.123Z` for wall-clock milliseconds.
pub(crate) fn iso(ms: f64) -> String {
    let ms = ms.max(0.0) as i64;
    let (days, rem) = (ms.div_euclid(86_400_000), ms.rem_euclid(86_400_000));
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z", rem / 3_600_000, rem / 60_000 % 60, rem / 1000 % 60, rem % 1000)
}

