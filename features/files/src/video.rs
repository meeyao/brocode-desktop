//! A video file open in the Files panel: Desktop's `ArtifactVideo` (`@opencode/plugin` file
//! `artifact-view.tsx:508-690`). The clip sits in the stage's frame (`p-6`), as wide as the frame,
//! on black with 4 pt corners and the artifact shadow, under the controls Chromium draws for a
//! `<video controls>`: play, the time, the mute button with its slider, the overflow menu and the
//! timeline, over a gradient. They show while paused and while the pointer moves over a playing
//! clip, and fade once it rests or leaves (Desktop's `VideoOverlayState`,
//! `artifact-view-machine.ts:256-399`). Desktop's own expand button sits where Chromium's
//! fullscreen button would.
//!
//! The clip streams through Rune's `Player`: a file on this machine's server is read where it
//! lies, a remote one over the server's file route. Nothing of it is downloaded first.

use std::cell::Cell;
use std::hash::{Hash, Hasher};

mod controls;

use rune::{Drag, Phase, Player, PlayerState, Tx};
pub use opencode_ui::video_overlay::{Effect, Event, Overlay, step};
pub use controls::stage;

use crate::FilesHost;

pub use opencode_kit::path::is_video;

/// Builds the `/api/fs/read/...` path and lowercase file extension for downloading remote media
/// via `Authorization` HTTP headers (never `auth_token` query parameters).
pub fn remote_read_request(directory: &str, path: &str) -> (String, String) {
    use opencode_search::query::{encode_fs_path, percent_encode};
    let req = format!(
        "/api/fs/read/{}?location%5Bdirectory%5D={}",
        encode_fs_path(path),
        percent_encode(directory)
    );
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    (req, ext)
}

/// Workspace movie source shared by the file viewer and chat; never fetch the entire file, never
/// embed credentials (`auth_token`) in URLs (`packages/app/src/runtime/server/api.ts:45-56`), and
/// verify local files stay inside `directory` without symlinks/junctions before handing a local
/// path to `rune::Player`. On native local servers (or with no server), a rejected path returns
/// `None` with no fallback to `/api/fs/read/...` so symlinks and junctions cannot bypass
/// confinement. On password-protected remote servers, returns `None` so callers download media
/// via `Authorization` headers (`TempMediaFile`) instead.
pub fn source(server: Option<&opencode_kit::client::Server>, directory: &str, path: &str) -> Option<String> {
    let stream_url = |s: &opencode_kit::client::Server| {
        let (req, _) = remote_read_request(directory, path);
        s.stream_url(&req)
    };
    #[cfg(target_arch = "wasm32")]
    {
        return server.and_then(stream_url);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        if let Some(s) = server.filter(|s| !s.is_local()) {
            return stream_url(s);
        }
        let candidate = std::path::Path::new(directory).join(path).to_string_lossy().into_owned();
        browser_policy::verify_within_root(&candidate, directory, true)
    }
}

#[cfg(not(target_arch = "wasm32"))]
type AuthVideoDone = (String, String, u64, Result<opencode_kit::client::TempMediaFile, String>);

/// Lifecycle of one authenticated remote video download.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AuthVideoState {
    Loading { attempt: u64 },
    Failed,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug)]
enum AuthVideoEvent {
    Start,
    SpawnFailed { attempt: u64 },
    Landed { attempt: u64, ok: bool },
    Clear,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AuthVideoEffect {
    None,
    Spawn { attempt: u64 },
    Install,
    FallbackRead,
}

#[cfg(not(target_arch = "wasm32"))]
fn step_auth_video(
    state: Option<AuthVideoState>,
    event: AuthVideoEvent,
    next: u64,
) -> (Option<AuthVideoState>, AuthVideoEffect) {
    match (state, event) {
        (None, AuthVideoEvent::Start) => {
            (Some(AuthVideoState::Loading { attempt: next }), AuthVideoEffect::Spawn { attempt: next })
        }
        (Some(AuthVideoState::Loading { attempt }), AuthVideoEvent::SpawnFailed { attempt: a }) if attempt == a => {
            (Some(AuthVideoState::Failed), AuthVideoEffect::FallbackRead)
        }
        (Some(AuthVideoState::Loading { attempt }), AuthVideoEvent::Landed { attempt: a, ok: true })
            if attempt == a =>
        {
            (None, AuthVideoEffect::Install)
        }
        (Some(AuthVideoState::Loading { attempt }), AuthVideoEvent::Landed { attempt: a, ok: false })
            if attempt == a =>
        {
            (Some(AuthVideoState::Failed), AuthVideoEffect::FallbackRead)
        }
        (_, AuthVideoEvent::Clear) => (None, AuthVideoEffect::None),
        (state, _) => (state, AuthVideoEffect::None),
    }
}

/// Background authenticated remote video downloads (`Authorization` header -> owner-only
/// `TempMediaFile`, never `auth_token` query parameters).
#[cfg(not(target_arch = "wasm32"))]
pub(crate) struct AuthVideoDownloads {
    tx: std::sync::mpsc::Sender<AuthVideoDone>,
    rx: std::sync::mpsc::Receiver<AuthVideoDone>,
    states: std::collections::HashMap<(String, String), AuthVideoState>,
    next: u64,
}

#[cfg(not(target_arch = "wasm32"))]
impl Default for AuthVideoDownloads {
    fn default() -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        Self { tx, rx, states: std::collections::HashMap::new(), next: 0 }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl AuthVideoDownloads {
    fn step(&mut self, sid: &str, path: &str, event: AuthVideoEvent) -> AuthVideoEffect {
        let key = (sid.to_owned(), path.to_owned());
        let (next_state, effect) = step_auth_video(self.states.get(&key).copied(), event, self.next + 1);
        if matches!(effect, AuthVideoEffect::Spawn { .. }) {
            self.next += 1;
        }
        match next_state {
            Some(s) => {
                self.states.insert(key, s);
            }
            None => {
                self.states.remove(&key);
            }
        }
        effect
    }

    /// Starts downloading an authenticated remote video if not already loading or failed.
    /// Returns `true` if a download is running (newly started or already in flight), or `false`
    /// if spawning failed or the video already failed and should fall back to `read_file`.
    pub(crate) fn start(
        &mut self,
        server: &opencode_kit::client::Server,
        directory: &str,
        sid: &str,
        path: &str,
        waker: Option<rune::Waker>,
    ) -> bool {
        let attempt = match self.step(sid, path, AuthVideoEvent::Start) {
            AuthVideoEffect::Spawn { attempt } => attempt,
            _ => return self.is_loading(sid, path),
        };
        let srv = server.clone();
        let (req, ext) = remote_read_request(directory, path);
        let sid_owned = sid.to_owned();
        let path_owned = path.to_owned();
        let tx = self.tx.clone();
        if std::thread::Builder::new()
            .name("opencode-auth-video".into())
            .spawn(move || {
                let res = srv.download_media_temp(&req, &ext);
                let _ = tx.send((sid_owned, path_owned, attempt, res));
                if let Some(w) = waker {
                    w();
                }
            })
            .is_err()
        {
            self.step(sid, path, AuthVideoEvent::SpawnFailed { attempt });
            return false;
        }
        true
    }

    pub(crate) fn is_loading(&self, sid: &str, path: &str) -> bool {
        self.states
            .iter()
            .any(|((s, p), st)| s == sid && p == path && matches!(st, AuthVideoState::Loading { .. }))
    }

    pub(crate) fn has_failed(&self, sid: &str, path: &str) -> bool {
        self.states
            .iter()
            .any(|((s, p), st)| s == sid && p == path && *st == AuthVideoState::Failed)
    }

    pub(crate) fn clear(&mut self, sid: &str, path: &str) {
        self.step(sid, path, AuthVideoEvent::Clear);
    }

    pub(crate) fn forget_session(&mut self, sid: &str) {
        self.states.retain(|(s, _), _| s != sid);
    }

    /// Lock-free poll of completed downloads (`&self`), so `video::poll` never bumps `FILES`
    /// while downloads are in flight and always drops temp files for closed tabs/sessions.
    pub(crate) fn take(&self) -> Option<AuthVideoDone> {
        self.rx.try_recv().ok()
    }

    fn land(&mut self, sid: &str, path: &str, attempt: u64, ok: bool) -> AuthVideoEffect {
        self.step(sid, path, AuthVideoEvent::Landed { attempt, ok })
    }
}

/// How often a playing clip's time and timeline move: Chromium's `timeupdate`.
const TICK: f64 = 0.25;
/// The timeline's track: 16 pt in from each side.
pub(super) const TRACK_INSET: f32 = 16.0;

/// Explicit timer machine for Chromium's 0.25 s `timeupdate` cadence while controls are visible.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum TimeUpdate {
    #[default]
    Idle,
    Scheduled { due: f64 },
}

#[derive(Clone, Copy, Debug)]
enum TimeUpdateEvent {
    Active { now: f64 },
    Inactive,
}

#[derive(Clone, Copy, Debug, Default)]
struct TimeUpdateEffect {
    wake_at: Option<f64>,
    redraw: bool,
}

fn step_time_update(state: &mut TimeUpdate, event: TimeUpdateEvent) -> TimeUpdateEffect {
    match (*state, event) {
        (TimeUpdate::Idle, TimeUpdateEvent::Active { now }) => {
            let due = now + TICK;
            *state = TimeUpdate::Scheduled { due };
            TimeUpdateEffect { wake_at: Some(due), redraw: false }
        }
        (TimeUpdate::Scheduled { due }, TimeUpdateEvent::Active { now }) if now >= due => {
            let next = now + TICK;
            *state = TimeUpdate::Scheduled { due: next };
            TimeUpdateEffect { wake_at: Some(next), redraw: true }
        }
        (TimeUpdate::Scheduled { due }, TimeUpdateEvent::Active { .. }) => {
            TimeUpdateEffect { wake_at: Some(due), redraw: false }
        }
        (_, TimeUpdateEvent::Inactive) => {
            *state = TimeUpdate::Idle;
            TimeUpdateEffect::default()
        }
    }
}

/// Decode-failure fallback lifecycle for an open video clip.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum DecodeFallback {
    #[default]
    Active,
    /// Decoding failed and the file has been handed off to ordinary file reading.
    ReadingAsFile,
}

/// Snapshot of visible video presentation state used to detect discrete updates (end-of-stream,
/// late metadata, mute/scrub/tick changes) without hashing idle deadlines or hidden timestamps.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Presentation {
    state: std::mem::Discriminant<PlayerState>,
    loading: bool,
    shown: bool,
    scrub_bits: Option<u64>,
    playing: bool,
    muted: bool,
    has_sound: Option<bool>,
    size: Option<(u32, u32)>,
    duration_bits: Option<u64>,
    tick: Option<u64>,
}

/// A video file's player and its controls' state.
pub struct FileVideo {
    pub player: Player,
    /// Control visibility machine (`Overlay`), stored in a `Cell` so timer renewals on pointer
    /// move don't bump the `FILES` slot or trigger panel rebuilds when visibility didn't change.
    overlay: Cell<Overlay>,
    /// Periodic `timeupdate` timer state while playing with controls visible.
    time_update: Cell<TimeUpdate>,
    /// Last presentation snapshot reported to the Files panel.
    reported: Cell<Option<Presentation>>,
    /// The timeline under a drag: the clip time it points at.
    pub scrub: Scrub,
    /// Decode-failure handoff state (`poll`).
    fallback: DecodeFallback,
    /// Keeps an authenticated remote download alive on disk until this player drops.
    #[cfg(not(target_arch = "wasm32"))]
    _temp_file: Option<opencode_kit::client::TempMediaFile>,
}

impl FileVideo {
    /// A clip at `source` (a path or a URL), paused at its first frame, as `preload="metadata"`.
    pub fn new(source: &str, waker: Option<rune::Waker>) -> FileVideo {
        let player = Player::new(source.into());
        player.set_muted(false);
        if let Some(waker) = waker {
            player.set_waker(waker);
        }
        player.preload();
        FileVideo {
            player,
            overlay: Cell::default(),
            time_update: Cell::default(),
            reported: Cell::default(),
            scrub: Scrub::Idle,
            fallback: DecodeFallback::Active,
            #[cfg(not(target_arch = "wasm32"))]
            _temp_file: None,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn with_temp(temp: opencode_kit::client::TempMediaFile, waker: Option<rune::Waker>) -> Option<FileVideo> {
        let path = temp.path_str()?.to_owned();
        let mut clip = Self::new(&path, waker);
        clip._temp_file = Some(temp);
        Some(clip)
    }

    /// It can't play (Desktop's `decode.failed`).
    pub fn failed(&self) -> bool {
        matches!(self.player.state(), PlayerState::Failed(_))
    }

    pub fn is_loading(&self) -> bool {
        self.overlay.get() == Overlay::Loading
    }

    /// Chromium's controls show (and so does Desktop's expand button).
    pub fn controls_shown(&self) -> bool {
        self.overlay.get().shown() || self.scrub != Scrub::Idle
    }

    fn presentation(&self) -> Presentation {
        let p = &self.player;
        let shown = self.controls_shown();
        let scrub_bits = match self.scrub {
            Scrub::Idle => None,
            Scrub::Dragging { at, .. } => Some(at.to_bits()),
        };
        Presentation {
            state: std::mem::discriminant(&p.state()),
            loading: self.is_loading(),
            shown,
            scrub_bits,
            playing: p.is_playing(),
            muted: p.muted(),
            has_sound: p.has_sound(),
            size: p.natural_size(),
            duration_bits: p.duration().map(f64::to_bits),
            tick: shown.then(|| (p.time() * 4.0) as u64),
        }
    }

    /// Hashes only visible video presentation state (excluding idle-timer deadlines and hidden
    /// playback timestamps) so pointer moves and hidden playback don't rebuild the panel.
    pub fn version_hash(&self, h: &mut impl Hasher) {
        self.presentation().hash(h);
    }

    fn presentation_changed(&self) -> bool {
        let next = self.presentation();
        self.reported.replace(Some(next)) != Some(next)
    }

    fn step_overlay(&self, event: Event, tx: &mut Tx) -> bool {
        let mut state = self.overlay.get();
        let was_shown = state.shown();
        let was_loading = state == Overlay::Loading;
        if let Some(Effect::WakeAt(due)) = step(&mut state, event) {
            tx.wake_at(due);
        }
        self.overlay.set(state);
        was_shown != state.shown() || was_loading != (state == Overlay::Loading)
    }

    fn sync_time_update(&self, now: f64, tx: &mut Tx) -> bool {
        let mut state = self.time_update.get();
        let event = if self.player.is_playing() && self.controls_shown() {
            TimeUpdateEvent::Active { now }
        } else {
            TimeUpdateEvent::Inactive
        };
        let eff = step_time_update(&mut state, event);
        self.time_update.set(state);
        if let Some(due) = eff.wake_at {
            tx.wake_at(due);
        }
        eff.redraw
    }
}

/// The timeline's drag (Chromium's range input): where it points, where it started (for cancel),
/// and whether the clip played when it started, so it plays on once let go.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Scrub {
    #[default]
    Idle,
    Dragging { at: f64, origin: f64, resume: bool },
}

#[derive(Clone, Copy, Debug)]
enum ScrubEvent {
    Down { at: f64, origin: f64, playing: bool },
    Move { at: f64 },
    Up { at: f64, cancelled: bool },
}

#[derive(Clone, Copy, Debug)]
enum SeekKind {
    Fast(f64),
    Exact(f64),
}

#[derive(Clone, Copy, Debug, Default)]
struct ScrubEffects {
    pause: bool,
    play: bool,
    seek: Option<SeekKind>,
}

fn step_scrub(state: &mut Scrub, event: ScrubEvent) -> ScrubEffects {
    match (*state, event) {
        (Scrub::Idle, ScrubEvent::Down { at, origin, playing }) => {
            *state = Scrub::Dragging { at, origin, resume: playing };
            ScrubEffects { pause: playing, play: false, seek: Some(SeekKind::Fast(at)) }
        }
        (Scrub::Dragging { origin, resume, .. }, ScrubEvent::Down { at, .. } | ScrubEvent::Move { at }) => {
            *state = Scrub::Dragging { at, origin, resume };
            ScrubEffects { pause: false, play: false, seek: Some(SeekKind::Fast(at)) }
        }
        (Scrub::Dragging { origin, resume, .. }, ScrubEvent::Up { at, cancelled }) => {
            let target = if cancelled { origin } else { at };
            *state = Scrub::Idle;
            ScrubEffects { pause: false, play: resume, seek: Some(SeekKind::Exact(target)) }
        }
        (Scrub::Idle, ScrubEvent::Move { .. } | ScrubEvent::Up { .. }) => ScrubEffects::default(),
    }
}

fn video_ref<'a>(host: &'a dyn FilesHost, sid: &str, path: &str) -> Option<&'a FileVideo> {
    host.files().sessions.get(sid)?.videos.get(path)
}

fn video_mut<'a>(host: &'a mut dyn FilesHost, sid: &str, path: &str) -> Option<&'a mut FileVideo> {
    host.files_mut().sessions.get_mut(sid)?.videos.get_mut(path)
}

/// Folds each open video's playback into its overlay, and moves a playing one's time on.
pub fn poll(host: &mut dyn FilesHost, tx: &mut Tx) -> bool {
    let mut instant_changed = false;
    #[cfg(not(target_arch = "wasm32"))]
    while let Some((sid, path, attempt, res)) = host.files().auth_videos.take() {
        let waker = host.files().waker.clone();
        let clip = res.ok().and_then(|temp| FileVideo::with_temp(temp, waker));
        let effect = host.files_mut().auth_videos.land(&sid, &path, attempt, clip.is_some());
        if !host.files().sessions.contains_key(&sid) {
            continue;
        }
        let wrap = host.files().wrap;
        match effect {
            AuthVideoEffect::Install => {
                if let Some(clip) = clip
                    && let Some(ws) = host.files_mut().sessions.get_mut(&sid)
                {
                    let mut doc = wrap.doc(&path, "");
                    doc.set_binary(0);
                    ws.docs.insert(path.clone(), doc);
                    ws.videos.insert(path, clip);
                    instant_changed = true;
                }
            }
            AuthVideoEffect::FallbackRead => {
                if let Some(ws) = host.files_mut().sessions.get_mut(&sid) {
                    ws.docs.insert(path.clone(), wrap.doc(&path, ""));
                }
                crate::ops::read_file(host, &sid, &path, true);
                instant_changed = true;
            }
            AuthVideoEffect::None | AuthVideoEffect::Spawn { .. } => {}
        }
    }
    if host.files().sessions.values().all(|ws| ws.videos.is_empty()) {
        if instant_changed {
            tx.instant();
        }
        return instant_changed;
    }
    let now = tx.clock();
    let mut overlay_changed = false;
    let mut state_changed = false;
    let mut read = Vec::new();
    for (sid, ws) in host.files().sessions.iter() {
        for (path, v) in ws.videos.iter() {
            if v.failed() && v.fallback == DecodeFallback::Active {
                read.push((sid.clone(), path.clone()));
            }
            let paused = !v.player.is_playing();
            let event = match (v.overlay.get(), v.player.state()) {
                (Overlay::Loading, PlayerState::Loading) => None,
                (Overlay::Loading, _) => Some(Event::Loaded { paused, now }),
                _ => Some(Event::Playback { paused, now }),
            };
            if let Some(event) = event {
                overlay_changed |= v.step_overlay(event, tx);
            }
            overlay_changed |= v.step_overlay(Event::Tick { now }, tx);
            state_changed |= v.sync_time_update(now, tx);
            state_changed |= v.presentation_changed();
        }
    }
    // A clip that can't play is read as the file it is, as Desktop shows a failed decode.
    for (sid, path) in read {
        if let Some(ws) = host.files_mut().sessions.get_mut(&sid) {
            if let Some(v) = ws.videos.get_mut(&path) {
                v.fallback = DecodeFallback::ReadingAsFile;
            }
            ws.docs.remove(&path);
        }
        crate::ops::ensure_file_loaded(host, &sid, &path);
        instant_changed = true;
    }
    if overlay_changed || state_changed {
        host.touch_files();
    }
    if overlay_changed {
        tx.animated();
    } else if state_changed || instant_changed {
        tx.instant();
    }
    overlay_changed || state_changed || instant_changed
}

/// Toggles play/pause for the video at `(sid, path)`; returns `true` if a live video was toggled.
pub(crate) fn toggle_play(host: &mut dyn FilesHost, sid: &str, path: &str, tx: &mut Tx) -> bool {
    let Some(v) = video_ref(host, sid, path).filter(|v| !v.failed()) else { return false };
    if v.player.is_playing() {
        v.player.pause();
    } else {
        v.player.play();
    }
    let now = tx.clock();
    let paused = !v.player.is_playing();
    let vis_changed = v.step_overlay(Event::Playback { paused, now }, tx);
    v.sync_time_update(now, tx);
    v.presentation_changed();
    host.touch_files();
    if vis_changed {
        tx.animated();
    } else {
        tx.instant();
    }
    true
}

pub(super) fn on_stage_press(host: &mut dyn FilesHost, sid: &str, path: &str, tx: &mut Tx) {
    toggle_play(host, sid, path, tx);
}

pub(super) fn on_stage_pointer_move(host: &mut dyn FilesHost, sid: &str, path: &str, tx: &mut Tx) {
    let now = tx.clock();
    let Some(v) = video_ref(host, sid, path) else {
        tx.live_only();
        return;
    };
    if v.step_overlay(Event::Moved { now }, tx) {
        v.sync_time_update(now, tx);
        v.presentation_changed();
        host.touch_files();
        tx.animated();
    } else {
        tx.live_only();
    }
}

pub(super) fn on_stage_hover_change(host: &mut dyn FilesHost, sid: &str, path: &str, inside: bool, tx: &mut Tx) {
    if !inside
        && let Some(v) = video_ref(host, sid, path)
        && v.step_overlay(Event::Left, tx)
    {
        v.sync_time_update(tx.clock(), tx);
        v.presentation_changed();
        host.touch_files();
        tx.animated();
    } else {
        tx.live_only();
    }
}

pub(super) fn on_button_press(host: &mut dyn FilesHost, sid: &str, path: &str, press: fn(&mut FileVideo), tx: &mut Tx) {
    if let Some(v) = video_mut(host, sid, path) {
        let was_playing = v.player.is_playing();
        press(v);
        let now = tx.clock();
        let paused = !v.player.is_playing();
        let mut vis_changed = v.step_overlay(Event::Playback { paused, now }, tx);
        if was_playing && !paused {
            vis_changed |= v.step_overlay(Event::Moved { now }, tx);
        }
        v.sync_time_update(now, tx);
        v.presentation_changed();
        if vis_changed {
            tx.animated();
        } else {
            tx.instant();
        }
    }
}

pub(super) fn on_timeline_drag(host: &mut dyn FilesHost, sid: &str, path: &str, d: &Drag, tx: &mut Tx) {
    let Some(v) = video_mut(host, sid, path) else { return };
    let duration = v.player.duration().unwrap_or(0.0);
    if !duration.is_finite() || duration <= 0.0 {
        tx.live_only();
        return;
    }
    let track = (d.w - 2.0 * TRACK_INSET).max(1.0);
    let at = ((d.x - TRACK_INSET) / track).clamp(0.0, 1.0) as f64 * duration;
    let event = match d.phase {
        Phase::Down => ScrubEvent::Down { at, origin: v.player.time(), playing: v.player.is_playing() },
        Phase::Move => ScrubEvent::Move { at },
        Phase::Up => ScrubEvent::Up { at, cancelled: d.cancelled() },
    };
    let effects = step_scrub(&mut v.scrub, event);
    if effects.pause {
        v.player.pause();
    }
    match effects.seek {
        Some(SeekKind::Fast(to)) => v.player.seek_fast(to),
        Some(SeekKind::Exact(to)) => v.player.seek(to),
        None => {}
    }
    if effects.play {
        v.player.play();
    }
    let now = tx.clock();
    if d.phase == Phase::Up {
        let paused = !v.player.is_playing();
        v.step_overlay(Event::Playback { paused, now }, tx);
        if !paused {
            v.step_overlay(Event::Moved { now }, tx);
        }
    }
    v.sync_time_update(now, tx);
    v.presentation_changed();
    tx.instant();
}
