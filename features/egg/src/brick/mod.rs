//! Brick Breaker (Desktop's `new-session/brick-breaker.tsx` `mountGame`): the lodger curls into
//! the ball, the pins are the bricks, and the composer shrinks into the paddle. Like Arkanoid, the
//! ball waits on the paddle until it is served with a click or Space. The physics steps on the
//! frame's own time, clamped to 1/240-1/30 s, with substeps short enough that the ball can't
//! tunnel through a pin, as Desktop's does: a 240 Hz display plays at the same speed as 60 Hz.

pub mod machine;

use machine::{Event, Exit, State, timing};

use super::ease::Tween;
use super::lodger::{BallPose, Lodger, Sender};
use super::pins::Pins;
use super::rng::Rng;
use super::sound::Cue;

/// A box in panel points with its corner radius.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Box {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub r: f64,
}

#[derive(Clone, Copy, Debug, Default)]
struct Rally {
    combo: u32,
    hits: u32,
    base: f64,
    speed: f64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Paddle {
    pub x: f64,
    pub target: f64,
    v: f64,
    pub w: f64,
    pub h: f64,
    pub y: f64,
    pub tilt: f64,
    kick_at: f64,
}

#[derive(Clone, Copy, Debug, Default)]
struct BallState {
    x: f64,
    y: f64,
    vx: f64,
    vy: f64,
    hit_at: f64,
    hit_angle: f64,
    hit_amp: f64,
    squint_until: f64,
    curl: f64,
}

/// What the game asks of the page around it, besides drawing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ask {
    /// A sound, its random numbers drawn (`sound::ask`).
    Sound(super::sound::Sound),
    /// The composer and the tip fade out for the game (true), or back in at its end.
    Composer(bool),
    /// The pointer hides over the panel while the paddle is in play.
    Cursor(bool),
    /// The game has finished its exit: unmount (`onDone`).
    Done,
}

pub struct Game {
    pub state: State,
    /// The panel's size and the arena's walls, in panel points.
    pub w: f64,
    pub h: f64,
    pub l: f64,
    pub r: f64,
    last: f64,
    pub score: u32,
    rally: Rally,
    pub pad: Paddle,
    b: BallState,
    keys: [bool; 2],
    start_ball: [f64; 2],
    start_composer: Box,
    start_from: Box,
    composer: Box,
    /// The paddle as drawn (rounded as Desktop writes it to the DOM) and its tilt in degrees.
    pub drawn: Box,
    pub drawn_tilt: f64,
    /// The paddle's fill moving from the composer's surface to `icon-muted` and back (0 to 1),
    /// and its opacity.
    pub paint: Tween,
    pub opacity: Tween,
    /// The HUD is showing (it fades on its own 300 ms transition).
    pub hud: bool,
    pub spares: u8,
    /// The last serve's random draw, for a replay to repeat it.
    pub serve: f64,
}

impl Game {
    /// Mounts the game at `now` (`mountGame`): the ball where the lodger is, the paddle over the
    /// composer. `logo` is the wordmark box, in panel points.
    pub fn new(now: f64, w: f64, h: f64, logo: Option<[f64; 4]>, composer: Box, ball: [f64; 2]) -> Game {
        let mut g = Game {
            state: State::new(now),
            w, h, l: 0.0, r: 0.0,
            last: now - 1.0 / 60.0,
            score: 0,
            rally: Rally::default(),
            pad: Paddle { h: 12.0, kick_at: -1.0, ..Paddle::default() },
            b: BallState { x: ball[0], y: ball[1], hit_at: -1.0, curl: 0.85, ..BallState::default() },
            keys: [false; 2],
            start_ball: ball,
            start_composer: composer,
            start_from: composer,
            composer,
            drawn: composer,
            drawn_tilt: 0.0,
            paint: Tween::At(0.0),
            opacity: Tween::At(1.0),
            hud: false,
            spares: machine::LIVES - 1,
            serve: 0.0,
        };
        g.layout(w, h, logo, composer);
        g.pad.x = composer.x + composer.w / 2.0;
        g.pad.target = g.pad.x;
        g.draw(composer, 0.0);
        g
    }

    /// The arena is exactly as wide as the wordmark, like Arkanoid's tall, narrow playfield. Walls
    /// sit on the wordmark's outer pin edges, at the board's whole-point pitch and rounded origin,
    /// so the ball can only reach the top by breaking through.
    pub fn layout(&mut self, w: f64, h: f64, logo: Option<[f64; 4]>, composer: Box) {
        self.w = w;
        self.h = h;
        self.composer = composer;
        match logo {
            Some([x, _, lw, _]) => {
                let pitch = (lw / 68.0).round().max(3.0);
                let x0 = (x + (lw - pitch * 68.0) / 2.0).round();
                self.l = x0;
                self.r = x0 + pitch * 68.0;
            }
            None => {
                self.l = composer.x.max(0.0);
                self.r = w.min(composer.x + composer.w);
            }
        }
        self.pad.w = ((self.r - self.l) * 0.15).clamp(84.0, 136.0);
        self.pad.y = h - 64.0;
        self.rally.base = (h * 0.7).clamp(460.0, 760.0);
        self.rally.speed = self.rally.base * (1.0 + (f64::from(self.rally.hits) * 0.025).min(0.55));
    }

    fn paddle_box(&self) -> Box {
        Box { x: self.pad.x - self.pad.w / 2.0, y: self.pad.y, w: self.pad.w, h: self.pad.h, r: self.pad.h / 2.0 }
    }

    /// One frame. Crosses any phase threshold first, then animates the one phase the frame is
    /// in: a phase entered while animating (a miss, the last brick) starts animating next frame.
    pub fn frame(&mut self, now: f64, pins: &mut Pins, lodger: &mut Lodger, s: &mut Sender, rng: &mut Rng, asks: &mut Vec<Ask>) {
        let dt = (now - self.last).clamp(1.0 / 240.0, 1.0 / 30.0);
        self.last = now;
        self.dispatch(Event::Frame { now }, now, pins, lodger, s, rng, asks);
        if self.state == State::Disposed {
            asks.push(Ask::Done);
            return;
        }
        self.move_paddle(dt, now);
        match self.state {
            State::Enter { started_at, .. } => self.enter(now - started_at, lodger),
            State::Serve { started_at, landed, .. } => self.serve(if landed { 1.0 } else { seg(now - started_at, 0.0, timing::SERVE_DROP) }, lodger),
            State::Play { .. } => self.play(dt, now, pins, lodger, s, rng, asks),
            State::Exit { started_at, .. } => self.exit(now - started_at, now, asks),
            State::Miss { .. } | State::Disposed => {}
        }
        if !matches!(self.state, State::Exit { .. } | State::Disposed) {
            self.pose(now, lodger);
            pins.steer(self.b.x, self.b.y);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn dispatch(&mut self, event: Event, now: f64, pins: &mut Pins, lodger: &mut Lodger, s: &mut Sender, rng: &mut Rng, asks: &mut Vec<Ask>) {
        let prev = self.state;
        self.state = machine::step(prev, event);
        if self.state != prev {
            self.changed(prev, now, pins, lodger, s, rng, asks);
        }
    }

    /// What happens once as the game moves from one phase, or landing, to the next.
    #[allow(clippy::too_many_arguments)]
    fn changed(&mut self, prev: State, now: f64, pins: &mut Pins, lodger: &mut Lodger, s: &mut Sender, rng: &mut Rng, asks: &mut Vec<Ask>) {
        let next = self.state;
        if next == State::Disposed {
            return;
        }
        let was_landed = matches!(prev, State::Enter { landed: false, .. } | State::Serve { landed: false, .. });
        let is_landed = matches!(next, State::Enter { landed: true, .. } | State::Serve { landed: true, .. });
        if was_landed && is_landed {
            self.bump(std::f64::consts::FRAC_PI_2, if matches!(prev, State::Enter { .. }) { 0.32 } else { 0.36 }, now);
            asks.extend(super::sound::ask(Cue::Paddle { x: self.b.x, width: self.w }, now).map(Ask::Sound));
        }
        if matches!(prev, State::Enter { .. }) && matches!(next, State::Serve { .. }) {
            self.pad.target = self.pad.x;
            asks.push(Ask::Cursor(true));
        }
        if matches!(prev, State::Serve { .. }) && matches!(next, State::Play { .. }) {
            self.launch(now, rng, asks);
        }
        if matches!(prev, State::Play { .. }) && matches!(next, State::Miss { .. }) {
            asks.extend(super::sound::ask(Cue::Sweep { duration: 0.45, reverse: true }, now).map(Ask::Sound));
        }
        if matches!(prev, State::Miss { .. }) {
            self.spares = machine::spares(next);
        }
        if let State::Exit { kind, .. } = next {
            self.finish(kind, now, pins, lodger, s, rng, asks);
        }
    }

    fn move_paddle(&mut self, dt: f64, now: f64) {
        if !machine::moves_paddle(self.state) {
            return;
        }
        if self.keys[0] != self.keys[1] {
            self.pad.target += if self.keys[1] { 1.0 } else { -1.0 } * 1100.0 * dt;
        }
        self.pad.target = self.pad.target.min(self.r - self.pad.w / 2.0 - 4.0).max(self.l + self.pad.w / 2.0 + 4.0);
        let prev = self.pad.x;
        self.pad.x += (self.pad.target - self.pad.x) * (1.0 - (-dt * 30.0).exp());
        self.pad.v = (self.pad.x - prev) / dt;
        self.pad.tilt += ((self.pad.v / 260.0).clamp(-5.0, 5.0) - self.pad.tilt) * (1.0 - (-dt * 14.0).exp());
        let k = now - self.pad.kick_at;
        let kick = if k < 0.5 { (-k * 12.0).exp() * (k * 34.0).sin() * 4.0 } else { 0.0 };
        let b = Box { y: self.pad.y + kick, ..self.paddle_box() };
        self.draw(b, self.pad.tilt);
    }

    /// The composer's content fades, then its box springs down and narrows into the paddle while
    /// the ball rides on top.
    fn enter(&mut self, t: f64, lodger: &Lodger) {
        let u = seg(t, 0.1, timing::ENTER);
        let e = spring(u);
        let to = self.paddle_box();
        let from = self.start_from;
        let bx = lerp(from.x + from.w / 2.0, to.x + to.w / 2.0, e);
        let by = lerp(from.y, to.y, e);
        let bw = lerp(from.w, to.w, e).max(self.pad.h);
        let bh = lerp(from.h, to.h, e).max(4.0);
        let br = lerp(from.r, to.r, (u * 1.6).min(1.0));
        self.draw(Box { x: bx - bw / 2.0, y: by, w: bw, h: bh, r: br }, 0.0);
        let k = ease_out(seg(t, 0.0, timing::ENTER_LAND));
        let r = lodger.ball_radius();
        self.b.x = lerp(self.start_ball[0], bx, k);
        self.b.y = lerp(self.start_ball[1], by - r, k) - (std::f64::consts::PI * k).sin() * 28.0;
        self.b.curl = lerp(0.85, 1.0, k);
        // Once it has shrunk enough to read as a paddle, it takes on the colour of the pins it is
        // about to break.
        if t >= 0.3 {
            self.paint.set(1.0, self.last, 0.36, super::ease::EASE_OUT);
        }
        if t >= 0.5 {
            self.hud = true;
        }
    }

    /// The ball waits on the paddle like Arkanoid's; after a miss it drops back onto it first.
    fn serve(&mut self, drop: f64, lodger: &Lodger) {
        let r = lodger.ball_radius();
        self.b.x = self.pad.x;
        self.b.y = self.pad.y - r - (1.0 - drop).powi(2) * 110.0;
        self.b.vx = 0.0;
        self.b.vy = 0.0;
        self.b.curl = 1.0;
    }

    fn launch(&mut self, now: f64, rng: &mut Rng, asks: &mut Vec<Ask>) {
        // Serve away from the side the paddle is moving toward, up and to the right when still.
        let lean = if self.pad.v.abs() > 40.0 { self.pad.v.signum() } else { 1.0 };
        self.serve = rng.next();
        let a = lean * (0.32 + self.serve * 0.12);
        self.b.vx = a.sin() * self.rally.speed;
        self.b.vy = -a.cos() * self.rally.speed;
        self.rally.combo = 0;
        self.pad.kick_at = now;
        self.bump(std::f64::consts::FRAC_PI_2, 0.3, now);
        asks.extend(super::sound::ask(Cue::Paddle { x: self.b.x, width: self.w }, now).map(Ask::Sound));
    }

    #[allow(clippy::too_many_arguments)]
    fn play(&mut self, dt: f64, now: f64, pins: &mut Pins, lodger: &mut Lodger, s: &mut Sender, rng: &mut Rng, asks: &mut Vec<Ask>) {
        let r = lodger.ball_radius();
        // Substeps keep the ball from tunnelling through a pin between frames at high speed.
        let steps = ((self.rally.speed * dt) / (r * 0.35)).ceil().max(1.0) as usize;
        let sdt = dt / steps as f64;
        for _ in 0..steps {
            let b = &mut self.b;
            b.x += b.vx * sdt;
            b.y += b.vy * sdt;
            if b.x < self.l + r || b.x > self.r - r {
                b.x = b.x.min(self.r - r).max(self.l + r);
                b.vx = if b.x <= self.l + r { b.vx.abs() } else { -b.vx.abs() };
                let x = b.x;
                self.bump(0.0, 0.22, now);
                asks.extend(super::sound::ask(Cue::Bounce { x, width: self.w, speed: 4000.0 }, now).map(Ask::Sound));
            }
            let b = &mut self.b;
            if b.y < r {
                b.y = r;
                b.vy = b.vy.abs();
                let x = b.x;
                self.bump(std::f64::consts::FRAC_PI_2, 0.22, now);
                asks.extend(super::sound::ask(Cue::Bounce { x, width: self.w, speed: 4000.0 }, now).map(Ask::Sound));
            }
            let (pad, b) = (self.pad, self.b);
            if b.vy > 0.0 && b.y + r >= pad.y && b.y - r < pad.y + pad.h && (b.x - pad.x).abs() <= pad.w / 2.0 + r * 0.6 {
                // Classic paddle control: where the ball lands sets its angle, up to 60 degrees from vertical.
                let off = ((b.x - pad.x) / (pad.w / 2.0)).clamp(-1.0, 1.0);
                let a = off * (std::f64::consts::PI / 3.0) + (pad.v / 6000.0).clamp(-0.15, 0.15);
                self.rally.hits += 1;
                self.rally.speed = self.rally.base * (1.0 + (f64::from(self.rally.hits) * 0.025).min(0.55));
                self.rally.combo = 0;
                self.b.vx = a.sin() * self.rally.speed;
                self.b.vy = -a.cos() * self.rally.speed;
                self.b.y = pad.y - r;
                self.pad.kick_at = now;
                self.bump(std::f64::consts::FRAC_PI_2, 0.34, now);
                asks.extend(super::sound::ask(Cue::Paddle { x: self.b.x, width: self.w }, now).map(Ask::Sound));
            }
            // Each hit carves a round bite out of the wordmark rather than a single pin.
            let b = self.b;
            if let Some(hit) = pins.collide(b.x, b.y, r, r * 0.55, b.vx, b.vy, now, rng) {
                let side = hit.nx.abs() > hit.ny.abs();
                if side && b.vx * hit.nx < 0.0 {
                    self.b.vx = -self.b.vx;
                }
                if !side && b.vy * hit.ny < 0.0 {
                    self.b.vy = -self.b.vy;
                }
                self.rally.combo += 1;
                self.score += hit.count as u32 * 10 * self.rally.combo.min(8);
                self.b.squint_until = now + 0.14;
                self.bump(if side { 0.0 } else { std::f64::consts::FRAC_PI_2 }, 0.26, now);
                asks.extend(super::sound::ask(Cue::Brick { x: self.b.x, width: self.w, combo: self.rally.combo }, now).map(Ask::Sound));
                if pins.left() == 0 {
                    return self.dispatch(Event::Cleared { now }, now, pins, lodger, s, rng, asks);
                }
            }
            // A nearly flat rally never comes back down, so keep at least a quarter of the speed vertical.
            let min = self.rally.speed * 0.25;
            let b = &mut self.b;
            if b.vy.abs() < min {
                b.vy = if b.vy < 0.0 { -1.0 } else { 1.0 } * min;
                b.vx = js_sign(if b.vx == 0.0 { 1.0 } else { b.vx }) * (self.rally.speed * self.rally.speed - min * min).sqrt();
            }
            if self.b.y - r > self.h {
                return self.dispatch(Event::Missed { now }, now, pins, lodger, s, rng, asks);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn finish(&mut self, kind: Exit, now: f64, pins: &mut Pins, lodger: &mut Lodger, s: &mut Sender, rng: &mut Rng, asks: &mut Vec<Ask>) {
        self.start_composer = self.composer;
        self.start_from = self.drawn;
        asks.push(Ask::Cursor(false));
        self.hud = false;
        self.paint.set(0.0, now, 0.36, super::ease::EASE_OUT);
        pins.restore(now, rng);
        lodger.ball_done(now, s);
        asks.extend(super::sound::ask(Cue::Sweep { duration: 0.9, reverse: kind != Exit::Win }, now).map(Ask::Sound));
    }

    /// The paddle grows back into the composer, then the composer's content fades in over it.
    fn exit(&mut self, t: f64, now: f64, asks: &mut Vec<Ask>) {
        let e = ease_in_out(seg(t, 0.15, 0.75));
        let (from, to) = (self.start_from, self.start_composer);
        self.draw(Box { x: lerp(from.x, to.x, e), y: lerp(from.y, to.y, e), w: lerp(from.w, to.w, e), h: lerp(from.h, to.h, e), r: lerp(from.r, to.r, e) }, self.pad.tilt * (1.0 - e));
        if t < 0.75 {
            return;
        }
        if self.opacity.target() != 0.0 {
            asks.push(Ask::Composer(false));
            self.opacity.set(0.0, now, 0.18, super::ease::EASE_OUT);
        }
    }

    fn bump(&mut self, angle: f64, amp: f64, now: f64) {
        self.b.hit_at = now;
        self.b.hit_angle = angle;
        self.b.hit_amp = amp;
    }

    fn pose(&self, now: f64, lodger: &mut Lodger) {
        let b = &self.b;
        let t = now - b.hit_at;
        let jiggle = if t < 0.45 { b.hit_amp * (-t * 12.0).exp() * (t * 38.0).cos() } else { 0.0 };
        let moving = machine::ball_moving(self.state);
        let heading = b.vy.atan2(b.vx);
        let squash = jiggle.abs() > 0.015;
        lodger.ball_place(b.x, b.y, &BallPose {
            look: if moving { 1.0 } else { 0.8 },
            look_angle: if moving { heading } else { -std::f64::consts::FRAC_PI_2 },
            stretch: if squash { -jiggle } else if moving { 0.07 } else { 0.0 },
            angle: (if squash { b.hit_angle } else { heading }).to_degrees(),
            squint: if now < b.squint_until { 0.45 } else { 1.0 },
            curl: b.curl,
        }, now);
    }

    /// The paddle's box as Desktop writes it: size to a tenth of a point, position to a tenth,
    /// tilt to a hundredth of a degree.
    fn draw(&mut self, b: Box, tilt: f64) {
        self.drawn = Box { x: (b.x * 10.0).round() / 10.0, y: (b.y * 10.0).round() / 10.0, w: (b.w * 10.0).round() / 10.0, h: (b.h * 10.0).round() / 10.0, r: b.r };
        self.drawn_tilt = (tilt * 100.0).round() / 100.0;
    }

    /// The score as the HUD shows it, five digits.
    pub fn score_text(&self) -> String {
        format!("{:05}", self.score)
    }

    // Input.

    pub fn pointer_move(&mut self, x: f64) {
        self.pad.target = x;
    }

    /// A left press anywhere on the panel serves.
    #[allow(clippy::too_many_arguments)]
    pub fn press(&mut self, now: f64, pins: &mut Pins, lodger: &mut Lodger, s: &mut Sender, rng: &mut Rng, asks: &mut Vec<Ask>) -> bool {
        if !machine::accepts_input(self.state) {
            return false;
        }
        self.dispatch(Event::Launch, now, pins, lodger, s, rng, asks);
        true
    }

    /// A key, down or up: arrows and A/D move, Space and Up serve, Escape quits. False when the
    /// game doesn't take it.
    #[allow(clippy::too_many_arguments)]
    pub fn key(&mut self, key: GameKey, down: bool, now: f64, pins: &mut Pins, lodger: &mut Lodger, s: &mut Sender, rng: &mut Rng, asks: &mut Vec<Ask>) -> bool {
        if !machine::accepts_input(self.state) {
            return false;
        }
        match key {
            GameKey::Escape if down => self.dispatch(Event::Quit { now }, now, pins, lodger, s, rng, asks),
            GameKey::Left => self.keys[0] = down,
            GameKey::Right => self.keys[1] = down,
            GameKey::Serve if down => self.dispatch(Event::Launch, now, pins, lodger, s, rng, asks),
            _ => return false,
        }
        true
    }

    /// The window lost focus: held keys let go.
    pub fn blur(&mut self) {
        self.keys = [false; 2];
    }

    /// The game unmounts.
    pub fn dispose(&mut self) {
        self.state = machine::step(self.state, Event::Dispose);
    }

    pub fn wants_cursor_hidden(&self) -> bool {
        machine::moves_paddle(self.state)
    }

    pub fn ball(&self) -> [f64; 2] {
        [self.b.x, self.b.y]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameKey {
    Left,
    Right,
    Serve,
    Escape,
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

fn seg(t: f64, from: f64, to: f64) -> f64 {
    ((t - from) / (to - from)).clamp(0.0, 1.0)
}

fn ease_out(t: f64) -> f64 {
    1.0 - (1.0 - t).powi(3)
}

fn ease_in_out(t: f64) -> f64 {
    if t < 0.5 { 4.0 * t * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(3) / 2.0 }
}

/// An underdamped spring that settles by t = 1, overshooting about 9% on the way.
fn spring(t: f64) -> f64 {
    if t >= 1.0 { 1.0 } else { 1.0 - (-7.0 * t).exp() * (9.0 * t).cos() }
}

/// `Math.sign`.
fn js_sign(v: f64) -> f64 {
    if v > 0.0 { 1.0 } else if v < 0.0 { -1.0 } else { v }
}
