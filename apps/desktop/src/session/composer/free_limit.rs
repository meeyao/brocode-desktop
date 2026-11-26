//! Free limit reached panel (`packages/app/src/session/composer/free-limit-panel.tsx`,
//! `anomalyco/opencode#63531`).
//!
//! Free usage on OpenCode Zen is counted per network, not per session, so one
//! `provider.free-limit` refusal holds across sessions and restarts until its reset timestamp.
//! While active and no paid provider is connected, this panel replaces the session composer;
//! connecting a paid provider or passing the reset timestamp brings the composer back on its own.

use opencode_ui::focus::FocusExt;
use std::time::Duration;

use opencode_i18n::t;
use opencode_kernel::{Feature, KernelBuilder, Slot};
use opencode_ui::palette::{Elevate as _, Pal};
use opencode_ui::style::{UI, UI_MED, dock_w};
use rune::machine::Effects;
use rune::{El, LocalTime, Tx, col, row, text};
use serde_json::{Value, json};
use session_view::composer::{RADIUS, composer_fill, submit_surface};
use session_view::turn_errors::{date_time, short_time};

use crate::{App, Route};

const STORAGE_NS: &str = "opencode.global.dat";
const STORAGE_KEY: &str = "free-limit";
const GO_URL: &str = "https://apibrother.com";
/// OpenCode Zen always says when free usage resets; covers a refusal without a delay or out of range.
const FALLBACK_RESET_MS: i64 = 3_600_000;
/// `MAX_RETRY_AFTER_MS` (`usage-exceeded-dialogs.tsx:21`): 45 days.
const MAX_RETRY_AFTER_MS: i64 = 45 * 86_400_000;

pub(crate) static FREE_LIMIT: Slot<FreeLimit> = Slot::new();

/// Explicit free-limit lifecycle: `Unloaded` until storage hydrates, `Clear` when inactive or
/// expired, and `Active` while holding a future reset timestamp (or `None` only in stories showing
/// the unknown-reset fallback copy).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum State {
    #[default]
    Unloaded,
    Clear,
    Active { reset_at: Option<i64> },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FreeLimit {
    pub state: State,
}

pub enum Event {
    Hydrated { reset_at: Option<i64>, now_ms: i64 },
    LimitHit { created_ms: i64, retry_after_ms: Option<i64> },
    Tick { now_ms: i64 },
    /// Used by scenes/stories to force a specific panel state without persisting.
    Seed(State),
}

#[derive(Debug, PartialEq, Eq)]
pub enum Effect {
    Persist(Option<i64>),
}

/// When a usage limit resets (`usage-exceeded-dialogs.tsx:26-41`), or `None` when `retry_after_ms`
/// is missing or out of `[0, 45 days]`.
pub fn limit_reset_at(created_ms: i64, retry_after_ms: Option<i64>) -> Option<i64> {
    let delay = retry_after_ms?;
    if created_ms <= 0 || !(0..=MAX_RETRY_AFTER_MS).contains(&delay) {
        return None;
    }
    created_ms.checked_add(delay)
}

pub fn step(state: &mut State, event: Event) -> Effects<Effect> {
    match (*state, event) {
        (State::Unloaded, Event::Hydrated { reset_at, now_ms }) => {
            *state = match reset_at {
                Some(at) if at > now_ms => State::Active { reset_at: Some(at) },
                _ => State::Clear,
            };
            Effects::none()
        }
        (_, Event::Hydrated { .. }) => Effects::none(),
        (_, Event::LimitHit { created_ms, retry_after_ms }) => {
            let at = limit_reset_at(created_ms, retry_after_ms)
                .unwrap_or_else(|| created_ms.saturating_add(FALLBACK_RESET_MS));
            *state = State::Active { reset_at: Some(at) };
            Effects::one(Effect::Persist(Some(at)))
        }
        (State::Active { reset_at: Some(at) }, Event::Tick { now_ms }) if now_ms >= at => {
            *state = State::Clear;
            Effects::one(Effect::Persist(None))
        }
        (_, Event::Tick { .. }) => Effects::none(),
        (_, Event::Seed(next)) => {
            *state = next;
            Effects::none()
        }
    }
}

fn dispatch(s: &mut App, event: Event) -> bool {
    let before = s.kernel.slots.get(&FREE_LIMIT).state;
    let effects = step(&mut s.kernel.slots.get_mut(&FREE_LIMIT).state, event);
    let changed = s.kernel.slots.get(&FREE_LIMIT).state != before;
    for effect in effects {
        match effect {
            Effect::Persist(reset_at) if s.persists() => {
                crate::storage::set_json(STORAGE_NS, STORAGE_KEY, &json!({ "resetAt": reset_at }));
            }
            Effect::Persist(_) => {}
        }
    }
    changed
}

/// Records a `provider.free-limit` step failure from any session (`free-limit-panel.tsx:28-34`).
pub(crate) fn on_step_failed(s: &mut App, data: &Value) -> bool {
    if data["error"]["type"].as_str() != Some("provider.free-limit") {
        return false;
    }
    let created_ms = data["eventCreated"].as_i64().unwrap_or(*s.now_ms);
    let retry_after_ms = data["error"]["retryAfterMs"]
        .as_i64()
        .or_else(|| data["error"]["retryAfterMs"].as_f64().filter(|x| x.is_finite() && *x >= 0.0).map(|x| x as i64));
    let changed = dispatch(s, Event::LimitHit { created_ms, retry_after_ms });
    if replaces_composer(s) {
        s.composer.focused = false;
    }
    changed
}

/// Whether the free-tier limit currently holds (unexpired and no paid provider connected).
pub(crate) fn active(s: &App) -> bool {
    let State::Active { reset_at } = s.kernel.slots.get(&FREE_LIMIT).state else { return false };
    if s.composer_ext.pickers.has_paid_provider() {
        return false;
    }
    reset_at.is_none_or(|at| at > *s.now_ms)
}

/// Whether the free limit panel stands in place of the session composer (`region.tsx:613-620`).
pub(crate) fn replaces_composer(s: &App) -> bool {
    matches!(*s.route, Route::Session { .. }) && !s.dock_shown() && active(s)
}

/// Hydrates persisted free-limit state once storage is ready and wakes once when an active limit
/// expires (zero idle wakes when inactive or when a paid provider is connected).
fn poll(s: &mut App, _now: f64, tx: &mut Tx) -> bool {
    let state = s.kernel.slots.get(&FREE_LIMIT).state;
    match state {
        State::Unloaded => {
            if s.persists() && !crate::storage::is_ready() {
                return false;
            }
            let reset_at = s
                .persists()
                .then(|| crate::storage::json(STORAGE_NS, STORAGE_KEY).and_then(|v| v["resetAt"].as_i64()))
                .flatten();
            let now_ms = *s.now_ms;
            let changed = dispatch(s, Event::Hydrated { reset_at, now_ms });
            arm_expiry(s, tx);
            changed
        }
        State::Active { reset_at: Some(at) } => {
            let now_ms = *s.now_ms;
            if now_ms >= at {
                return dispatch(s, Event::Tick { now_ms });
            }
            arm_expiry(s, tx);
            false
        }
        State::Clear | State::Active { reset_at: None } => false,
    }
}

fn arm_expiry(s: &App, tx: &mut Tx) {
    let State::Active { reset_at: Some(at) } = s.kernel.slots.get(&FREE_LIMIT).state else { return };
    if s.composer_ext.pickers.has_paid_provider() {
        return;
    }
    let remaining_ms = (at - *s.now_ms).max(0) as u64;
    // Wake once when the limit expires (capped at 1 h so wall-clock jumps still settle cleanly).
    tx.wake_after(Duration::from_millis(remaining_ms.min(3_600_000).max(50)));
}

fn formatted_reset(reset_at: Option<i64>, now_ms: i64) -> Option<String> {
    let at = reset_at?;
    let (t, today) = (LocalTime::at(at), LocalTime::at(now_ms));
    let same_day = (t.year, t.month, t.day) == (today.year, today.month, today.day);
    Some(if same_day { short_time(t) } else { date_time(t) })
}

/// `SessionFreeLimitPanel` (`free-limit-panel.tsx:93-132`): stands in for the composer while
/// free usage is spent and no paid provider is connected.
pub(crate) fn panel(s: &App, p: &Pal) -> El {
    let reset_at = match s.kernel.slots.get(&FREE_LIMIT).state {
        State::Active { reset_at } => reset_at,
        _ => None,
    };
    let description = match formatted_reset(reset_at, *s.now_ms) {
        Some(time) => t!("session.freeLimit.reset", time = time),
        None => t!("session.freeLimit.noReset").to_owned(),
    };
    let subscribe_label = t!("session.freeLimit.subscribe");
    let connect_label = t!("session.freeLimit.connect");

    let subscribe = row()
        .key("session-free-limit-subscribe")
        .role("button")
        .label_static(subscribe_label)
        .h(28.0)
        .px(11.0)
        .center()
        .radius(6.0)
        .hover(p.hover)
        .press(p.pressed)
        .focus_inset(p)
        .on_press_tx(|s: &mut App, tx: &mut Tx| {
            crate::open_in::open_external(tx, GO_URL);
            s.open_connect(Some("opencode"), false, tx);
        })
        .child(text(subscribe_label).font(UI_MED.features("\"tnum\"")).color(p.text));

    let connect = submit_surface(
        row()
            .key("session-free-limit-connect")
            .role("button")
            .label_static(connect_label)
            .h(28.0)
            .px(11.0)
            .center()
            .radius(6.0),
        p,
        true,
    )
    .on_press_tx(|s: &mut App, tx: &mut Tx| s.open_connect(None, false, tx))
    .child(text(connect_label).font(UI_MED.features("\"tnum\"")).color(p.q.submit_text));

    col()
        .key("session-free-limit")
        .role("status")
        .w_full()
        .max_w(dock_w())
        .corners([RADIUS; 4])
        .box_shadow(Pal::ring_out(p.bg_base))
        .fill(composer_fill(p))
        .elevate(p.raised)
        .padding(8.0)
        .gap(16.0)
        .child(
            col()
                .w_full()
                .gap(2.0)
                .pl(6.0)
                .pr(4.0)
                .pt(4.0)
                .child(text(t!("session.freeLimit.title")).font(UI_MED).color(p.text))
                .child(text(description).font(UI.features("\"tnum\"")).color(p.muted).wrap()),
        )
        .child(row().w_full().justify_end().center_y().gap(6.0).child(subscribe).child(connect))
}

/// Kernel feature registration for the Free Limit Reached panel.
pub struct FreeLimitFeature;

impl Feature<App> for FreeLimitFeature {
    fn id(&self) -> &'static str {
        "feature.free_limit"
    }

    fn register(&self, builder: &mut KernelBuilder<App>) {
        builder.register_slot(&FREE_LIMIT);
        builder.schedule(crate::tick::stage::INGEST_HYDRATE, "free_limit.poll", poll);
    }
}

fn scene(s: &mut App, reset_at: Option<i64>) {
    crate::scenes::open_scene(s);
    // Keep only a free OpenCode model so `has_paid_provider()` stays false in the scene.
    s.composer_ext.pickers.provider_order = vec!["opencode".into()];
    s.composer_ext.pickers.models = vec![crate::pickers::Model {
        key: "opencode:big-pickle".into(),
        id: "big-pickle".into(),
        provider: "opencode".into(),
        group: "Brocode".into(),
        name: "Big Pickle".into(),
        variants: Vec::new(),
        hidden: false,
        free: true,
        latest: false,
        inputs: Default::default(),
        tools: true,
        speed: None,
    }];
    dispatch(s, Event::Seed(State::Active { reset_at }));
}

rune_register::register!(crate::scenes::SCENE_LIST: crate::scenes::Scene = [
    ("free-limit", |s| {
        let at = *s.now_ms + 3 * 3_600_000;
        scene(s, Some(at));
    }),
    ("free-limit-tomorrow", |s| {
        let at = *s.now_ms + 28 * 3_600_000;
        scene(s, Some(at));
    }),
    ("free-limit-no-reset", |s| scene(s, None)),
]);
