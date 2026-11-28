//! Stage layout and Chromium-style playback controls for a video open in the Files panel.

use opencode_ui::palette::{Pal, kit};
use rune::{
    Anim, Color, Curve, Easing, El, Family, Icon, Key, Shadow, SurfaceFit, TextStyle, Tx, el, icon,
    row, text,
};
use rune_theme_opencode::icons as ic;

use super::{
    FileVideo, Scrub, TRACK_INSET, on_button_press, on_stage_hover_change, on_stage_pointer_move,
    on_stage_press, on_timeline_drag, toggle_play,
};
use crate::FilesHost;

/// Chromium's controls: 48 pt buttons over a 24 pt timeline, a 112 pt scrim under them.
const BUTTON: f32 = 48.0;
const TIMELINE: f32 = 24.0;
const SCRIM: f32 = 112.0;
/// The timeline's track: 4 pt thick.
const TRACK: f32 = 4.0;
/// `--internal-media-controls-button-hover-background`: a 36 pt disc.
const HOVER_DISC: f32 = 36.0;
/// The time: Roboto at 14 px, white, on the buttons' 48 pt line.
const TIME: TextStyle = TextStyle::new(14.0, 400.0)
    .family(Family::Named("Roboto-Regular, Roboto, Arial"))
    .lh(48.0);

/// The stage for `v`, filling the panel under the toolbar.
pub fn stage<H: FilesHost + 'static>(s: &H, p: &Pal, session_id: &str, path: &str, v: &FileVideo) -> El {
    let [w, h] = s.files().stage_size.get();
    let (nw, nh) = v.player.natural_size().unwrap_or((16, 9));
    // `w-full` in the `p-6` frame, its height from the clip's shape, at most the frame's
    // (`max-height: 100%` with `object-fit: contain`).
    let box_w = (w - 48.0).max(0.0);
    let box_h = (box_w * nh as f32 / nw.max(1) as f32).min((h - 48.0).max(0.0)).round();
    let shown = v.controls_shown();
    let loading = v.is_loading();
    let (press_sid, press_path) = (session_id.to_owned(), path.to_owned());
    let picture = el()
        .key("artifact-media")
        .w(box_w)
        .h(box_h)
        .radius(4.0)
        .clip_rounded(4.0)
        .fill(Color::BLACK)
        .box_shadow(Pal::ring_out(p.border))
        .box_shadow(Shadow {
            y: 12.0,
            blur: 32.0,
            spread: -12.0,
            color: p.text.alpha(0.25),
            ..Shadow::default()
        })
        .on_press_tx(move |s: &mut H, tx: &mut Tx| on_stage_press(s, &press_sid, &press_path, tx))
        .video(&v.player, SurfaceFit::Fit)
        .children((!loading).then(|| controls::<H>(session_id, path, v, box_w, box_h, shown)));
    let mut expand = expand_button(p, shown).absolute(box_w - 96.0, box_h - 72.0);
    if !shown {
        expand = expand.inert();
    }
    let frame = el()
        .key("artifact-video-frame")
        .w(box_w)
        .h(box_h)
        .child(picture)
        .child(expand);
    let sizer = s.files().stage_size.clone();
    let (move_sid, move_path) = (session_id.to_owned(), path.to_owned());
    let (hover_sid, hover_path) = (session_id.to_owned(), path.to_owned());
    el().key("artifact-stage")
        .grow(1.0)
        .min_h(0.0)
        .w_full()
        .fill(p.layer1)
        .center()
        .on_layout_tx(move |s: &mut H, [_, _, w, h], tx| {
            let prev = sizer.get();
            if (prev[0] - w).abs() > 0.5 || (prev[1] - h).abs() > 0.5 {
                sizer.set([w, h]);
                s.touch_files();
                tx.instant();
            } else {
                tx.live_only();
            }
        })
        .on_pointer_move_local(move |s: &mut H, _, tx| on_stage_pointer_move(s, &move_sid, &move_path, tx))
        .on_hover_change(move |s: &mut H, inside, tx| on_stage_hover_change(s, &hover_sid, &hover_path, inside, tx))
        .child(frame)
        .children(loading.then(|| {
            // `MediaLoading`: Desktop's Loader at 16 pt in `text-weak`, over the stage.
            el().pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
                .center()
                .inert()
                .child(rune_kit::Loader::new().size(16.0).build(kit(p)))
        }))
}

/// Chromium's controls over a `w` x `h` clip: the scrim, the button row and the timeline. They
/// fade out over 1 s when the clip plays on with the pointer resting, and back in at once.
fn controls<H: FilesHost + 'static>(sid: &str, path: &str, v: &FileVideo, w: f32, h: f32, shown: bool) -> El {
    let player = &v.player;
    let duration = player.duration().unwrap_or(0.0);
    let time = match v.scrub {
        Scrub::Dragging { at, .. } => at,
        Scrub::Idle => player.time(),
    };
    let playing = player.is_playing();
    let fade = if shown { Curve::tween(0.25, Easing::EASE) } else { Curve::tween(1.0, Easing::EASE) };
    let scrim = el()
        .pin(Some(0.0), None, Some(0.0), Some(0.0))
        .h(SCRIM.min(h))
        .inert()
        .linear_gradient(std::f32::consts::PI, SCRIM_STOPS.map(|(at, a)| (at, Color::BLACK.alpha(a))));
    let buttons = row()
        .key("media-buttons")
        .pin(Some(0.0), None, Some(0.0), Some(TIMELINE))
        .h(BUTTON)
        .center_y()
        .child(button::<H>(
            sid,
            path,
            "media-play",
            if playing { &PAUSE } else { &PLAY },
            if playing { "Pause" } else { "Play" },
            |v| {
                if v.player.is_playing() { v.player.pause() } else { v.player.play() }
            },
        ))
        .child(text(clock(time)).font(TIME).color(Color::WHITE))
        .child(text(format!("/ {}", clock(duration))).font(TIME).color(Color::WHITE).ml(4.0))
        .child(el().grow(1.0))
        // A clip with no sound track greys its mute button, as Chromium disables it.
        .child(match player.has_sound() {
            Some(false) => disabled_button("media-mute", &SOUND, "Mute"),
            _ => button::<H>(
                sid,
                path,
                "media-mute",
                if player.muted() { &MUTED } else { &SOUND },
                if player.muted() { "Unmute" } else { "Mute" },
                |v| v.player.set_muted(!v.player.muted()),
            ),
        })
        // Chromium's fullscreen button keeps its place, hidden; Desktop's expand button lies over it.
        .child(el().w(BUTTON).h(BUTTON))
        .child(button::<H>(sid, path, "media-more", &MORE, "Show more media controls", |_| {}));
    let bar = el()
        .key("media-controls")
        .w_full()
        .h(SCRIM.min(h))
        .group_opacity(if shown { 1.0 } else { 0.0 }, 1.0)
        .opacity_motion(Anim::new(fade))
        .child(scrim)
        .child(buttons)
        .child(timeline::<H>(sid, path, v, w, time, duration));
    el().key("media-controls-group")
        .pin(Some(0.0), None, Some(0.0), Some(0.0))
        .h(SCRIM.min(h))
        .hover_group_focus()
        .hover_motion(Anim::new(Curve::tween(0.25, Easing::EASE)), Anim::new(Curve::tween(1.0, Easing::EASE)))
        .child(bar)
}

/// The scrim's stops, bottom (1) to top (0): Chromium's eased black.
const SCRIM_STOPS: [(f32, f32); 16] = [
    (0.0, 0.0),
    (0.081, 0.01),
    (0.155, 0.047),
    (0.225, 0.106),
    (0.29, 0.176),
    (0.353, 0.26),
    (0.412, 0.353),
    (0.471, 0.45),
    (0.529, 0.55),
    (0.588, 0.647),
    (0.647, 0.74),
    (0.71, 0.824),
    (0.775, 0.894),
    (0.845, 0.953),
    (0.919, 0.99),
    (1.0, 1.0),
];

/// The hover disc's `rgba(32, 33, 36, 0.71)`.
fn hover() -> Color {
    rune::color::hex("#202124").unwrap_or(Color::BLACK).alpha(0.71)
}

/// `m:ss`, or `h:mm:ss` from an hour, as Chromium writes media times.
fn clock(seconds: f64) -> String {
    let t = if seconds.is_finite() { seconds.max(0.0) as u64 } else { 0 };
    let (h, m, s) = (t / 3600, t / 60 % 60, t % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}

fn button_shell(key: &'static str, glyph: &'static Icon, label: &'static str) -> El {
    el().key(key)
        .size(BUTTON, BUTTON)
        .center()
        .role("button")
        .label(label)
        .child(
            el().size(HOVER_DISC, HOVER_DISC)
                .radius(HOVER_DISC / 2.0)
                .hover(hover())
                .hover_motion(
                    Anim::new(Curve::tween(0.25, Easing::EASE)),
                    Anim::new(Curve::tween(0.25, Easing::EASE)),
                )
                .center()
                .child(icon(glyph, 20.0).color(Color::WHITE)),
        )
}

/// A 48 pt control with its 20 pt glyph, white, and the hover disc behind it.
fn button<H: FilesHost + 'static>(
    sid: &str,
    path: &str,
    key: &'static str,
    glyph: &'static Icon,
    label: &'static str,
    press: fn(&mut FileVideo),
) -> El {
    let (sid, path) = (sid.to_owned(), path.to_owned());
    button_shell(key, glyph, label)
        .on_press_tx(move |s: &mut H, tx: &mut Tx| on_button_press(s, &sid, &path, press, tx))
}

/// A control Chromium disables (`:disabled`): its glyph at 40 % opacity, no hover disc, no press.
/// Rune composites a light glyph on dark brighter than Chromium's opacity does, so 0.32 here lands
/// on Chromium's pixels (158 against 161 on the scrim at 1.5x).
fn disabled_button(key: &'static str, glyph: &'static Icon, label: &'static str) -> El {
    el().key(key)
        .size(BUTTON, BUTTON)
        .center()
        .role("button")
        .label(label)
        .child(icon(glyph, 20.0).color(Color::WHITE).opacity(0.32))
}

fn timeline_shell(track: f32, played: f32, scrubbing: bool) -> El {
    el().key("media-timeline")
        .pin(Some(0.0), None, Some(0.0), Some(0.0))
        .h(TIMELINE)
        .role("slider")
        .label("Video time scrubber")
        .child(
            el().absolute(TRACK_INSET, 0.0)
                .w(track)
                .h(TRACK)
                .fill(Color::WHITE.alpha(0.3))
                .child(el().absolute(0.0, 0.0).w(track).h(TRACK).fill(Color::WHITE.alpha(0.54)))
                .child(el().absolute(0.0, 0.0).w(played).h(TRACK).fill(Color::WHITE))
                .child(
                    el().absolute(played - 6.0, -4.0)
                        .size(12.0, 12.0)
                        .radius(6.0)
                        .fill(Color::WHITE)
                        .opacity(if scrubbing { 1.0 } else { 0.0 }),
                ),
        )
}

/// The timeline: the track 16 pt in from each side along the top of the bottom 24 pt, white
/// where it played, 54 % white what's loaded (all of it, from a file), 30 % white the rest.
/// A drag scrubs, and the clip plays on from where it's let go if it was playing.
fn timeline<H: FilesHost + 'static>(sid: &str, path: &str, v: &FileVideo, w: f32, time: f64, duration: f64) -> El {
    let track = (w - 2.0 * TRACK_INSET).max(0.0);
    let played = if duration > 0.0 {
        (time / duration).clamp(0.0, 1.0) as f32 * track
    } else {
        0.0
    };
    let scrubbing = v.scrub != Scrub::Idle;
    let (drag_sid, drag_path) = (sid.to_owned(), path.to_owned());
    let (key_sid, key_path) = (sid.to_owned(), path.to_owned());
    timeline_shell(track, played, scrubbing)
        .focusable(true)
        .on_drag(move |s: &mut H, d, tx: &mut Tx| on_timeline_drag(s, &drag_sid, &drag_path, d, tx))
        .on_key(move |s: &mut H, key, tx: &mut Tx| {
            key.mods == rune::Mods::default()
                && matches!(&key.key, Key::Text(t) if t == " ")
                && toggle_play(s, &key_sid, &key_path, tx)
        })
}

/// Desktop's expand button (`artifact-video-fullscreen`): a 48 pt hit area 96 in from the clip's
/// right and 72 up from its bottom, where Chromium's fullscreen button sits, with a 36 pt disc
/// that darkens to `rgb(0 0 0 / 0.55)` under the pointer and the white `outline-expand` glyph.
fn expand_button(p: &Pal, shown: bool) -> El {
    let _ = p;
    el().key("artifact-video-fullscreen")
        .size(BUTTON, BUTTON)
        .center()
        .opacity(if shown { 1.0 } else { 0.0 })
        .role("button")
        .label_static(opencode_i18n::t!("plugin.file.view.fullscreen"))
        .help(opencode_i18n::t!("plugin.file.view.fullscreen").to_owned())
        .child(
            el().size(HOVER_DISC, HOVER_DISC)
                .radius(HOVER_DISC / 2.0)
                .hover(Color::BLACK.alpha(0.55))
                .center()
                .child(icon(&ic::OUTLINE_EXPAND, 20.0).color(Color::WHITE)),
        )
}

static PLAY: Icon = Icon {
    name: "media-play",
    view: 24.0,
    body: r#"<path d="M8 5v14l11-7z" fill="currentColor"/>"#,
};
static PAUSE: Icon = Icon {
    name: "media-pause",
    view: 24.0,
    body: r#"<path d="M6 19h4V5H6v14zm8-14v14h4V5h-4z" fill="currentColor"/>"#,
};
static SOUND: Icon = Icon {
    name: "media-sound",
    view: 24.0,
    body: r#"<path d="M3 9v6h4l5 5V4L7 9H3zm13.5 3c0-1.77-1.02-3.29-2.5-4.03v8.05c1.48-.73 2.5-2.25 2.5-4.02zM14 3.23v2.06c2.89.86 5 3.54 5 6.71s-2.11 5.85-5 6.71v2.06c4.01-.91 7-4.49 7-8.77s-2.99-7.86-7-8.77z" fill="currentColor"/>"#,
};
static MUTED: Icon = Icon {
    name: "media-muted",
    view: 24.0,
    body: r#"<path d="M16.5 12c0-1.77-1.02-3.29-2.5-4.03v2.21l2.45 2.45c.03-.2.05-.41.05-.63zm2.5 0c0 .94-.2 1.82-.54 2.64l1.51 1.51C20.63 14.91 21 13.5 21 12c0-4.28-2.99-7.86-7-8.77v2.06c2.89.86 5 3.54 5 6.71zM4.27 3L3 4.27 7.73 9H3v6h4l5 5v-6.73l4.25 4.25c-.67.52-1.42.93-2.25 1.18v2.06c1.38-.31 2.63-.95 3.69-1.81L19.73 21 21 19.73l-9-9L4.27 3zM12 4L9.91 6.09 12 8.18V4z" fill="currentColor"/>"#,
};
static MORE: Icon = Icon {
    name: "media-more",
    view: 24.0,
    body: r#"<circle cx="12" cy="6" r="2" fill="currentColor"/><circle cx="12" cy="12" r="2" fill="currentColor"/><circle cx="12" cy="18" r="2" fill="currentColor"/>"#,
};

