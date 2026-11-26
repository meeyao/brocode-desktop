//! The new-session page's Easter egg, as Desktop's (`new-session/wordmark*`, `pin-board*`,
//! `brick-breaker*`, `pin-sound.ts`): something lives in the top counter of the wordmark's final
//! "e". Its eyes peek out when the pointer comes near; pulling it out pops every letter into a 3D
//! board of 800 pins that lean toward the pointer and fly when knocked; dropping the lodger on the
//! composer plays Brick Breaker with the pins as bricks; carrying it back into the "e" packs the
//! board away.
//!
//! Nothing of it runs, allocates or draws until the pointer comes near the "e": the page's
//! wordmark is the plain one (`view::logo_at`), and `Egg::live` is `None`. While it moves, the
//! frames step here (`poll`) and draw in the compositor (`gpu.rs`) without rebuilding the view; at
//! rest it asks for no frame, and once the lodger is back in the "e" with its eyes closed,
//! everything is let go.

pub mod brick;
mod draw;
mod ease;
pub mod gpu;
pub mod lodger;
pub mod pins;
pub mod rng;
pub mod sound;
pub mod view;
pub mod wordmark;

use std::cell::Cell;
use std::time::Duration;

use rune::Tx;

use lodger::{Ask, BlinkTimer, Lodger, Sender, Stage};
use pins::Pins;
use pins::draw::{Colors, Quads, Scratch};
use wordmark::{Event, State};

/// Host state and callbacks required by the new-session Easter egg.
pub trait EggHost: opencode_kernel::AppEnv {
    fn egg(&self) -> &Egg;
    fn egg_mut(&mut self) -> &mut Egg;
    fn is_draft_route(&self) -> bool;
    fn interface_sounds(&self) -> bool;
    fn focus_composer(&mut self);
}

pub(crate) fn arg(name: &str) -> Option<String> {
    let a = rune::extra_args();
    a.iter().position(|x| x == name).and_then(|i| a.get(i + 1).cloned())
}

/// The egg, or nothing at all.
#[derive(Default)]
pub struct Egg {
    pub live: Option<Box<Live>>,
    /// The page's geometry from its last build, for the palette's way in before the egg is out.
    pub rest: Cell<Geo>,
    /// The egg has come out once in this run: from then the palette offers its game.
    pub found: bool,
    /// The window is off screen (minimised, hidden or covered whole): Desktop's frame loops stop
    /// with `requestAnimationFrame`, and so do the egg's.
    pub off_screen: bool,
}

/// The window went off screen or came back (`App::on_visibility`). Back, the egg carries on from
/// where its clock is, as Desktop's loops do: timed moves land where they'd be, physics takes one
/// clamped step.
pub fn visibility(s: &mut dyn EggHost, shown: bool, tx: &mut Tx) {
    let egg = s.egg_mut();
    egg.off_screen = !shown;
    if shown && egg.live.is_some() {
        tx.wake_after(Duration::ZERO);
    }
}

/// Where things are on the page, in the new-session panel's points, as the last layout put them.
/// The view writes it; the egg reads it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Geo {
    /// The panel's origin in the window, and its size.
    pub panel: [f64; 4],
    /// The wordmark's box.
    pub logo: [f64; 4],
    /// The composer's box and its corner radius.
    pub composer: Option<[f64; 4]>,
    /// The composer and its tray together (the drop hint's box).
    pub wrap: Option<[f64; 4]>,
    pub radius: f64,
    /// The page is dark: the pins and the lodger take its colours.
    pub dark: bool,
}

impl Geo {
    /// The lodger's stage: the 68x14 whole-point pitch footprint, its origin rounded in the panel
    /// (`fitStage`).
    pub fn stage(&self) -> Stage {
        let [x, y, w, h] = self.logo;
        let pitch = (w / 68.0).round().max(3.0);
        Stage { x: (x + (w - pitch * 68.0) / 2.0).round(), y: (y + (h - pitch * 14.0) / 2.0).round(), k: pitch / 3.0 }
    }
}

/// Idle-or-running animation frame loop shared by the pin board and lodger (`wanted` + `last`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum FrameLoop {
    #[default]
    Idle,
    /// Running; `last` is the previous frame timestamp (`now - 1/60` on wake so the first frame
    /// steps by one 60 Hz tick when run at `now`).
    Running { last: f64 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FrameLoopEvent {
    Wake { now: f64 },
    BeginFrame { now: f64 },
    EndFrame { now: f64, keep_running: bool },
}

impl FrameLoop {
    pub fn running(now: f64) -> Self {
        Self::Running { last: now - 1.0 / 60.0 }
    }

    pub fn is_running(self) -> bool {
        matches!(self, Self::Running { .. })
    }

    pub fn step(&mut self, event: FrameLoopEvent) -> Option<f64> {
        match (*self, event) {
            (Self::Idle, FrameLoopEvent::Wake { now }) => { *self = Self::running(now); None }
            (Self::Running { last }, FrameLoopEvent::BeginFrame { now }) => {
                *self = Self::Idle;
                Some((now - last).clamp(1.0 / 240.0, 1.0 / 30.0))
            }
            (Self::Idle, FrameLoopEvent::EndFrame { now, keep_running: true }) => {
                *self = Self::Running { last: now };
                None
            }
            _ => None,
        }
    }
}

/// The pin board while it's mounted, with what it draws.
pub struct Board {
    pub pins: Pins,
    pub colors: Colors,
    pub back: Quads,
    pub front: Quads,
    pub front_on: bool,
    scratch: Scratch,
    frames: FrameLoop,
}

/// The shimmer across the flat wordmark when it shows again (`wordmark-shimmer`, 800 ms).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shimmer {
    Off,
    Running { since: f64 },
}

pub const SHIMMER: f64 = 0.8;

pub struct Live {
    pub state: State,
    pub lodger: Lodger,
    pub board: Option<Box<Board>>,
    pub game: Option<Box<brick::Game>>,
    rng: rng::Rng,
    pub geo: Cell<Geo>,
    applied: Geo,
    asks: Vec<Ask>,
    game_asks: Vec<brick::Ask>,
    cues: Vec<sound::Sound>,
    pub shimmer: Shimmer,
    /// The composer's drop hint, 0 to 1 on its curve (wordmark.css:31-37), as the view's
    /// transition runs it: where the composer is when the game takes its box.
    sink: ease::Tween,
    /// The flat wordmark showed last frame (its shimmer runs each time it comes back).
    flat_shown: bool,
    /// What the page around the egg shows, which the view reads.
    pub page: Page,
    /// The last frame's clock, which the compositor draws at.
    pub clock: f64,
    /// The keyboard's way in, playing the pointer's part.
    auto: Auto,
}

/// The palette's Play Brick Breaker: the egg played as a pointer would, one step a frame. It
/// pulls the lodger out of the `e` (`Pull`), waits for it to perch (`Perch`), then carries it
/// onto the composer from where it perched (`Carry`) and lets go there.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Auto {
    #[default]
    Off,
    Pull(u8),
    Perch,
    Carry { from: [f64; 2], step: u8 },
}

/// The pull's and the carry's frames: as long as a quick drag's.
const PULL: u8 = 12;
const CARRY: u8 = 18;

/// What the egg changes on the page around it: Desktop sets these on the DOM.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Page {
    /// The composer and its tray fade out for the game (160 ms ease-out), and the tip (200 ms).
    pub composer_hidden: bool,
    /// The pointer hides over the panel while the paddle plays.
    pub cursor_hidden: bool,
    /// Who draws the flat wordmark.
    pub flat: Flat,
}

/// The flat wordmark: the page's own still one, the egg's while its letters move, fade or
/// shimmer, or none while the pin board stands in for it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Flat {
    #[default]
    Page,
    Egg,
    None,
}

impl Live {
    fn new(now: f64, geo: Geo, still: bool) -> Live {
        mark("wake", now);
        let mut lodger = Lodger::new(still, geo.stage());
        lodger.now = now;
        lodger.render();
        Live {
            state: State::new(false),
            lodger,
            board: None,
            game: None,
            rng: rng::Rng::new(now.to_bits()),
            geo: Cell::new(geo),
            applied: geo,
            asks: Vec::with_capacity(8),
            game_asks: Vec::with_capacity(8),
            cues: Vec::with_capacity(16),
            shimmer: Shimmer::Off,
            sink: ease::Tween::At(0.0),
            flat_shown: true,
            page: Page::default(),
            clock: now,
            auto: Auto::Off,
        }
    }

    /// Runs the lodger's `f` with a sender, then mounts or drops the pin board as the state asks.
    fn with_lodger<T>(&mut self, now: f64, f: impl FnOnce(&mut Lodger, &mut Sender) -> T) -> T {
        let mut sender = Sender { state: &mut self.state, asks: &mut self.asks };
        let out = f(&mut self.lodger, &mut sender);
        self.sync(now);
        out
    }

    /// The pin board is mounted from the moment the lodger pops it out until the last pin slots
    /// back into the "e" (`selectPinBoard`), and closes while the lodger packs it.
    fn sync(&mut self, now: f64) {
        match wordmark::pin_board(self.state) {
            Some(sel) => {
                if self.board.is_none() {
                    let geo = self.geo.get();
                    let mut pins = Pins::new(if sel.sweep { pins::Entrance::Sweep } else { pins::Entrance::Chain }, now, self.lodger.still);
                    pins.layout(geo.panel[2], geo.panel[3], geo.logo, geo.composer.map(|[x, y, w, _]| [x, x + w, y]));
                    if let Some(cue) = pins.mount_cue() {
                        self.cues.extend(sound::ask(cue, now));
                    }
                    let colors = colors(geo.dark);
                    self.board = Some(Box::new(Board { pins, colors, back: Quads::default(), front: Quads::default(), front_on: false, scratch: Scratch::default(), frames: FrameLoop::running(now) }));
                    wordmark::step(&mut self.state, Event::BoardMounted);
                }
                if sel.closing && let Some(b) = self.board.as_deref_mut() && !matches!(b.pins.board, pins::machine::Board::Closing { .. }) {
                    b.pins.close(now, &mut self.rng, &mut self.cues);
                    b.wake(now);
                }
            }
            None => {
                if let Some(mut b) = self.board.take() {
                    b.pins.board = pins::machine::step(b.pins.board, pins::machine::Event::Dispose);
                }
            }
        }
        if !wordmark::brick_breaker(self.state)
            && let Some(mut g) = self.game.take()
        {
            g.dispose();
        }
    }

    /// Nothing moves, nothing is out and the eyes are shut: the egg can go.
    fn spent(&self) -> bool {
        self.state == State::Hidden && self.lodger.c.eyes == 0.0 && !self.lodger.frames.is_running() && self.board.is_none() && self.game.is_none() && self.shimmer == Shimmer::Off && self.lodger.blink == lodger::Blink::Open
    }
}

impl Board {
    fn wake(&mut self, now: f64) {
        self.frames.step(FrameLoopEvent::Wake { now });
    }
}

/// The pin board's theme colours (`readColors`): the panel's `bg-base` and the icon tokens.
fn colors(dark: bool) -> Colors {
    let t = |name: &str, fallback: [f64; 3]| {
        opencode_ui::palette::token(dark, name).map_or(fallback, |c| [c.r, c.g, c.b].map(|v| (f64::from(rune::color::srgb_encode(v)) * 255.0).round()))
    };
    Colors::new(
        t("background-bg-base", [22.0; 3]),
        t("icon-icon-faint", [92.0; 3]),
        t("icon-icon-muted", [128.0; 3]),
        t("icon-icon-base", [219.0; 3]),
        t("orange-500", [255.0, 164.0, 120.0]),
    )
}

/// The egg's frames: the lodger, the pin board and the game, each stepping on the frame's time,
/// and the sounds they ask for. True when the view must run again.
pub fn poll(s: &mut dyn EggHost, now: f64, tx: &mut Tx) -> bool {
    let started = tracing().then(rune::time::Instant::now);
    // Only a window with a person at it plays (as the app's other sounds), unless a capture asks.
    // Desktop's sounds also wait while the page is hidden (`audio()`: `document.hidden`).
    let sounds = s.interface_sounds() && (s.persists() || sound::capturing()) && !s.egg().off_screen;
    sound::enable(sounds);
    if sound::tick(now) {
        tx.wake_after(Duration::ZERO);
    }
    if s.egg().off_screen {
        return false;
    }
    drive(s, tx);
    let egg = s.egg_mut();
    let Some(live) = egg.live.as_deref_mut() else { return false };
    let page = live.page;
    let state = live.state;
    let geo = live.geo.get();
    live.clock = now;
    if geo != live.applied {
        if geo.dark != live.applied.dark && let Some(b) = live.board.as_deref_mut() {
            b.colors = colors(geo.dark);
            b.wake(now);
        }
        live.applied = geo;
        live.lodger.stage = geo.stage();
        if let Some(b) = live.board.as_deref_mut() && b.pins.layout(geo.panel[2], geo.panel[3], geo.logo, geo.composer.map(|[x, y, w, _]| [x, x + w, y])) {
            b.wake(now);
        }
        if let (Some(g), Some(c)) = (live.game.as_deref_mut(), geo.composer) {
            g.layout(geo.panel[2], geo.panel[3], Some(geo.logo), brick::Box { x: c[0], y: c[1], w: c[2], h: c[3], r: geo.radius });
        }
    }
    if let BlinkTimer::Due(due) = live.lodger.blink_timer && now >= due {
        live.lodger.blink_due(now, live.state, &mut live.rng);
    }
    if live.lodger.frames.is_running() {
        live.with_lodger(now, |l, s| l.frame(now, s));
    }
    // Desktop's lodger `render` ends by starting the blink timer when it's idle (`wordmark.tsx:396`):
    // in its own frame, before the board's.
    if matches!(live.state, State::Idle { .. }) {
        live.lodger.schedule_blink(live.state, now, &mut live.rng);
    }
    if !matches!(live.state, State::Hidden | State::Drag | State::Snap) {
        egg.found = true;
    }
    if live.game.is_some() {
        step_game(live, now, tx);
    }
    if let Some(b) = live.board.as_deref_mut()
        && let Some(dt) = b.frames.step(FrameLoopEvent::BeginFrame { now })
    {
        b.pins.step(now, dt, &mut live.rng, &mut live.cues);
        b.front_on = b.pins.draw(now, &b.colors, &mut b.back, &mut b.front, &mut b.scratch);
        let keep_running = !b.pins.settled(now);
        b.frames.step(FrameLoopEvent::EndFrame { now, keep_running });
    }
    // The flat wordmark's shimmer runs each time it shows again (a CSS animation restarts when
    // `display` comes back); never on the page's first show.
    live.sink.set(if wordmark::drop_over(live.state) { 1.0 } else { 0.0 }, now, 0.26, view::SINK);
    let flat = wordmark::flat_visible(live.state);
    if flat && !live.flat_shown && !live.lodger.still {
        live.shimmer = Shimmer::Running { since: now };
    }
    live.flat_shown = flat;
    if let Shimmer::Running { since } = live.shimmer && now - since >= SHIMMER {
        live.shimmer = Shimmer::Off;
    }
    live.lodger.end_blink(now);
    live.page.flat = match (flat, live.state == State::Hidden && live.shimmer == Shimmer::Off) {
        (false, _) => Flat::None,
        (true, true) => Flat::Page,
        (true, false) => Flat::Egg,
    };
    let mut focus = false;
    for a in live.asks.drain(..) {
        match a {
            Ask::Sound(cue) => live.cues.push(cue),
            Ask::FocusComposer => focus = true,
        }
    }
    // Asked for (and their numbers drawn) where they happened; played together at the frame's end.
    for s in live.cues.drain(..) {
        sound::play(s, now);
    }
    let moving = live.lodger.frames.is_running() || live.board.as_ref().is_some_and(|b| b.frames.is_running()) || live.game.is_some() || live.lodger.blink != lodger::Blink::Open || live.shimmer != Shimmer::Off || live.auto != Auto::Off;
    if moving {
        tx.wake_after(Duration::ZERO);
    } else if let BlinkTimer::Due(due) = live.lodger.blink_timer {
        tx.wake_at(due);
    }
    gpu::set_animating(moving);
    if let Some(t) = started {
        COST.with(|c| c.set([t.elapsed().as_secs_f64() * 1e6, c.get()[1]]));
    }
    trace(live, now);
    let changed = live.state != state || live.page != page;
    if live.spent() {
        egg.live = None;
        mark("gone", now);
        gpu::set_animating(false);
        return true;
    }
    // Brick Breaker gave the keyboard back (`focus.composer`).
    if focus {
        s.focus_composer();
    }
    changed || focus
}

thread_local! {
    /// `--egg-trace`'s cost columns: this frame's poll and the last GPU prepare, in µs.
    pub static COST: Cell<[f64; 2]> = const { Cell::new([0.0; 2]) };
}

/// A proof run is tracing the egg (`--egg-trace`).
pub fn tracing() -> bool {
    thread_local! {
        static ON: bool = arg("--egg-trace").is_some();
    }
    ON.with(|o| *o)
}

#[cfg(not(target_arch = "wasm32"))]
fn memory_stats() -> (i64, u64) {
    (procwatch::alloc::heap().map_or(0, |h| h.live), procwatch::usage().footprint.unwrap_or(0))
}
#[cfg(target_arch = "wasm32")]
fn memory_stats() -> (i64, u64) { (0, 0) }

/// `--egg-trace`: the process's memory as the egg wakes and once it has gone.
pub(crate) fn mark(what: &str, now: f64) {
    if !tracing() {
        return;
    }
    let (heap, footprint) = memory_stats();
    eprintln!("egg memory {what} t={now:.3} heap={heap} footprint={footprint}");
}

/// `--egg-trace <file>`: one JSON line per frame of what the egg drew, as the harness reads
/// Desktop's DOM for the same frame, so the two can be compared number by number.
fn trace(live: &Live, now: f64) {
    use std::io::Write;
    thread_local! {
        static OUT: std::cell::RefCell<Option<std::io::BufWriter<std::fs::File>>> =
            std::cell::RefCell::new(arg("--egg-trace").and_then(|p| std::fs::File::create(p).ok()).map(std::io::BufWriter::new));
    }
    OUT.with_borrow_mut(|out| {
        let Some(out) = out else { return };
        let (c, d) = (&live.lodger.c, &live.lodger.drawn);
        let [cx, cy] = d.critter.apply(0.0, 0.0);
        let mut line = format!(
            "{{\"t\":{now:.6},\"state\":\"{:?}\",\"x\":{cx},\"y\":{cy},\"cx\":{},\"cy\":{},\"body\":{},\"eyes\":{},\"pin\":{},\"neck\":{},\"notch\":{},\"letter0\":{},\"letter7\":{},\"flat\":\"{:?}\"",
            live.state, c.x, c.y, d.body, d.eyes, d.pin, d.neck, d.notch, d.letters[0].0, d.letters[6].0, live.page.flat
        );
        let [poll_us, prepare_us] = COST.with(Cell::get);
        let (heap, footprint) = memory_stats();
        line += &format!(",\"look\":[{},{}],\"heap\":{heap},\"footprint\":{footprint}", c.look_x, c.look_y);
        line += &format!(",\"sink\":{},\"draws\":{},\"poll_us\":{poll_us:.1},\"prepare_us\":{prepare_us:.1}", live.sink.value(now), rng::drawn());
        if let Some(b) = live.board.as_deref() {
            line += &format!(",\"home\":{},\"tilt\":[{},{}]", b.pins.left(), b.pins.tilt[0], b.pins.tilt[1]);
        }
        if let Some(g) = live.game.as_deref() {
            let [x, y] = g.ball();
            line += &format!(",\"ball\":[{x},{y}],\"paddle\":[{},{},{},{},{}],\"score\":{},\"serve\":{},\"game\":\"{:?}\"", g.drawn.x, g.drawn.y, g.drawn.w, g.drawn.h, g.drawn_tilt, g.score, g.serve, g.state);
        }
        line.push('}');
        writeln!(out, "{line}").ok();
        out.flush().ok();
    });
}

fn step_game(live: &mut Live, now: f64, tx: &mut Tx) {
    let (Some(game), Some(board)) = (live.game.as_deref_mut(), live.board.as_deref_mut()) else { return };
    let mut sender = Sender { state: &mut live.state, asks: &mut live.asks };
    game.frame(now, &mut board.pins, &mut live.lodger, &mut sender, &mut live.rng, &mut live.game_asks);
    board.wake(now);
    game_asks(live, now, tx);
}

fn game_asks(live: &mut Live, now: f64, tx: &mut Tx) {
    let mut done = false;
    for a in std::mem::take(&mut live.game_asks) {
        match a {
            brick::Ask::Sound(cue) => live.cues.push(cue),
            brick::Ask::Composer(hidden) => live.page.composer_hidden = hidden,
            brick::Ask::Cursor(hidden) => live.page.cursor_hidden = hidden,
            brick::Ask::Done => done = true,
        }
    }
    if done {
        // The HUD is drawn, not read: the reader hears the score as the game ends.
        if let Some(g) = live.game.as_deref() {
            tx.announce(opencode_i18n::t!("rune.egg.over", score = g.score), rune::LiveRegion::Polite);
        }
        live.with_lodger(now, |_, s| s.send(Event::GameDone));
        live.page.composer_hidden = false;
        live.page.cursor_hidden = false;
    }
}

/// The game mounts as the lodger lands on the composer (`selectBrickBreaker`): the composer fades
/// and its focus goes, the pins hurry home, and the ball starts where the lodger is.
fn start_game(live: &mut Live, now: f64, tx: &mut Tx) {
    if live.game.is_some() || !wordmark::brick_breaker(live.state) {
        return;
    }
    let geo = live.geo.get();
    let Some(c) = geo.composer else { return };
    // The composer as drawn: sunk under the lodger by however far its hint has run (Desktop
    // reads the form's transformed rect).
    let p = live.sink.value(now);
    let [cx, cy] = geo.wrap.map_or([c[0] + c[2] / 2.0, c[1] + c[3] / 2.0], |w| [w[0] + w[2] / 2.0, w[1] + w[3] / 2.0]);
    let s = 1.0 - 0.015 * p;
    let composer = brick::Box { x: cx + (c[0] - cx) * s, y: cy + (c[1] - cy) * s + 3.0 * p, w: c[2] * s, h: c[3] * s, r: geo.radius };
    let game = brick::Game::new(now, geo.panel[2], geo.panel[3], Some(geo.logo), composer, live.lodger.ball_at());
    live.game = Some(Box::new(game));
    if let Some(b) = live.board.as_deref_mut() {
        b.pins.play(now, &mut live.rng);
        b.wake(now);
    }
    live.page.composer_hidden = true;
    // The composer's focus goes (`wrap.inert`); the game's surface takes the keys.
    tx.focus("egg-game", false);
}

// Input, from the page's elements (`view.rs`). Points are the panel's.

/// The pointer moved over the "e"'s reach or the grip, the egg not necessarily out yet: it wakes
/// once the eyes would show.
pub fn pointer_move_at(s: &mut dyn EggHost, geo: Geo, x: f64, y: f64, tx: &mut Tx) {
    if s.egg().live.is_none() {
        if !gpu::on_gpu() {
            return;
        }
        let stage = geo.stage();
        let (ux, uy) = ((x - stage.x) / stage.k, (y - stage.y) / stage.k);
        if (ux - lodger::HOLE[0]).hypot(uy - lodger::HOLE[1]) >= 24.0 {
            return;
        }
        let still = s.reduced_motion();
        s.egg_mut().live = Some(Box::new(Live::new(tx.clock(), geo, still)));
        tx.instant();
    } else {
        tx.live_only();
    }
    pointer_move(s, x, y, tx);
}

/// The pointer moved over the panel.
pub fn pointer_move(s: &mut dyn EggHost, x: f64, y: f64, tx: &mut Tx) {
    let now = tx.clock();
    let Some(live) = s.egg_mut().live.as_deref_mut() else { return };
    let state = live.state;
    if live.lodger.pointer_move(x, y, now, state) || live.lodger.frames.is_running() {
        tx.wake_after(Duration::ZERO);
    }
    if let Some(g) = live.game.as_deref_mut() {
        g.pointer_move(x);
    }
    if let Some(b) = live.board.as_deref_mut() {
        b.pins.pointer_move(x, y);
        b.wake(now);
        tx.wake_after(Duration::ZERO);
    }
}

/// The pointer came into the panel: the board takes its first point as where it was too, so
/// the entry adds no speed (`onEnter`); the next move gives it.
pub fn pointer_entered(s: &mut dyn EggHost, tx: &mut Tx) {
    let Some(live) = s.egg_mut().live.as_deref_mut() else { return };
    if let Some(b) = live.board.as_deref_mut() {
        b.pins.pointer_entered();
        b.wake(tx.clock());
        tx.wake_after(Duration::ZERO);
    }
    tx.live_only();
}

/// The pointer left the panel.
pub fn pointer_leave(s: &mut dyn EggHost, tx: &mut Tx) {
    let now = tx.clock();
    let Some(live) = s.egg_mut().live.as_deref_mut() else { return };
    let state = live.state;
    live.lodger.pointer_leave(now, state);
    if let Some(b) = live.board.as_deref_mut() {
        b.pins.pointer_leave();
        b.wake(now);
    }
    tx.wake_after(Duration::ZERO);
    tx.live_only();
}

/// A press on the grip: the lodger is grabbed.
pub fn grab_at(s: &mut dyn EggHost, geo: Geo, x: f64, y: f64, tx: &mut Tx) {
    let now = tx.clock();
    if s.egg().live.is_none() {
        if !gpu::on_gpu() {
            return;
        }
        let still = s.reduced_motion();
        s.egg_mut().live = Some(Box::new(Live::new(now, geo, still)));
    }
    tx.instant();
    let Some(live) = s.egg_mut().live.as_deref_mut() else { return };
    let composer = live.geo.get().composer.map(|[x, y, w, h]| [x, y, x + w, y + h]);
    if live.with_lodger(now, |l, sn| l.grab(x, y, now, composer, sn)) {
        tx.wake_after(Duration::ZERO);
    }
}

/// The grip was let go (or the press was taken away).
pub fn release(s: &mut dyn EggHost, tx: &mut Tx) {
    let now = tx.clock();
    let Some(live) = s.egg_mut().live.as_deref_mut() else { return };
    live.with_lodger(now, |l, sn| l.release(now, sn));
    start_game(live, now, tx);
    tx.wake_after(Duration::ZERO);
}

/// A press on the panel off its controls: the board knocks pins out, or the game serves.
pub fn panel_press(s: &mut dyn EggHost, x: f64, y: f64, tx: &mut Tx) {
    let now = tx.clock();
    let Some(live) = s.egg_mut().live.as_deref_mut() else { return };
    if let (Some(g), Some(b)) = (live.game.as_deref_mut(), live.board.as_deref_mut()) {
        let mut sender = Sender { state: &mut live.state, asks: &mut live.asks };
        g.press(now, &mut b.pins, &mut live.lodger, &mut sender, &mut live.rng, &mut live.game_asks);
        game_asks(live, now, tx);
        tx.wake_after(Duration::ZERO);
        return;
    }
    if let Some(b) = live.board.as_deref_mut()
        && b.pins.pointer_down(x, y, now, &mut live.rng, &mut live.cues)
    {
        b.wake(now);
        tx.wake_after(Duration::ZERO);
    }
}

pub fn panel_release(s: &mut dyn EggHost) {
    if let Some(b) = s.egg_mut().live.as_deref_mut().and_then(|l| l.board.as_deref_mut()) {
        b.pins.pointer_up();
    }
}

/// A key going down or up while Brick Breaker plays, as Desktop reads `KeyboardEvent.key`:
/// the arrows, `a` and `d`, Space, Escape. True when the game took it.
pub fn game_key(s: &mut dyn EggHost, ev: &rune::KeyEvent, down: bool, tx: &mut Tx) -> bool {
    use brick::GameKey;
    let key = match &ev.key {
        rune::Key::Escape => GameKey::Escape,
        rune::Key::Left => GameKey::Left,
        rune::Key::Right => GameKey::Right,
        rune::Key::Up => GameKey::Serve,
        rune::Key::Text(t) if t == "a" => GameKey::Left,
        rune::Key::Text(t) if t == "d" => GameKey::Right,
        rune::Key::Text(t) if t == " " => GameKey::Serve,
        _ => return false,
    };
    key_to_game(s, key, down, tx)
}

fn key_to_game(s: &mut dyn EggHost, key: brick::GameKey, down: bool, tx: &mut Tx) -> bool {
    let now = tx.clock();
    let Some(live) = s.egg_mut().live.as_deref_mut() else { return false };
    let (Some(g), Some(b)) = (live.game.as_deref_mut(), live.board.as_deref_mut()) else { return false };
    let mut sender = Sender { state: &mut live.state, asks: &mut live.asks };
    let took = g.key(key, down, now, &mut b.pins, &mut live.lodger, &mut sender, &mut live.rng, &mut live.game_asks);
    game_asks(live, now, tx);
    tx.wake_after(Duration::ZERO);
    took
}

/// The window lost focus: the game lets held keys go, the board ends a drag.
pub fn blur(s: &mut dyn EggHost) {
    let Some(live) = s.egg_mut().live.as_deref_mut() else { return };
    if let Some(g) = live.game.as_deref_mut() {
        g.blur();
    }
    if let Some(b) = live.board.as_deref_mut() {
        b.pins.pointer_up();
    }
}

// The keyboard's way in. Desktop has none: the egg is the pointer's alone. Here the palette
// plays it the way a hand would, and the game then takes the keys as Desktop's does.

/// The draft page shows, the lodger is in the "e" or perched with no game yet, and the egg has
/// been found in this run: an Easter egg the palette lists isn't hidden. Under Reduce Motion there's
/// no game to play (`wordmark-machine.ts:209`).
pub fn can_play_command(s: &dyn EggHost) -> bool {
    let egg = s.egg();
    egg.found
        && !s.reduced_motion()
        && s.is_draft_route()
        && egg.live.as_deref().is_none_or(|l| l.auto == Auto::Off && matches!(l.state, State::Hidden | State::Idle { .. }))
}

/// Runs the palette's "Play Brick Breaker" command: pulls the lodger out and carries it onto the
/// composer as a hand would.
pub fn run_play_command(s: &mut dyn EggHost, tx: &mut Tx) {
    if !gpu::on_gpu() {
        return;
    }
    let now = tx.clock();
    let still = s.reduced_motion();
    let egg = s.egg_mut();
    let live = egg.live.get_or_insert_with(|| Box::new(Live::new(now, egg.rest.get(), still)));
    live.auto = match live.state {
        State::Hidden => Auto::Pull(0),
        State::Idle { .. } => Auto::Perch,
        _ => return,
    };
    tx.instant();
    tx.wake_after(Duration::ZERO);
}

/// Enter or Space on the "e" (or the perched lodger): the palette's play.
pub fn grip_key(s: &mut dyn EggHost, ev: &rune::KeyEvent, tx: &mut Tx) -> bool {
    let press = matches!(&ev.key, rune::Key::Enter) || matches!(&ev.key, rune::Key::Text(t) if t == " ");
    if !press {
        return false;
    }
    run_play_command(s, tx);
    true
}

/// One step of the palette's play: the pointer's press, move or release for this frame.
fn drive(s: &mut dyn EggHost, tx: &mut Tx) {
    let Some(live) = s.egg().live.as_deref() else { return };
    let (auto, state, geo, still) = (live.auto, live.state, live.geo.get(), live.lodger.still);
    let grip = live.lodger.to_panel(lodger::HOLE[0], lodger::HOLE[1]);
    let perch = live.lodger.to_panel(live.lodger.c.x, live.lodger.c.y - 5.0);
    let next = match auto {
        Auto::Off => return,
        Auto::Pull(0) => {
            grab_at(s, geo, grip[0], grip[1], tx);
            Auto::Pull(1)
        }
        // A long enough pull pops it before the press ends; a short one snaps it back.
        Auto::Pull(_) if matches!(state, State::Show { .. } | State::Idle { .. }) => {
            release(s, tx);
            Auto::Perch
        }
        Auto::Pull(_) if state != State::Drag => Auto::Off,
        Auto::Pull(n) if n <= PULL => {
            let u = f64::from(n) / f64::from(PULL);
            pointer_move(s, grip[0] + 160.0 * u, grip[1] + 80.0 * u, tx);
            Auto::Pull(n + 1)
        }
        Auto::Pull(_) => {
            release(s, tx);
            Auto::Perch
        }
        Auto::Perch => match state {
            // Under Reduce Motion a drop on the composer plays nothing: the lodger stays perched.
            State::Idle { .. } if still => Auto::Off,
            State::Idle { .. } => {
                grab_at(s, geo, perch[0], perch[1], tx);
                Auto::Carry { from: perch, step: 1 }
            }
            State::Show { .. } | State::Hop { .. } | State::Return { .. } => Auto::Perch,
            _ => Auto::Off,
        },
        Auto::Carry { .. } if !matches!(state, State::Carry { .. }) => Auto::Off,
        Auto::Carry { from, step } if step <= CARRY => {
            let to = geo.composer.map_or(from, |[x, y, w, h]| [x + w / 2.0, y + h / 2.0]);
            let u = f64::from(step) / f64::from(CARRY);
            pointer_move(s, from[0] + (to[0] - from[0]) * u, from[1] + (to[1] - from[1]) * u, tx);
            Auto::Carry { from, step: step + 1 }
        }
        Auto::Carry { .. } => {
            release(s, tx);
            Auto::Off
        }
    };
    if let Some(live) = s.egg_mut().live.as_deref_mut() {
        live.auto = next;
    }
}
