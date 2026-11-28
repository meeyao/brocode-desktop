//! The Working row under a running turn: Desktop's label, with a smooth sweep
//! of light across the words, tuned by the Working knobs (⌘⇧K, Working).
//!
//! The sweep is loops the renderer runs by itself, so nothing lays out again per frame. Its
//! rhythm is keyed to the run rather than to when a node mounted: [`sync`] notes when each
//! session's run started, and a loop that mounts mid-run waits for its place (a slide's
//! `delay`). A new label keeps the same nodes, so their loops carry on at the same phase
//! (`LoopTrack::retime`).

use std::collections::HashMap;

use rune::color::{hex, srgb_decode, srgb_encode};
use rune::{Color, Easing, El, LoopSpec, TextStyle, el, text};

use crate::TimelineHost;
use crate::knobs::{
    WORKING_BAND, WORKING_DIRECTION, WORKING_INTENSITY, WORKING_PACE, WORKING_PAUSE, WORKING_PERIOD, WORKING_SOFTNESS, WORKING_SPEED, WORKING_UNIT,
    secs,
};
use crate::palette::Pal;
use crate::style::UI;

#[derive(Default)]
pub struct Working {
    /// When each session's current run started, on the app clock: a session listed is live.
    since: HashMap<String, f64>,
    /// A label in place of "Working" (the storybook's L).
    label: Option<&'static str>,
    /// The sweep across a change of label.
    pub relabeled: Relabel,
}

/// The sweep across a change of the label's words (Kit's design: Desktop's `TextShimmer` starts
/// a new text's sweep afresh, `text-shimmer/machine.ts` has no such phase).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Relabel {
    #[default]
    Steady,
    /// The words changed from `old` at `at` on the app clock: the sweep's pass then under way
    /// carries on from where it was ([`finish_pass`]) until it ends.
    Finishing { old: &'static str, at: f64 },
}

/// What a working session waits on the user for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Waiting {
    Approval,
    Answer,
}

/// The status line's words under a working turn (`timeline-next/status.ts`'s `timelineStatus`):
/// Working, unless the turn waits on the user, on a shell command or on its subagents. The
/// subagent wait is this app's own: official's status line goes with its session's own run, and
/// here a session whose subagents still run stays live under it (`TimelineHost::is_session_live`),
/// where Working read as the model thinking while nothing in the session itself ran.
pub fn status<S: TimelineHost>(s: &S, session: &str) -> &'static str {
    if let Some(label) = s.working_label_for(session).or(s.working_state().label) {
        return label;
    }
    match s.session_waiting(session) {
        Some(Waiting::Approval) => return opencode_i18n::t!("session.timeline.status.waitingApproval"),
        Some(Waiting::Answer) => return opencode_i18n::t!("session.timeline.status.waitingAnswer"),
        None => {}
    }
    // The call the turn's last step ends on while it runs, and how many of it run there: a
    // foreground subagent holds its turn as a shell command does.
    let blocking = s.with_transcript(session, |t| {
        use opencode_kit::model::{Message, Part, ToolStatus};
        let Some(Message::Assistant(a)) = t.messages.iter().rev().find(|m| matches!(m, Message::Assistant(_))) else { return None };
        // A running call's tool: whether it is a shell, or a subagent.
        let running = |p: &Part| match p {
            Part::Tool { name, status: ToolStatus::Running | ToolStatus::Pending, .. } => {
                let name = crate::activity::rules::current_name(name);
                Some((name == "shell", name == "subagent"))
            }
            _ => None,
        };
        let (shell, subagent) = a.parts.last().and_then(running)?;
        Some((shell, subagent, a.parts.iter().filter(|p| matches!(running(p), Some((_, true)))).count()))
    }).flatten();
    let subagents = match blocking {
        Some((true, ..)) => return opencode_i18n::t!("session.timeline.status.waitingShell"),
        Some((_, true, n)) => n,
        _ => s.subagents_awaited(session),
    };
    match subagents {
        0 => opencode_i18n::t!("session.timeline.status.working"),
        1 => opencode_i18n::t!("rune.session.status.waitingSubagent"),
        _ => opencode_i18n::t!("rune.session.status.waitingSubagents"),
    }
}

/// Whether the running turn ends in its Thinking row, which stands in for the status line's
/// Working (`message-timeline.tsx:495-500` `liveThought`: the turn has a `Thinking` row): the
/// thought the model is writing, or the finished thoughts the turn ends on while nothing else
/// shows after them (`turns::fold_with`, anomalyco/opencode#66349). For a session that is busy.
pub fn thinking<S: TimelineHost>(s: &S, session: &str) -> bool {
    use opencode_kit::model::Message;
    let detail = s.timeline_detail();
    if detail.placement(crate::detail::Category::Thinking) == crate::detail::Placement::Hidden {
        return false;
    }
    s.with_transcript(session, |t| {
        // The turn's work: what follows the last message that is neither a step nor a notice.
        let start = t.messages.iter().rposition(|m| !matches!(m, Message::Assistant(_) | Message::Notice(_))).map_or(0, |i| i + 1);
        let entries = crate::timeline::entries(&t.messages[start..], &detail);
        matches!(crate::turns::fold_with_flags(&entries, true, &detail, s.full_codemode(session)).last(), Some(crate::turns::Block::Thought { live: true, .. }))
    }) == Some(true)
}

impl Working {
    pub fn label(&self) -> &'static str {
        self.label.unwrap_or(opencode_i18n::t!("session.timeline.status.working"))
    }
    /// When `session`'s run started, while the host counts it live; the sweep keeps its rhythm
    /// from then.
    pub fn since(&self, session: &str) -> Option<f64> {
        self.since.get(session).copied()
    }
    /// Shows `label` from `now` on (app clock).
    pub fn relabel(&mut self, label: &'static str, now: f64) {
        if self.relabeled == Relabel::Steady {
            self.relabeled = Relabel::Finishing { old: self.label(), at: now };
        }
        self.label = Some(label);
    }
}

/// After the frame's events: starts the clock of each run that began and drops those that
/// ended, and ends a pass a new label interrupted once it's over. True when what's drawn
/// changed.
pub fn sync<S: TimelineHost>(s: &mut S) -> bool {
    // Every poll comes here: with every live session counted, none ended and no label
    // finishing, nothing below changes, so the state isn't written.
    let w = s.working_state();
    let mut quiet = !matches!(w.relabeled, Relabel::Finishing { .. }) && w.since.keys().all(|id| s.is_session_live(id));
    if quiet {
        s.for_each_tracked_session(|id| quiet &= w.since.contains_key(id) || !s.is_session_live(id));
    }
    if quiet {
        return false;
    }
    let clock = s.clock();
    let mut working = std::mem::take(s.working_state_mut());
    let runs = working.since.len();
    working.since.retain(|id, _| s.is_session_live(id));
    let mut changed = working.since.len() != runs;
    s.for_each_tracked_session(|id| {
        if !working.since.contains_key(id) && s.is_session_live(id) {
            working.since.insert(id.to_owned(), clock);
            changed = true;
        }
    });
    if let Relabel::Finishing { old, at } = working.relabeled {
        let id = s.active_session_id();
        let since = id.and_then(|id| working.since.get(id).copied());
        let b = band(s, width(old));
        let pass = |t: f64, since: f64| ((t - since) / b.period).floor();
        if !since.is_some_and(|since| pass(clock, since) == pass(at, since)) {
            // The new words' passes run from the end of the one they interrupted, where the
            // band is out of sight: a row that mounts again picks up the same rhythm.
            if let (Some(id), Some(since)) = (id, since) {
                working.since.insert(id.to_owned(), since + (pass(at, since) + 1.0) * b.period);
            }
            working.relabeled = Relabel::Steady;
            changed = true;
        }
    }
    *s.working_state_mut() = working;
    changed
}

/// The label alone on the text column, as Desktop's Working is. Hidden, it is drawn still: its
/// loops would keep waking frames.
pub fn row<S: TimelineHost>(s: &S, p: &Pal, session: &str, live: bool) -> El {
    rune::row().h(LINE).center_y().child(label(s, p, session, live && !s.reduced_motion()))
}

/// Desktop's Working lane under a turn (`h-9 pt-3`): its row is the bottom 24 of 36.
/// Official's `StatusLane` (`timeline-next.css:523-541`) lays the 16 pt label flush at its top
/// gap without the extra 4 pt centering of a 24 pt box (`LANE_TEXT_TOP`).
pub const LANE_PAD: f32 = 12.0;
pub const LANE_TEXT_TOP: f32 = 10.0;
pub const H: f32 = 24.0;
pub const LANE: f32 = LANE_PAD + H;
/// The status line's height (`--line-height-compact`).
pub const LINE: f32 = 16.0;
/// The lane's row (`timeline-status-lane`: flex, centred, 8 apart): the status line and what
/// sits beside it, the background hint's 24 pt button centred on the line and reaching 4 past it.
pub const ROW_TOP: f32 = LANE_TEXT_TOP - (H - LINE) / 2.0;

/// Desktop's reply Copy in that lane (`text-part-copy-wrapper`): a 20 pt square holding a 14 pt
/// glyph in its middle, 8 pt under the reply's last line, pulled 2 pt toward the text edge when
/// it leads the footer (`message-part.css:607-609`, `margin-inline-start: -2px`).
pub const COPY: f32 = 20.0;
pub const COPY_GLYPH: f32 = 14.0;
pub const COPY_AT: (f32, f32) = (-2.0, 8.0);

/// A label's width at the row's 13 px Inter.
pub fn width(label: &str) -> f32 {
    rune::text::measure(label, &[], UI, None).0
}

fn label<S: TimelineHost>(s: &S, p: &Pal, session: &str, on: bool) -> El {
    let words = status(s, session);
    if !on {
        // Under reduced motion official inherits `[data-slot="timeline-status-label"]`'s muted ink
        // (`timeline-next.css:566`, `packages/ui/src/typography/text-shimmer/text-shimmer.css:135-147`),
        // not shimmer's faint base color.
        return rune::row().child(text(words).key("label").font(UI).color(p.muted));
    }
    let (weak, strong) = colors(p);
    let base = text(words).key("label").font(UI).color(weak);
    let clock = s.clock();
    let run = Run { since: s.working_state().since.get(session).copied().unwrap_or(clock), now: clock };
    let lit = weak.mix(strong, s.knobs().get(&WORKING_INTENSITY));
    let w = width(words);
    let mut b = band(s, w);
    let mut shown = words;
    if let Relabel::Finishing { old, at } = s.working_state().relabeled {
        let was = band(s, width(old));
        if ((clock - run.since) / was.period).floor() == ((at - run.since) / was.period).floor() {
            let f = ((at - run.since) / was.period).rem_euclid(1.0) as f32;
            let here = was.x0 + f * (was.x1 - was.x0);
            let past = if was.x1 >= was.x0 { here >= width(old) } else { here + was.width <= 0.0 };
            if past {
                // Out of sight past the old words: the pass ends unseen, over them.
                (b, shown) = (was, old);
            } else {
                b = finish_pass(was, w, f);
            }
        }
    }
    sweep(run, shown, UI, base, lit, b)
}

/// Another running label in the Working label's light (Desktop's `TextShimmer`, as a running
/// tool's title has it), its rhythm counted from `since` on the app clock. Still when motion
/// is reduced, in the label's own ink, as Desktop's `data-motion="reduce"` shows the text it
/// shimmers (`text-shimmer.css`: the base layer whole, the shimmer layer `color: inherit`).
pub fn shimmer<S: TimelineHost>(s: &S, p: &Pal, words: &str, font: TextStyle, since: f64) -> El {
    let (weak, strong) = colors(p);
    if s.reduced_motion() {
        return rune::row().child(text(words.to_owned()).key("label").font(font).color(p.text));
    }
    let base = text(words.to_owned()).key("label").font(font).color(weak);
    let lit = weak.mix(strong, s.knobs().get(&WORKING_INTENSITY));
    let w = rune::text::measure(words, &[], font, None).0;
    sweep(Run { since, now: s.clock() }, words, font, base, lit, band(s, w))
}

/// Where a run is: when it started and the app clock now.
#[derive(Clone, Copy)]
pub struct Run {
    pub since: f64,
    pub now: f64,
}

impl Run {
    /// The delay that starts a loop mounting now in step with the run. A loop that mounts
    /// mid-pass waits for the next.
    pub fn delay(self, period: f64) -> f64 {
        let d = (-(self.now - self.since) / period).rem_euclid(1.0) * period;
        if d > period - 1e-6 { 0.0 } else { d }
    }
}

/// A sweep: a band `width` wide whose left edge goes from `x0` to `x1` once a `period`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Band {
    pub width: f32,
    /// The soft part of each edge.
    pub soft: f32,
    pub x0: f32,
    pub x1: f32,
    pub period: f64,
}

/// The sweep over a label `w` wide. The band leaves the label entirely before it comes round;
/// a pause is more travel past the end, out of sight.
pub fn band<S: TimelineHost>(s: &S, w: f32) -> Band {
    let k = s.knobs();
    let width = (if k.get(&WORKING_UNIT) > 0.5 { w * k.get(&WORKING_BAND) / 100.0 } else { k.get(&WORKING_BAND) }).max(4.0);
    let span = w + width;
    let pass = if k.get(&WORKING_PACE) > 0.5 { f64::from(span / k.get(&WORKING_SPEED)) } else { secs(k, &WORKING_PERIOD) };
    let period = pass + secs(k, &WORKING_PAUSE);
    let soft = k.get(&WORKING_SOFTNESS) * width / 2.0;
    let x0 = -width;
    let x1 = x0 + span * (period / pass) as f32;
    let (x0, x1) = if k.get(&WORKING_DIRECTION) > 0.5 { (w - x0 - width, w - x1 - width) } else { (x0, x1) };
    Band { width, soft, x0, x1, period }
}

/// The rest of a pass a new label `w` wide interrupted `at` of the way through: the band
/// carries on from where it was, at its old size, speed and period. Only if that wouldn't
/// clear the new words before the pass ends does it speed up, just enough to: it never jumps,
/// and the next pass is the new label's, in the run's rhythm.
fn finish_pass(was: Band, w: f32, at: f32) -> Band {
    let here = was.x0 + at * (was.x1 - was.x0);
    let speed = was.x1 - was.x0;
    let left = (1.0 - at).max(1e-3);
    let slope = if speed >= 0.0 { speed.max((w - here) / left) } else { speed.min((-was.width - here) / left) };
    Band { x0: here - at * slope, x1: here + (1.0 - at) * slope, ..was }
}

/// A soft-edged window of the lit `words` sliding over the weak label: the window travels, and
/// the lit copy inside it slides back so it stays on the label.
fn sweep(run: Run, words: &str, font: TextStyle, base: El, lit: Color, b: Band) -> El {
    let w = b.width;
    let delay = run.delay(b.period);
    let spec = |a: f32, c: f32| LoopSpec { delay, ..LoopSpec::slide(b.period, [a, 0.0], [c, 0.0]).easing(Easing::LINEAR) };
    let copy = el().key("lit").absolute(0.0, 0.0).w(w).h_full().looping(spec(-b.x0 / w, -b.x1 / w))
        .child(text(words.to_owned()).font(font).color(lit).absolute(0.0, 0.0));
    rune::row().child(base).child(
        el().key("sheen").absolute(0.0, 0.0).w(w).h_full().clip_fade(b.soft, 0.0, b.soft, 0.0).looping(spec(b.x0 / w, b.x1 / w)).child(copy),
    )
}

/// Desktop's `--text-weak` and `--text-strong` (white at 42.2% and 93.6% on dark), each solved
/// to the opaque colour a browser blends over the page in sRGB.
pub fn colors(p: &Pal) -> (Color, Color) {
    if p.theme > 0 {
        return (p.muted, p.text);
    }
    if !p.dark {
        return (hex("#8f8f8f").unwrap_or(p.muted), hex("#171717").unwrap_or(p.text));
    }
    let bg = p.bg_base;
    let over = |a: f32| {
        let c = |b: f32| srgb_decode(srgb_encode(b) + (1.0 - srgb_encode(b)) * a);
        Color { r: c(bg.r), g: c(bg.g), b: c(bg.b), a: 1.0 }
    };
    (over(0.422), over(0.936))
}
