//! What the lodger's SVG holds after `render` (`wordmark.tsx` `render`): each part's transform
//! and opacity, in logo units. The compositor draws it (`gpu.rs`), with the blink, which Desktop
//! runs as its own animation, applied on top.

use super::{GROUND, HOLE, LETTER_CX, Lodger};

/// A 2D affine transform, SVG's `matrix(a b c d e f)`: x' = a x + c y + e, y' = b x + d y + f.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine(pub [f64; 6]);

impl Default for Affine {
    fn default() -> Self {
        Affine::ID
    }
}

impl Affine {
    pub const ID: Affine = Affine([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

    pub fn then(self, m: Affine) -> Affine {
        // self · m: m applies first, as SVG lists transforms left to right.
        let [a, b, c, d, e, f] = self.0;
        let [a2, b2, c2, d2, e2, f2] = m.0;
        Affine([a * a2 + c * b2, b * a2 + d * b2, a * c2 + c * d2, b * c2 + d * d2, a * e2 + c * f2 + e, b * e2 + d * f2 + f])
    }

    pub fn translate(x: f64, y: f64) -> Affine {
        Affine([1.0, 0.0, 0.0, 1.0, x, y])
    }

    pub fn scale(x: f64, y: f64) -> Affine {
        Affine([x, 0.0, 0.0, y, 0.0, 0.0])
    }

    /// `rotate(deg cx cy)`.
    pub fn rotate_about(deg: f64, cx: f64, cy: f64) -> Affine {
        let (s, c) = deg.to_radians().sin_cos();
        Affine::translate(cx, cy).then(Affine([c, s, -s, c, 0.0, 0.0])).then(Affine::translate(-cx, -cy))
    }

    pub fn apply(self, x: f64, y: f64) -> [f64; 2] {
        let [a, b, c, d, e, f] = self.0;
        [a * x + c * y + e, b * x + d * y + f]
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Drawn {
    pub critter: Affine,
    pub body: f64,
    /// The body's curl into a ball, to hundredths: width `12 - 2 curl`, corners `1.5 + 3.5 curl`.
    pub curl: f64,
    pub eyes: f64,
    /// The eyes' gaze, inside the critter.
    pub look: Affine,
    pub neck: f64,
    pub neck_line: [f64; 4],
    pub neck_w: f64,
    pub notch: f64,
    pub pin: f64,
    pub pin_at: Affine,
    pub letters: [(f64, Affine); 7],
}

impl Lodger {
    /// `render`: writes the parts from the critter's state, with the squash of recent landings.
    pub fn render(&mut self) {
        let c = &self.c;
        let now = self.now;
        let squash: f64 = self.impacts.iter().map(|&(at, amp)| {
            let t = now - at;
            if t < 0.0 { 0.0 } else { amp * (-t * 10.0).exp() * (t * 30.0).cos() }
        }).sum();
        let sy = (1.0 - squash) * (1.0 - c.crouch);
        let sx = (1.0 + squash * 0.7) * (1.0 + c.crouch * 0.8);
        // Stretching along an angle and rotating back keeps the eyes upright while the ball squashes on impact.
        let mut critter = Affine::translate(c.x, c.y);
        if c.stretch != 0.0 {
            critter = critter
                .then(Affine::rotate_about(c.angle, 0.0, -5.0))
                .then(Affine::scale(1.0 + c.stretch, 1.0 - c.stretch * 0.8))
                .then(Affine::rotate_about(-c.angle, 0.0, -5.0));
        }
        critter = critter.then(Affine::rotate_about(c.rot, 0.0, -5.0)).then(Affine::scale(sx, sy));
        let d = &mut self.drawn;
        d.critter = critter;
        d.body = c.body;
        d.curl = super::js_round(c.ball * 100.0) / 100.0;
        d.eyes = c.eyes;
        d.look = Affine::translate(c.look_x, c.look_y - 6.0).then(Affine::scale(c.wide, c.wide * c.squint)).then(Affine::translate(0.0, 6.0));
        d.neck = c.neck;
        if c.neck != 0.0 {
            let len = (c.x - HOLE[0]).hypot(c.y - 5.0 - HOLE[1]);
            d.neck_line = [HOLE[0], HOLE[1], c.x, c.y - 5.0];
            d.neck_w = (40.0 / len.max(1.0)).clamp(2.0, 6.0);
        }
        d.notch = c.notch;
        d.pin = c.pin_opacity;
        if c.pin_opacity != 0.0 {
            d.pin_at = Affine::translate(c.pin_x, c.pin_y).then(Affine::rotate_about(c.pin_rot, 0.0, 0.0)).then(Affine::scale(c.pin_scale, c.pin_scale));
        }
        for (i, p) in self.poses.iter().enumerate() {
            let cx = LETTER_CX[i];
            d.letters[i] = (p.opacity, Affine::translate(cx + p.x, GROUND + p.y).then(Affine::scale(p.sx, p.sy)).then(Affine::translate(-cx, -GROUND)));
        }
    }
}
