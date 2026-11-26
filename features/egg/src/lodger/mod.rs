//! The lodger in the final "e" (Desktop's `new-session/wordmark.tsx` `mountLodger`): eyes peek out
//! when the pointer comes near; dragging them pulls the lodger and a single corner pin out on a
//! stretchy neck; pull far enough and the pin sets off the pin board. From its perch beside the
//! board it can be carried back into the hole, tapped to hop, or dropped on the composer to play.
//! Lengths are the logo's 204x42 units; the stage maps them to panel points.

mod steps;
pub mod render;

use super::wordmark::{self, Event, State};
use super::{FrameLoop, FrameLoopEvent};

pub const W: f64 = 204.0;
pub const HOLE: [f64; 2] = [192.0, 15.0];
pub const REST: [f64; 2] = [192.0, 21.0];
pub const PIN_SLOT: [f64; 2] = [196.5, 10.5];
pub const GROUND: f64 = 36.0;
pub const HOME: f64 = 216.0;
const SLIP: f64 = 60.0;
const HOLE_MAGNET: f64 = 24.0;
/// A blink (`blinkOnce`): the eyes close to a tenth and open again over 160 ms, eased in and out.
pub const BLINK: f64 = 0.16;

/// Each letter's bottom-centre pivot.
pub const LETTER_CX: [f64; 7] = [12.0, 42.0, 72.0, 102.0, 132.0, 162.0, 192.0];

/// Everything the lodger draws, as `render` writes it (`c` in Desktop).
#[derive(Clone, Copy, Debug)]
pub struct Critter {
    pub t0: f64,
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
    pub from_x: f64,
    pub from_y: f64,
    pub rot: f64,
    pub crouch: f64,
    pub body: f64,
    pub neck: f64,
    pub eyes: f64,
    pub near: f64,
    pub look_x: f64,
    pub look_y: f64,
    pub wide: f64,
    pub squint: f64,
    pub notch: f64,
    pub pin_opacity: f64,
    pub pin_x: f64,
    pub pin_y: f64,
    pub pin_rot: f64,
    pub pin_scale: f64,
    pub ball: f64,
    pub stretch: f64,
    pub angle: f64,
    pub dur: f64,
    pub arc: f64,
}

impl Default for Critter {
    fn default() -> Self {
        Critter {
            t0: 0.0, x: REST[0], y: REST[1], vx: 0.0, vy: 0.0, from_x: REST[0], from_y: REST[1], rot: 0.0, crouch: 0.0, body: 0.0, neck: 0.0,
            eyes: 0.0, near: 0.0, look_x: 0.0, look_y: 0.0, wide: 1.0, squint: 1.0, notch: 0.0, pin_opacity: 0.0, pin_x: PIN_SLOT[0],
            pin_y: PIN_SLOT[1], pin_rot: 0.0, pin_scale: 1.0, ball: 0.0, stretch: 0.0, angle: 0.0, dur: 0.34, arc: 10.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LetterPose {
    pub x: f64,
    pub y: f64,
    pub sx: f64,
    pub sy: f64,
    pub opacity: f64,
}

impl LetterPose {
    const REST: LetterPose = LetterPose { x: 0.0, y: 0.0, sx: 1.0, sy: 1.0, opacity: 1.0 };
}

/// The pointer in logo units, and where the press was.
#[derive(Clone, Copy, Debug)]
pub struct Pointer {
    pub x: f64,
    pub y: f64,
    pub inside: bool,
    pub down_x: f64,
    pub down_y: f64,
}

/// Where the lodger sits in the panel: the stage's top-left in panel points and points per unit.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Stage {
    pub x: f64,
    pub y: f64,
    pub k: f64,
}

/// The eyes' blink: the one playing, and the next one due (`timers.blink`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Blink {
    Open,
    Closing { since: f64 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BlinkTimer {
    Off,
    Due(f64),
}

/// Something the lodger asks of the egg around it as it moves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ask {
    /// A sound, its random numbers drawn (`sound::ask`).
    Sound(super::sound::Sound),
    /// Brick Breaker has gone: the composer takes the keyboard again.
    FocusComposer,
}

pub struct Lodger {
    pub c: Critter,
    pub poses: [LetterPose; 7],
    pub pointer: Pointer,
    /// Squash and stretch on landings: when and how hard (`impacts`).
    pub impacts: Vec<(f64, f64)>,
    pub blink: Blink,
    pub blink_timer: BlinkTimer,
    pub stage: Stage,
    /// The composer, in lodger units, while carried from the perch (`drop`).
    pub drop: Option<[f64; 4]>,
    /// The frame clock and whether a frame is wanted (`timers.raf`, `timers.last`).
    pub now: f64,
    pub frames: FrameLoop,
    pub still: bool,
    /// Brick Breaker may start from a drop on the composer (`play`).
    pub play: bool,
    /// What the last `render` wrote, which the compositor draws.
    pub drawn: render::Drawn,
}

impl Lodger {
    pub fn new(still: bool, stage: Stage) -> Lodger {
        Lodger {
            c: Critter::default(),
            poses: [LetterPose::REST; 7],
            pointer: Pointer { x: HOLE[0], y: HOLE[1], inside: false, down_x: HOLE[0], down_y: HOLE[1] },
            impacts: Vec::with_capacity(8),
            blink: Blink::Open,
            blink_timer: BlinkTimer::Off,
            stage,
            drop: None,
            now: 0.0,
            frames: FrameLoop::Idle,
            still,
            play: true,
            drawn: render::Drawn::default(),
        }
    }

    /// Asks for frames from `now` (`wake`).
    pub fn wake(&mut self, now: f64) {
        self.frames.step(FrameLoopEvent::Wake { now });
    }

    fn reset_poses(&mut self, opacity: f64) {
        self.poses = [LetterPose { opacity, ..LetterPose::REST }; 7];
    }

    /// Panel points to logo units (`toUnits`).
    pub fn to_units(&self, x: f64, y: f64) -> [f64; 2] {
        [(x - self.stage.x) / self.stage.k, (y - self.stage.y) / self.stage.k]
    }

    pub fn to_panel(&self, x: f64, y: f64) -> [f64; 2] {
        [self.stage.x + x * self.stage.k, self.stage.y + y * self.stage.k]
    }

    /// One frame (`frame`). Each step reads the state again: one step can hand over to the next
    /// within the same frame. Returns whether to keep the frames coming.
    pub fn frame(&mut self, now: f64, s: &mut Sender) -> bool {
        let dt = self.frames.step(FrameLoopEvent::BeginFrame { now }).unwrap_or(1.0 / 60.0);
        self.now = now;
        if *s.state == State::Drag {
            self.drag_out(dt, s);
        }
        if *s.state == State::Snap {
            self.snap_in(dt, s);
        }
        if let State::Show { cue, .. } = *s.state {
            self.show(now - self.c.t0, cue, s);
        }
        if let State::Hop { cue, .. } = *s.state {
            self.hop(now - self.c.t0, cue, s);
        }
        if let State::Carry { over, .. } = *s.state {
            self.drag_carry(dt, over, s);
        }
        if matches!(*s.state, State::Return { .. } | State::Game { ball: wordmark::Ball::Returning, .. }) {
            self.return_home(now - self.c.t0, s);
        }
        if let State::Pack { cue, .. } = *s.state {
            self.pack(now - self.c.t0, cue, s);
        }
        self.render();
        // Desktop draws only in these frames: a squash still running when they stop stays
        // drawn as it was until the next one, and so does it here.
        self.frames.step(FrameLoopEvent::EndFrame { now, keep_running: wordmark::animating(*s.state) });
        self.frames.is_running()
    }

    /// Gaze toward the pointer, snapped to 0.08 units (about 0.2 pt), so far-off pointer moves
    /// that wouldn't move the eyes skip the frame (`look`).
    fn look(&mut self) {
        if !self.pointer.inside {
            self.c.look_x = 0.0;
            self.c.look_y = 0.0;
            return;
        }
        let dx = self.pointer.x - self.c.x;
        let dy = self.pointer.y - (self.c.y - 6.0);
        let d = match dx.hypot(dy) { 0.0 => 1.0, d => d };
        let reach = (d / 15.0).min(1.0);
        self.c.look_x = js_round(((dx / d) * reach) / 0.08) * 0.08;
        self.c.look_y = js_round(((dy / d) * reach * 0.6) / 0.08) * 0.08;
    }

    fn impact(&mut self, amp: f64) {
        self.impacts.push((self.now, amp));
        let now = self.now;
        while self.impacts.first().is_some_and(|(at, _)| now - at > 1.0) {
            self.impacts.remove(0);
        }
    }

    fn land(&mut self, amp: f64, s: &mut Sender) {
        self.impact(amp);
        s.sound(super::sound::Cue::Knock { x: self.c.x, width: W }, self.now);
    }

    fn tap(&mut self, amp: f64, s: &mut Sender) {
        if amp != 0.0 {
            self.impact(amp);
        }
        s.sound(super::sound::Cue::Bounce { x: self.c.x, width: W, speed: 3000.0 }, self.now);
    }

    pub fn blink_once(&mut self) {
        if self.still {
            return;
        }
        self.blink = Blink::Closing { since: self.now };
    }

    /// Blinks are only scheduled while the eyes are showing, so the hidden lodger holds no timers.
    fn eyes_visible(&self, state: State) -> bool {
        matches!(state, State::Idle { .. }) || (state == State::Hidden && self.c.eyes > 0.1)
    }

    /// `scheduleBlink`: the next in 2.5 to 6 s, drawing a random number only then.
    pub fn schedule_blink(&mut self, state: State, now: f64, random: &mut super::rng::Rng) {
        if self.blink_timer != BlinkTimer::Off || self.still || !self.eyes_visible(state) {
            return;
        }
        self.blink_timer = BlinkTimer::Due(now + 2.5 + random.next() * 3.5);
    }

    /// The blink timer came due.
    pub fn blink_due(&mut self, now: f64, state: State, random: &mut super::rng::Rng) {
        self.blink_timer = BlinkTimer::Off;
        self.now = now;
        if !self.eyes_visible(state) {
            return;
        }
        self.blink_once();
        self.schedule_blink(state, now, random);
    }

    /// The blink ends once its 160 ms have passed.
    pub fn end_blink(&mut self, now: f64) {
        if let Blink::Closing { since } = self.blink && now - since >= BLINK {
            self.blink = Blink::Open;
        }
    }

    /// The eyes' vertical scale at `now`: 1, eased to 0.1 and back over 160 ms (`blinkOnce`'s
    /// keyframes, `ease-in-out` over the whole).
    pub fn blink_scale_at(&self, now: f64) -> f64 {
        let Blink::Closing { since } = self.blink else { return 1.0 };
        let t = (now - since) / BLINK;
        if t >= 1.0 {
            return 1.0;
        }
        let p = super::ease::css_ease_in_out(t.max(0.0));
        if p < 0.5 { 1.0 + (0.1 - 1.0) * (p / 0.5) } else { 0.1 + (1.0 - 0.1) * ((p - 0.5) / 0.5) }
    }

    // Pointer input, in panel points.

    /// The pointer moved over the panel (`onMove`). Moves only ask for a frame when the lodger
    /// visibly changes; far from the "e" they do nothing.
    pub fn pointer_move(&mut self, x: f64, y: f64, now: f64, state: State) -> bool {
        let [ux, uy] = self.to_units(x, y);
        self.pointer.x = ux;
        self.pointer.y = uy;
        self.pointer.inside = true;
        self.c.near = ((24.0 - (ux - HOLE[0]).hypot(uy - HOLE[1])) / 14.0).clamp(0.0, 1.0);
        if !wordmark::can_interact(state) {
            return false;
        }
        // Whether only the gaze can have changed: the perched lodger's eyes are always open; the
        // hidden one's are when their opacity stays as it was.
        let gaze_only = match state {
            State::Hidden => {
                let eyes = self.c.near * 0.55;
                let same = eyes == self.c.eyes;
                // Far from the "e" (eyes shut) a move does nothing.
                if same && eyes == 0.0 {
                    return false;
                }
                self.c.eyes = eyes;
                // Not Desktop's: it skips every hidden move that leaves the opacity as it is
                // (`wordmark.tsx:1065-1070`), so within 10 units of the hole, where it's full,
                // the open eyes stop following the pointer: on the grip itself. Here they follow.
                same
            }
            State::Idle { .. } => true,
            _ => false,
        };
        let gaze = (self.c.look_x, self.c.look_y);
        self.look();
        if gaze_only && gaze == (self.c.look_x, self.c.look_y) {
            return false;
        }
        // `onMove` starts the blink timer once the eyes show (it draws its delay then).
        self.schedule_blink(state, now, &mut super::rng::Rng::shared());
        self.wake(now);
        true
    }

    pub fn pointer_leave(&mut self, now: f64, state: State) {
        self.pointer.inside = false;
        self.c.near = 0.0;
        if !wordmark::can_interact(state) {
            return;
        }
        if state == State::Hidden {
            self.c.eyes = 0.0;
        }
        self.look();
        self.wake(now);
    }

    /// A press on the grip (`onGrab`), the composer's rect in panel points when there is one.
    /// False when the lodger can't be grabbed now.
    pub fn grab(&mut self, x: f64, y: f64, now: f64, composer: Option<[f64; 4]>, s: &mut Sender) -> bool {
        if !wordmark::can_interact(*s.state) {
            return false;
        }
        if matches!(*s.state, State::Idle { .. }) {
            self.drop = composer.map(|[l, t, r, b]| {
                let [l, t] = self.to_units(l, t);
                let [r, b] = self.to_units(r, b);
                [l, t, r, b]
            });
        }
        let [ux, uy] = self.to_units(x, y);
        self.pointer = Pointer { x: ux, y: uy, inside: true, down_x: ux, down_y: uy };
        s.send(Event::Grab);
        self.wake(now);
        true
    }

    /// The press ended (`onRelease`), or was taken away.
    pub fn release(&mut self, now: f64, s: &mut Sender) {
        let state = *s.state;
        if state != State::Drag && !matches!(state, State::Carry { .. }) {
            return;
        }
        self.now = now;
        let distance = (self.pointer.x - self.pointer.down_x).hypot(self.pointer.y - self.pointer.down_y);
        if state == State::Drag {
            let next = s.send(Event::Release { distance, near_hole: false, reduced_motion: self.still });
            if matches!(next, State::Show { .. } | State::Idle { .. }) {
                return self.pop(next, s);
            }
            self.c.vx = 0.0;
            self.c.vy = 0.0;
            if next == State::Hidden {
                self.rest_hidden();
            }
            self.render();
            return;
        }
        let drop_x = HOME + (self.pointer.x - self.pointer.down_x);
        let drop_y = GROUND + (self.pointer.y - self.pointer.down_y) - 5.0;
        let near_hole = (self.c.x - HOLE[0]).hypot(self.c.y - 5.0 - HOLE[1]) < HOLE_MAGNET
            || (drop_x - HOLE[0]).hypot(drop_y - HOLE[1]) < HOLE_MAGNET
            || (self.pointer.x - HOLE[0]).hypot(self.pointer.y - HOLE[1]) < HOLE_MAGNET;
        let next = s.send(Event::Release { distance, near_hole, reduced_motion: self.still });
        match next {
            State::Idle { .. } => return self.settle_idle(s),
            State::Pack { .. } | State::Hidden => return self.start_pack(next, s),
            State::Game { .. } => {
                self.c.rot = 0.0;
                self.c.crouch = 0.0;
                self.c.wide = 1.0;
                self.c.squint = 1.0;
                self.render();
                return;
            }
            State::Hop { .. } => {
                self.c.x = HOME;
                self.c.y = GROUND;
                self.c.rot = 0.0;
                self.c.t0 = now;
            }
            State::Return { .. } => {
                self.c.from_x = self.c.x;
                self.c.from_y = self.c.y;
                self.c.t0 = now;
                self.c.dur = 0.34;
                self.c.arc = 10.0;
            }
            _ => {}
        }
        self.wake(now);
    }

    // The ball Brick Breaker plays with (`LodgerBall`), in panel points.

    pub fn ball_radius(&self) -> f64 {
        5.0 * self.stage.k
    }

    pub fn ball_at(&self) -> [f64; 2] {
        self.to_panel(self.c.x, self.c.y - 5.0)
    }

    pub fn ball_place(&mut self, x: f64, y: f64, pose: &BallPose, now: f64) {
        self.now = now;
        self.c.x = (x - self.stage.x) / self.stage.k;
        self.c.y = (y - self.stage.y) / self.stage.k + 5.0;
        self.c.look_x = pose.look_angle.cos() * pose.look;
        self.c.look_y = pose.look_angle.sin() * pose.look * 0.6;
        self.c.stretch = pose.stretch;
        self.c.angle = pose.angle;
        self.c.squint = pose.squint;
        self.c.ball = pose.curl;
        self.render();
    }

    /// Uncurls and hops back to its perch beside the pin board.
    pub fn ball_done(&mut self, now: f64, s: &mut Sender) {
        self.c.from_x = self.c.x;
        self.c.from_y = self.c.y;
        self.c.t0 = now;
        self.c.stretch = 0.0;
        self.c.squint = 1.0;
        self.c.dur = 0.7;
        self.c.arc = 36.0;
        s.send(Event::BallDone);
        self.wake(now);
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BallPose {
    pub look: f64,
    pub look_angle: f64,
    pub stretch: f64,
    pub angle: f64,
    pub squint: f64,
    pub curl: f64,
}

/// Sends events to the wordmark machine for the lodger and Brick Breaker, and collects what they
/// ask of the egg. The egg mounts or drops the pin board as the state asks once the call returns,
/// in the same frame, where Desktop's view does it inside the dispatch: nothing the lodger reads
/// in between depends on it.
pub struct Sender<'a> {
    pub state: &'a mut State,
    pub asks: &'a mut Vec<Ask>,
}

impl Sender<'_> {
    pub fn send(&mut self, event: Event) -> State {
        for effect in wordmark::step(self.state, event) {
            match effect {
                wordmark::Effect::FocusComposer => self.asks.push(Ask::FocusComposer),
            }
        }
        *self.state
    }

    pub fn ask(&mut self, a: Ask) {
        self.asks.push(a);
    }

    /// Desktop's `PinSound` call at `now`: it draws its random numbers there and then.
    pub fn sound(&mut self, cue: super::sound::Cue, now: f64) {
        if let Some(s) = super::sound::ask(cue, now) {
            self.asks.push(Ask::Sound(s));
        }
    }
}

pub fn js_round(v: f64) -> f64 {
    (v + 0.5).floor()
}

pub fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

pub fn clamp(v: f64, limit: f64) -> f64 {
    v.clamp(-limit, limit)
}

pub fn clamp01(v: f64) -> f64 {
    v.clamp(0.0, 1.0)
}

pub fn seg(t: f64, from: f64, to: f64) -> f64 {
    clamp01((t - from) / (to - from))
}

pub fn ease_out(t: f64) -> f64 {
    1.0 - (1.0 - t).powi(3)
}

pub fn ease_in_out(t: f64) -> f64 {
    if t < 0.5 { 4.0 * t * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(3) / 2.0 }
}
