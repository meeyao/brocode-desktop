//! How a streaming reply shows each line it wraps onto (Timeline › New line reveal, p252).
//! Working sits its lane's gap under the reply; the reply's row opens each new line on its own
//! quick spring (Line grow), and nothing it holds paints below the row's moving edge:
//!
//! - Clip: the row clips its words at its edge, so the line shows top-down as the row opens.
//! - Grow, then show: the row opens first, and the line's words come once it has.
//! - Soft edge: Clip, its edge fading over a few points while it moves.
//! - Held room: the line of room under a streaming reply that #304 kept, for comparison.
//!
//! A reply with a fenced block in it (a diagram, an outline, code) climbs at the stream's pace
//! instead: one build can add many lines.

use std::collections::{HashMap, VecDeque};

use rune::{El, Spring, Tx, col};
use rune_knobs::{Knob, Knobs};

use crate::TimelineHost;

pub const NEW_LINE_REVEAL: Knob = Knob::choice("New line reveal", &["Clip", "Grow, then show", "Soft edge", "Held room"], 2)
    .about("How streamed text shows each new line")
    .hints(&["Row clips its words as it opens", "Words come once the line is open", "Clip edge fades while it moves", "A line of room under the reply"]);
/// The row taking a new line, as a spring's duration in milliseconds.
pub const LINE_GROW: Knob = Knob::new("Line grow", 40.0, 240.0, 80.0).step(5.0).about("Milliseconds streaming text takes to open a line");
/// Soft edge's fade, in points.
pub const SOFT_EDGE: Knob = Knob::new("Soft edge", 2.0, 24.0, 8.0).step(1.0).about("Points the soft clip edge fades over");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Reveal {
    Clip,
    GrowThenShow,
    SoftEdge,
    HeldRoom,
}

impl Reveal {
    pub const ALL: [Reveal; 4] = [Reveal::Clip, Reveal::GrowThenShow, Reveal::SoftEdge, Reveal::HeldRoom];

    /// Its value on [`NEW_LINE_REVEAL`].
    pub fn knob(self) -> f32 {
        Reveal::ALL.iter().position(|&r| r == self).unwrap_or(0) as f32
    }
}

pub fn reveal(k: &Knobs) -> Reveal {
    Reveal::ALL.get(k.get(&NEW_LINE_REVEAL).round() as usize).copied().unwrap_or(Reveal::Clip)
}

pub fn line_grow(k: &Knobs) -> Spring {
    Spring::new(k.get(&LINE_GROW) / 1000.0, 0.0)
}

/// The feather on the row's moving edge, in points: none but for Soft edge.
pub fn feather(k: &Knobs) -> f32 {
    if reveal(k) == Reveal::SoftEdge { k.get(&SOFT_EDGE) } else { 0.0 }
}

/// The room Held room keeps under a streaming reply, over Working: a line of reply text.
pub(crate) const CUSHION: f32 = 24.0;


/// How much of each streaming text part shows, by "message:part": Desktop's paced text, and
/// Kit's Grow, then show on top of it.
#[derive(Clone, Default, PartialEq)]
pub struct Hold {
    /// Desktop's paced reveal of each live text part (`PacedTextState`).
    paced: HashMap<String, Paced>,
    /// Grow, then show: when each part's text reached each length, oldest first. While it holds
    /// more than one length, its words show only the oldest; its row already takes the newest.
    held: HashMap<String, VecDeque<(f64, usize)>>,
}

impl Hold {
    /// Grow, then show: the bytes of part `part` of `message` whose words show, while its row
    /// takes more.
    pub fn shown(&self, message: &str, part: usize) -> Option<usize> {
        if self.held.is_empty() {
            return None;
        }
        self.held.get(&format!("{message}:{part}")).filter(|at| at.len() > 1).map(|at| at[0].1)
    }

    /// The bytes of part `part` of `message` the paced reveal shows so far, while it paces.
    pub fn paced(&self, message: &str, part: usize) -> Option<usize> {
        if self.paced.is_empty() {
            return None;
        }
        match self.paced.get(&format!("{message}:{part}")) {
            Some(Paced::Pacing { shown, .. }) => Some(*shown),
            _ => None,
        }
    }
}

/// Desktop's `TEXT_RENDER_PACE_MS` (`packages/session-ui/src/message/paced-text-machine.ts`):
/// 24 ms fallback when the display refresh rate is unknown or slower, and the reference time base
/// for slice sizing so high-refresh displays keep the same catch-up rate without 42 fps stepping.
const BASE_PACE: f64 = 0.024;
/// Upper bound on paced text updates per second: displays at 60–180 Hz (including 120/144/160 Hz)
/// step on every refresh; faster panels (240–500 Hz) lock to an integer frame divisor (e.g. 240 Hz
/// -> 120 Hz, 360 Hz -> 180 Hz) to bound per-frame markdown/layout work with zero beat judder.
const MAX_PACE_HZ: f64 = 180.0;
/// `TEXT_RENDER_IMMEDIATE`: a live backlog up to this long shows at once.
const IMMEDIATE: usize = 512;
/// `nextSliceEnd` maximum bytes per [`BASE_PACE`] window.
const BASE_MAX_SLICE: f64 = 256.0;
/// `nextSliceEnd` remaining fraction kept per [`BASE_PACE`] window (`1 - 1/4`).
const BASE_KEEP: f64 = 0.75;

/// Target reveal cadence derived from the window's display refresh rate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaceCadence {
    /// Target interval between paced slices (seconds).
    pub interval: f64,
    /// Earliest interval after the last slice before the next slice fires (seconds). When synced
    /// to a known display rate this sits a quarter display frame below `interval` so minor
    /// compositor/vsync jitter never skips a frame and halves the rate, while mid-frame wakes
    /// still wait for the next refresh.
    pub guard: f64,
}

impl PaceCadence {
    pub fn for_display_hz(display_hz: Option<f64>) -> Self {
        let Some(hz) = display_hz.filter(|hz| hz.is_finite() && *hz > 1.0 / BASE_PACE) else {
            return Self { interval: BASE_PACE, guard: BASE_PACE };
        };
        let divisor = if hz > MAX_PACE_HZ { (hz / MAX_PACE_HZ).ceil() } else { 1.0 };
        let interval = (divisor / hz).min(BASE_PACE);
        let guard = ((divisor - 0.25) / hz).clamp(0.001, interval);
        Self { interval, guard }
    }
}

/// A streamed text part's reveal: Desktop's `PacedTextState`
/// (`packages/session-ui/src/message/paced-text-machine.ts:13-16`), adapted to step at the
/// display's refresh cadence rather than Desktop's fixed 24 ms DOM throttle while preserving
/// Desktop's bytes/sec throughput and [`IMMEDIATE`] fast path. A live backlog of more than
/// [`IMMEDIATE`] bytes (a reconnect's catch-up, a fast model's burst) shows in display-paced
/// slices rather than all at once. `disposed` is the part leaving [`Hold`]. Desktop counts
/// UTF-16 units; these count bytes, cut on character boundaries.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Paced {
    /// The whole text shows: `shown` bytes of it, `sig` the signature of those bytes.
    Synced { shown: usize, sig: u64 },
    /// `shown` bytes of a `target` bytes long text show; the last slice landed (or pacing began)
    /// at `at` and the next slice is due at `due` on the app clock. A delta while it paces keeps
    /// the due slice no later than before, so a steady stream can't postpone it.
    Pacing { shown: usize, sig: u64, target: usize, at: f64, due: f64 },
}

#[derive(Clone, Copy, Debug)]
pub enum PacedEvent<'a> {
    /// The part's text now, whether its turn streams (`sync`), and the display's reveal cadence.
    Sync { text: &'a str, live: bool, now: f64, cadence: PaceCadence },
    /// The clock reached `now` (`tick`): a due slice shows.
    Tick { text: &'a str, now: f64, cadence: PaceCadence },
}

impl Paced {
    /// `createPacedTextState`: a part shows all its text as it mounts.
    pub fn mount(text: &str) -> Paced {
        Paced::Synced { shown: text.len(), sig: sig(text) }
    }

    /// `transitionPacedText`. Returns the next slice's due time while it paces.
    pub fn step(&mut self, event: PacedEvent) -> Option<f64> {
        *self = match (*self, event) {
            (Paced::Pacing { shown, target, at, due, .. }, PacedEvent::Tick { text, now, cadence })
                if now + 1e-9 >= due && text.is_char_boundary(target) =>
            {
                if target - shown <= IMMEDIATE {
                    Paced::mount(&text[..target])
                } else {
                    let dt = (now - at).clamp(cadence.interval, BASE_PACE);
                    let end = slice_end(&text[..target], shown, dt);
                    if end >= target {
                        Paced::mount(&text[..target])
                    } else {
                        Paced::Pacing { shown: end, sig: sig(&text[..end]), target, at: now, due: now + cadence.guard }
                    }
                }
            }
            (state, PacedEvent::Tick { .. }) => state,
            (state, PacedEvent::Sync { text, live, now, cadence }) => {
                let (shown, shown_sig) = match state {
                    Paced::Synced { shown, sig } | Paced::Pacing { shown, sig, .. } => (shown, sig),
                };
                let same = |n: usize| text.is_char_boundary(n) && n <= text.len();
                match state {
                    Paced::Synced { .. } if text.len() == shown && same(shown) && sig(text) == shown_sig => state,
                    Paced::Pacing { target, .. } if live && text.len() == target => state,
                    // The length check runs first, so an ordinary delta never reads the whole text.
                    _ if !live || text.len().saturating_sub(shown) <= IMMEDIATE || !same(shown) || sig(&text[..shown]) != shown_sig => Paced::mount(text),
                    Paced::Pacing { at, due, .. } => Paced::Pacing { shown, sig: shown_sig, target: text.len(), at, due: due.min(now + cadence.guard) },
                    Paced::Synced { .. } => Paced::Pacing { shown, sig: shown_sig, target: text.len(), at: now, due: now + cadence.guard },
                }
            }
        };
        match *self {
            Paced::Pacing { due, .. } => Some(due),
            Paced::Synced { .. } => None,
        }
    }
}

/// `nextSliceEnd`: a quarter of what's left, at most 256 bytes per [`BASE_PACE`] (24 ms), scaled
/// proportionally to `dt` so high-refresh displays advance smoothly each frame at the same overall
/// throughput, ended just past a space or punctuation within the next 8 when there is one.
fn slice_end(text: &str, start: usize, dt: f64) -> usize {
    let remaining = text.len() - start;
    let step = if dt >= BASE_PACE - 1e-9 {
        256.min(remaining.div_ceil(4))
    } else {
        let ratio = (dt / BASE_PACE).clamp(0.0, 1.0);
        let frac = ((remaining as f64) * (1.0 - BASE_KEEP.powf(ratio))).ceil() as usize;
        let cap = (BASE_MAX_SLICE * ratio).round() as usize;
        cap.max(1).min(frac.max(1))
    };
    let end = text.ceil_char_boundary(text.len().min(start + step));
    let snap = text[end..].char_indices().take(8).find(|(_, c)| c.is_whitespace() || ".,!?;:)]".contains(*c));
    snap.map_or(end, |(i, c)| end + i + c.len_utf8())
}

/// What tells a text's start apart: its length and its first and last 32 bytes, read only when a
/// burst lands.
fn sig(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let b = text.as_bytes();
    let mut h = std::collections::hash_map::DefaultHasher::new();
    b.len().hash(&mut h);
    b[..b.len().min(32)].hash(&mut h);
    b[b.len().saturating_sub(32)..].hash(&mut h);
    h.finish()
}

/// Keeps [`Hold`] up with the open session's streaming text: its paced reveal, and Grow, then
/// show's held words.
pub fn poll<S: TimelineHost>(s: &mut S, now: f64, tx: &mut Tx) -> bool {
    let grow_then_show = reveal(s.knobs()) == Reveal::GrowThenShow;
    let delay = f64::from(s.knobs().get(&LINE_GROW)) / 1000.0;
    // Every poll comes here: with nothing streaming in the shown session and nothing held, the
    // hold isn't written.
    let live = s.active_session_id().is_some_and(|sid| s.is_session_live(sid));
    if !live && s.line_hold().paced.is_empty() && s.line_hold().held.is_empty() {
        return false;
    }
    let cadence = PaceCadence::for_display_hz(s.display_hz());
    let mut hold = s.line_hold().clone();
    let (mut changed, mut next) = (false, f64::INFINITY);
    let mut lens: Vec<(String, usize)> = Vec::new();
    let mut live_keys = std::collections::HashSet::new();
    let s_ref: &S = &*s;
    if let Some(sid) = s_ref.active_session_id()
        && s_ref.is_session_live(sid)
    {
        s_ref.with_transcript(sid, |t| {
            let Some(opencode_kit::model::Message::Assistant(a)) = t.messages.last() else { return };
            for (i, part) in a.parts.iter().enumerate() {
                let opencode_kit::model::Part::Text { text } = part else { continue };
                // Desktop's row mounts once `text.trim()` is non-empty (`timeline-next/projection.ts:117-135`),
                // so `createPacedTextState` mounts the first rendered chunk whole before pacing later bursts.
                if text.trim().is_empty() {
                    continue;
                }
                let key = format!("{}:{i}", a.id);
                let was = hold.paced(&a.id, i);
                let paced = hold.paced.entry(key.clone()).or_insert_with(|| Paced::mount(text));
                let mut due = paced.step(PacedEvent::Sync { text, live: true, now, cadence });
                if due.is_some_and(|d| now + 1e-9 >= d) {
                    due = paced.step(PacedEvent::Tick { text, now, cadence });
                }
                if let Some(d) = due {
                    next = next.min(d - now);
                }
                changed |= hold.paced(&a.id, i) != was;
                let shown = match hold.paced[&key] {
                    Paced::Synced { shown, .. } | Paced::Pacing { shown, .. } => shown,
                };
                lens.push((key.clone(), shown));
                live_keys.insert(key);
            }
        });
    }
    // A part that stopped streaming shows whole (`sync` with `live: false`).
    let before = hold.paced.len();
    hold.paced.retain(|k, _| live_keys.contains(k));
    changed |= hold.paced.len() != before;
    if !grow_then_show {
        changed |= !hold.held.is_empty();
        hold.held.clear();
    } else {
        for (key, len) in lens {
            let at = hold.held.entry(key).or_insert_with(|| VecDeque::from([(now - delay, 0)]));
            match at.back() {
                Some(&(_, last)) if len < last => *at = VecDeque::from([(now - delay, len)]),
                Some(&(_, last)) if len == last => {}
                _ => at.push_back((now, len)),
            }
        }
        hold.held.retain(|key, at| {
            let was = (at.len() > 1).then(|| at[0].1);
            // What showed a line-grow ago shows now; once it's all shown, the part shows whole.
            while at.len() > 1 && at[1].0 <= now - delay + 1e-9 {
                at.pop_front();
            }
            if let Some(&(t, _)) = at.get(1) {
                next = next.min(t + delay - now);
            }
            changed |= (at.len() > 1).then(|| at[0].1) != was;
            // Its last length is the base for the next words while it streams.
            at.len() > 1 || live_keys.contains(key)
        });
    }
    if hold != *s.line_hold() {
        *s.line_hold_mut() = hold;
    }
    if next.is_finite() {
        tx.wake_after(std::time::Duration::from_secs_f64(next.max(0.0)));
    }
    changed
}

/// A reply's words (`md` of the text and its word fade) in `body`, its padded column. For Grow,
/// then show, the column takes the whole text's height from an unseen copy in the same grid cell
/// while only the first `shown` bytes draw, so the row opens a line before its words come. The
/// words stay its first child either way, keeping their place and their names.
pub fn words(body: El, md: impl Fn(&str, f32) -> El, text: &str, fade: f32, shown: Option<usize>) -> El {
    match shown.filter(|&n| n < text.len() && text.is_char_boundary(n)) {
        None => body.child(md(text, fade)),
        Some(n) => body
            .equal_columns(1)
            .child(md(&text[..n], fade).grid_cell(0))
            .child(col().key("words-size").w_full().grid_cell(0).opacity(0.0).inert().child(md(text, 0.0))),
    }
}

/// The room over Working: [`CUSHION`] for Held room, else none.
pub fn room<S: TimelineHost>(s: &S) -> f32 {
    if reveal(s.knobs()) == Reveal::HeldRoom { CUSHION } else { 0.0 }
}
