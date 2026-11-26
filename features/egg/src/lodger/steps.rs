//! The lodger's timed moves, frame by frame (`wordmark.tsx` `dragOut` through `hop`).

use super::super::sound::Cue;
use super::super::wordmark::{Event, Phase, State};
use super::*;

const SLOT_SHOW: [f64; 5] = [0.45, 0.95, 1.35, 1.72, 1.95];
const SLOT_PACK: [f64; 2] = [0.95, 1.14];

use super::super::ease::due;

/// Runs each cue whose time has come, from the first not yet run, and returns how many have run.
fn run_cues(at: &[f64], from: u8, t: f64, mut run: impl FnMut(usize)) -> u8 {
    let mut cue = usize::from(from);
    while cue < at.len() && due(t, at[cue]) {
        run(cue);
        cue += 1;
    }
    cue as u8
}

impl Lodger {
    pub(super) fn drag_out(&mut self, dt: f64, s: &mut Sender) {
        let dx = self.pointer.x - self.pointer.down_x;
        let dy = self.pointer.y - self.pointer.down_y;
        let pull = dx.hypot(dy);
        if pull > SLIP {
            let next = s.send(Event::DragSlip { reduced_motion: self.still });
            return self.pop(next, s);
        }
        let c = &mut self.c;
        let give = 1.0 / (1.0 + pull / 40.0);
        let ease = 1.0 - (-dt * 30.0).exp();
        c.x += (REST[0] + dx * give - c.x) * ease;
        c.y += (REST[1] + dy * give - c.y) * ease;
        let out = (c.x - REST[0]).hypot(c.y - REST[1]);
        let stress = clamp01(pull / wordmark::LODGER_POP);
        c.body = clamp01(out / 4.5);
        c.neck = c.body;
        c.eyes = 1.0;
        c.wide = 1.0 + stress * 0.32;
        c.rot = clamp((c.x - REST[0]) * 2.0, 24.0);
        c.crouch = -clamp01(out / 20.0) * 0.18;
        c.look_x = clamp(dx / 15.0, 1.0);
        c.look_y = clamp(dy / 15.0, 1.0) * 0.6;
        c.notch = clamp01(out / 3.0);
        c.pin_opacity = c.notch;
        c.pin_x = lerp(PIN_SLOT[0], c.x - 1.5, clamp01(out / 7.0));
        c.pin_y = lerp(PIN_SLOT[1], c.y - 11.0, clamp01(out / 7.0));
        c.pin_rot = (c.x - REST[0]) * 2.5;
        c.pin_scale = 1.0 + stress * 0.35;
        let (x, y) = (c.x, c.y);
        let p = &mut self.poses[6];
        p.x = (x - REST[0]) * 0.12;
        p.y = (y - REST[1]) * 0.12;
        p.sx = 1.0 + stress * 0.05;
        p.sy = 1.0 + stress * 0.05;
    }

    pub(super) fn snap_in(&mut self, dt: f64, s: &mut Sender) {
        let c = &mut self.c;
        c.vx += (-(c.x - REST[0]) * 420.0 - c.vx * 18.0) * dt;
        c.vy += (-(c.y - REST[1]) * 420.0 - c.vy * 18.0) * dt;
        c.x += c.vx * dt;
        c.y += c.vy * dt;
        let out = (c.x - REST[0]).hypot(c.y - REST[1]);
        let fade = (-dt * 10.0).exp();
        c.body = clamp01(out / 4.5);
        c.neck = c.body;
        c.notch *= fade;
        c.pin_opacity *= fade;
        c.rot *= fade;
        c.crouch *= fade;
        c.wide = 1.0 + (c.wide - 1.0) * fade;
        for p in &mut self.poses {
            p.x *= fade;
            p.y *= fade;
            p.sx = 1.0 + (p.sx - 1.0) * fade;
            p.sy = 1.0 + (p.sy - 1.0) * fade;
        }
        if out > 0.05 || self.c.vx.hypot(self.c.vy) > 0.5 {
            return;
        }
        s.send(Event::Settled { phase: Phase::Snap });
        self.rest_hidden();
        s.sound(Cue::Slot { column: 60.0, columns: 68.0, x: self.c.x, width: W }, self.now);
        self.blink_once();
    }

    pub(super) fn rest_hidden(&mut self) {
        self.reset_poses(1.0);
        let eyes = self.c.near * 0.55;
        self.c = Critter { x: REST[0], y: REST[1], vx: 0.0, vy: 0.0, rot: 0.0, crouch: 0.0, wide: 1.0, squint: 1.0, body: 0.0, neck: 0.0, notch: 0.0, pin_opacity: 0.0, ball: 0.0, stretch: 0.0, eyes, ..self.c };
        self.look();
    }

    /// The machine has popped the pin board out: `Show`, or `Idle` with reduced motion.
    pub(super) fn pop(&mut self, state: State, s: &mut Sender) {
        self.c.t0 = self.now;
        self.c.from_x = self.c.x;
        self.c.from_y = self.c.y;
        self.c.neck = 0.0;
        s.sound(Cue::Knock { x: self.c.x, width: W }, self.now);
        if matches!(state, State::Idle { .. }) {
            return self.settle_idle(s);
        }
        let now = self.now;
        self.wake(now);
    }

    /// Pulling one pin out of the "e" sets off an elastic 3D pull-wave that pops every pin out
    /// toward the viewer from right to left; then the lodger hops up to slot its starter pin back in.
    pub(super) fn show(&mut self, t: f64, cue: u8, s: &mut Sender) {
        let mut land = [false; 5];
        let reached = run_cues(&SLOT_SHOW, cue, t, |i| land[i] = true);
        if land[0] {
            self.land(0.38, s);
        }
        if land[2] {
            s.sound(Cue::Slot { column: 65.0, columns: 68.0, x: PIN_SLOT[0], width: W }, self.now);
        }
        if land[3] {
            self.land(0.32, s);
        }
        if land[4] {
            self.blink_once();
        }
        if reached != cue {
            s.send(Event::CueStep { phase: Phase::Show, cue: reached });
        }
        if due(t, 2.15) {
            s.send(Event::Settled { phase: Phase::Show });
            return self.settle_idle(s);
        }
        let c = &mut self.c;
        c.rot = 0.0;
        c.crouch = 0.0;
        c.wide = 1.0;
        c.squint = 1.0;
        c.look_x = -1.0;
        c.look_y = 0.0;
        c.body = 1.0;
        c.eyes = 1.0;
        c.notch = if t < 1.35 { 1.0 } else { 0.0 };
        c.pin_opacity = if t < 1.35 { 1.0 } else { 0.0 };
        for (i, p) in self.poses.iter_mut().enumerate() {
            let start = (7 - i) as f64 * 0.08;
            p.opacity = 1.0 - seg(t, start, start + 0.28);
            p.x = 0.0;
            p.y = 0.0;
        }
        let pi = std::f64::consts::PI;
        if t < 0.45 {
            let u = t / 0.45;
            c.x = lerp(c.from_x, HOME, ease_out(u));
            c.y = lerp(c.from_y, GROUND, u) - (pi * u).sin() * 20.0;
            c.rot = 360.0 * ease_in_out(u);
            c.wide = 1.35;
            c.pin_x = c.x - 1.0;
            c.pin_y = c.y - 13.0;
            c.pin_rot = u * 360.0;
            c.pin_scale = 1.4;
            return;
        }
        if t < 1.05 {
            c.x = HOME;
            c.y = GROUND;
            c.look_x = -1.0;
            c.pin_x = c.x;
            c.pin_y = c.y - 12.5 - (t * 8.0).sin() * 0.8;
            c.pin_rot = (t * 5.0).sin() * 10.0;
            c.pin_scale = 1.3;
            return;
        }
        if t < 1.35 {
            let u = seg(t, 1.05, 1.35);
            c.x = lerp(HOME, 233.0, ease_in_out(u));
            c.y = lerp(GROUND, 18.0, ease_out(u)) - (pi * u).sin() * 7.0;
            c.pin_x = lerp(HOME, PIN_SLOT[0], ease_in_out(u));
            c.pin_y = lerp(GROUND - 12.5, PIN_SLOT[1], ease_in_out(u));
            c.pin_rot = 0.0;
            c.pin_scale = lerp(1.3, 1.0, u);
            return;
        }
        let u = seg(t, 1.35, 1.72);
        c.x = lerp(233.0, HOME, ease_in_out(u));
        c.y = lerp(18.0, GROUND, u) - (pi * u).sin() * 10.0;
        c.squint = if t > 1.72 { 0.35 } else { 1.0 };
    }

    pub(super) fn settle_idle(&mut self, _s: &mut Sender) {
        self.reset_poses(0.0);
        self.c = Critter { x: HOME, y: GROUND, rot: 0.0, crouch: 0.0, wide: 1.0, squint: 1.0, body: 1.0, eyes: 1.0, neck: 0.0, notch: 0.0, pin_opacity: 0.0, ball: 0.0, stretch: 0.0, ..self.c };
        self.look();
        self.render();
    }

    /// Carrying the lodger from its perch: it dangles under the pointer and looks toward the "e" hole.
    pub(super) fn drag_carry(&mut self, dt: f64, was_over: bool, s: &mut Sender) {
        let dx = self.pointer.x - self.pointer.down_x;
        let dy = self.pointer.y - self.pointer.down_y;
        let raw_x = HOME + dx;
        let raw_y = GROUND + dy;
        let dist = (raw_x - HOLE[0]).hypot(raw_y - 5.0 - HOLE[1]);
        if dist < 9.0 {
            let next = s.send(Event::CarryFrame { in_hole: true, over: false, reduced_motion: self.still });
            return self.start_pack(next, s);
        }
        let now = self.now;
        let c = &mut self.c;
        let magnet = clamp01((HOLE_MAGNET - dist) / HOLE_MAGNET) * 0.45;
        let target_x = lerp(raw_x, REST[0], magnet);
        let target_y = lerp(raw_y, REST[1], magnet);
        let ease = 1.0 - (-dt * 28.0).exp();
        let vx = target_x - c.x;
        c.x += vx * ease;
        c.y += (target_y - c.y) * ease;
        c.rot = clamp(vx * 3.5 + (now * 22.0).sin() * 5.0, 26.0);
        c.crouch = -0.12 + magnet * 0.2;
        c.wide = 1.2;
        c.squint = 1.0 - magnet * 0.3;
        let hx = HOLE[0] - c.x;
        let hy = HOLE[1] - (c.y - 5.0);
        let hd = match hx.hypot(hy) { 0.0 => 1.0, d => d };
        c.look_x = (hx / hd) * (hd / 10.0).min(1.0);
        c.look_y = (hy / hd) * (hd / 10.0).min(1.0) * 0.6;
        c.pin_opacity = magnet;
        c.pin_x = PIN_SLOT[0];
        c.pin_y = PIN_SLOT[1];
        c.pin_rot = 0.0;
        c.pin_scale = 1.0 + magnet * 0.25;
        // Over the composer it curls up and peers down at it, and the composer sinks a little under it.
        let (px, py) = (self.pointer.x, self.pointer.y);
        let over = self.play && self.drop.is_some_and(|[l, t, r, b]| px > l && px < r && py > t && py < b);
        if over != was_over {
            s.send(Event::CarryFrame { in_hole: false, over, reduced_motion: self.still });
        }
        let c = &mut self.c;
        c.ball += ((if over { 0.85 } else { 0.0 }) - c.ball) * (1.0 - (-dt * 14.0).exp());
        if !over {
            return;
        }
        c.look_x = 0.0;
        c.look_y = 0.6;
        c.squint = 0.8;
        c.rot *= 0.4;
    }

    pub(super) fn return_home(&mut self, t: f64, s: &mut Sender) {
        let c = &mut self.c;
        let u = seg(t, 0.0, c.dur);
        c.x = lerp(c.from_x, HOME, ease_out(u));
        c.y = lerp(c.from_y, GROUND, u) - (std::f64::consts::PI * u).sin() * c.arc;
        c.rot = lerp(c.rot, 0.0, u);
        c.crouch = 0.0;
        c.wide = 1.0;
        c.pin_opacity = 0.0;
        c.ball = c.ball.min(1.0 - ease_in_out(u));
        if t < c.dur {
            return;
        }
        self.tap(0.25, s);
        s.send(Event::Settled { phase: Phase::Return });
        self.settle_idle(s);
    }

    /// The machine has sent the lodger back into the hole: `Pack`, or `Hidden` with reduced motion.
    pub(super) fn start_pack(&mut self, state: State, s: &mut Sender) {
        self.reset_poses(0.0);
        self.c.t0 = self.now;
        self.c.from_x = self.c.x;
        self.c.from_y = self.c.y;
        s.sound(Cue::Knock { x: self.c.x, width: W }, self.now);
        if state == State::Hidden {
            return self.rest_hidden();
        }
        let now = self.now;
        self.wake(now);
    }

    /// The reverse: the lodger hops into the "e" hole, grabs the corner pin, and reels all 800
    /// pins from the "o" back into the "e" before tucking itself back inside.
    pub(super) fn pack(&mut self, t: f64, cue: u8, s: &mut Sender) {
        let mut ran = [false; 2];
        let reached = run_cues(&SLOT_PACK, cue, t, |i| ran[i] = true);
        if ran[0] {
            s.sound(Cue::Slot { column: 65.0, columns: 68.0, x: HOLE[0], width: W }, self.now);
        }
        if ran[1] {
            self.blink_once();
        }
        if reached != cue {
            s.send(Event::CueStep { phase: Phase::Pack, cue: reached });
        }
        if due(t, 1.2) {
            s.send(Event::Settled { phase: Phase::Pack });
            return self.rest_hidden();
        }
        for (i, p) in self.poses.iter_mut().enumerate() {
            let start = 0.32 + (i as f64 / 7.0) * 0.48;
            p.opacity = seg(t, start, start + 0.32);
            p.x = 0.0;
            p.y = 0.0;
        }
        let c = &mut self.c;
        // Hops from the pointer into the "e" counter and grabs the corner starter pin while any
        // loose pins fly home.
        if t < 0.3 {
            let u = t / 0.3;
            c.x = lerp(c.from_x, REST[0], ease_out(u));
            c.y = lerp(c.from_y, REST[1] - 3.0, ease_out(u)) - (std::f64::consts::PI * u).sin() * 8.0;
            c.rot = lerp(c.rot, 10.0, u);
            c.crouch = 0.0;
            c.wide = 1.15;
            c.squint = 1.0;
            c.body = 1.0;
            c.eyes = 1.0;
            c.notch = u;
            c.pin_opacity = 1.0;
            c.pin_x = lerp(PIN_SLOT[0], REST[0] + 3.5, u);
            c.pin_y = lerp(PIN_SLOT[1], REST[1] - 10.5, u);
            c.pin_rot = u * 18.0;
            c.pin_scale = 1.3;
            return;
        }
        // Reels the wave in from "o" to "e" while holding the corner pin.
        if t < 0.9 {
            let u = seg(t, 0.3, 0.9);
            let tug = (u * std::f64::consts::PI * 3.0).sin();
            c.x = REST[0] + tug * 1.2;
            c.y = REST[1] - 3.0 + u * 2.0;
            c.rot = tug * 10.0;
            c.crouch = tug.abs() * 0.14;
            c.wide = 1.0;
            c.squint = 0.45;
            c.look_x = -1.0;
            c.look_y = 0.0;
            c.notch = 1.0;
            c.pin_opacity = 1.0;
            c.pin_x = lerp(REST[0] + 3.5, PIN_SLOT[0], u);
            c.pin_y = lerp(REST[1] - 10.5, PIN_SLOT[1], u);
            c.pin_rot = (1.0 - u) * 18.0;
            c.pin_scale = lerp(1.3, 1.0, u);
            return;
        }
        // Slots the corner pin flush and sinks back into the "e" hole.
        let u = seg(t, 0.9, 1.14);
        c.x = REST[0];
        c.y = lerp(REST[1] - 1.0, REST[1], ease_out(u));
        c.rot = 0.0;
        c.crouch = (u * std::f64::consts::PI).sin() * 0.18;
        c.wide = 1.0;
        c.squint = 1.0;
        c.look_x = 0.0;
        c.look_y = 0.0;
        c.body = 1.0 - ease_in_out(u);
        c.notch = 1.0 - u;
        c.pin_opacity = 1.0 - u;
    }

    pub(super) fn hop(&mut self, t: f64, cue: u8, s: &mut Sender) {
        let u = seg(t, 0.08, 0.43);
        self.c.crouch = if t < 0.08 { 0.15 * (t / 0.08) } else { 0.0 };
        self.c.y = GROUND - (std::f64::consts::PI * u).sin() * 8.0;
        self.c.squint = 0.35;
        if due(t, 0.43) && cue == 0 {
            s.send(Event::CueStep { phase: Phase::Hop, cue: 1 });
            self.tap(0.3, s);
        }
        if t < 0.8 {
            return;
        }
        s.send(Event::Settled { phase: Phase::Hop });
        self.c.y = GROUND;
        self.c.crouch = 0.0;
        self.c.squint = 1.0;
        self.look();
    }
}
