//! First-quit notice that the OpenCode background service keeps running, plus the quit-policy
//! check that stops the local service when "Keep running in the background" is turned off
//! (packages/desktop/src/main/lifecycle/background-notice-machine.ts, background-notice.ts,
//! packages/desktop/src/main/service/keep-running.ts, lifecycle/machine.ts:107, opencode#66499).

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use opencode_i18n::t;
use opencode_kernel::{Feature, KernelBuilder, OverlayRegistration, Slot};
use opencode_ui::palette::{Elevate as _, Pal, kit};
use opencode_ui::widgets::{self, DialogSize, Fit, dialog_window, sized};
use rune::{Cx, El, Key, KeyEvent, TextStyle, Tx, col, row, spacer, text};
use rune_kit::ButtonVariant;

use crate::App;

/// Overlay stack identifier for the first-quit background-service notice.
pub const OVERLAY_ID: &str = "dialog.background_notice";

/// dialog-title: 15 on 20 at 530, -0.13 tracking.
const TITLE: TextStyle = TextStyle::new(15.0, 530.0).lh(20.0).tracking(-0.13);

/// Explicit state machine matching packages/desktop/src/main/lifecycle/background-notice-machine.ts:
/// Pending until shown, Showing while its dialog is open, Done once answered or skipped
/// because the user already disabled keeping the service running.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum State {
    #[default]
    Pending,
    Showing,
    Done,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Quit,
    Settings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// A quit the user started; keep_running is the setting as it stands.
    QuitRequested { keep_running: bool },
    /// The notice closed with the user choice.
    DialogClosed { choice: Choice },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Show,
    PersistShown,
    Quit,
    OpenSettings,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transition {
    /// True when the quit that raised the event waits; the app quits again via Command::Quit.
    pub holds_quit: bool,
    pub commands: Vec<Command>,
}

/// Pure transition function (background-notice-machine.ts:24-46).
pub fn step(state: &mut State, event: Event) -> Transition {
    match event {
        Event::QuitRequested { keep_running } => match *state {
            State::Showing => Transition { holds_quit: true, commands: Vec::new() },
            State::Done => Transition { holds_quit: false, commands: Vec::new() },
            // Someone who already turned the service off has nothing to be told.
            State::Pending if !keep_running => {
                *state = State::Done;
                Transition { holds_quit: false, commands: vec![Command::PersistShown] }
            }
            State::Pending => {
                *state = State::Showing;
                Transition { holds_quit: true, commands: vec![Command::Show] }
            }
        },
        Event::DialogClosed { choice } => {
            if *state != State::Showing {
                return Transition { holds_quit: false, commands: Vec::new() };
            }
            *state = State::Done;
            let next = match choice {
                Choice::Settings => Command::OpenSettings,
                Choice::Quit => Command::Quit,
            };
            Transition { holds_quit: false, commands: vec![Command::PersistShown, next] }
        }
    }
}

/// Why the process is exiting (endsApp in packages/desktop/src/main/lifecycle/machine.ts:107):
/// only a plain user quit may stop the background service, never a relaunch or an update handoff.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
enum ExitKind {
    #[default]
    Quit = 0,
    Relaunch = 1,
}

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
static EXIT_KIND: AtomicU8 = AtomicU8::new(ExitKind::Quit as u8);
static KEEP_SERVICE_RUNNING: AtomicBool = AtomicBool::new(true);

/// Records that the upcoming exit is a relaunch (App/Restart, updater restart, trace relaunch).
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub fn mark_relaunch() {
    EXIT_KIND.store(ExitKind::Relaunch as u8, Ordering::Release);
}

/// Records the latest persisted value of "Keep running in the background" for quit_last.
pub fn sync_keep_running(enabled: bool) {
    KEEP_SERVICE_RUNNING.store(enabled, Ordering::Release);
}

/// True only for a user quit — not a relaunch or an update handoff (endsApp, machine.ts:107).
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub fn ends_app() -> bool {
    if EXIT_KIND.load(Ordering::Acquire) != ExitKind::Quit as u8 {
        return false;
    }
    #[cfg(not(target_arch = "wasm32"))]
    if crate::updater::installs_on_quit() || crate::updater::handoff_quit() {
        return false;
    }
    true
}

/// Whether quit_last should stop the local background service (background-service.ts:51-53).
#[cfg(not(target_arch = "wasm32"))]
pub fn should_stop_service_on_quit() -> bool {
    ends_app() && !KEEP_SERVICE_RUNNING.load(Ordering::Acquire)
}

#[derive(Default)]
struct NoticeSlot {
    state: State,
}

static NOTICE: Slot<NoticeSlot> = Slot::new();

fn sync_slot(s: &mut App) {
    let shown = s.settings.background_notice_shown;
    let slot = s.kernel.slots.get_mut(&NOTICE);
    if shown && slot.state == State::Pending {
        slot.state = State::Done;
    }
}

fn dispatch(s: &mut App, event: Event, tx: &mut Tx) -> bool {
    sync_slot(s);
    let tr = {
        let slot = s.kernel.slots.get_mut(&NOTICE);
        step(&mut slot.state, event)
    };
    let mut should_quit = false;
    let mut should_open_settings = false;
    for cmd in tr.commands {
        match cmd {
            Command::Show => {
                tx.instant();
            }
            Command::PersistShown => {
                s.change_settings(|st| st.background_notice_shown = true);
            }
            Command::Quit => {
                should_quit = true;
            }
            Command::OpenSettings => {
                should_open_settings = true;
            }
        }
    }
    if should_open_settings {
        tx.instant();
        let entry = crate::settings::search::index(s)
            .into_iter()
            .find(|e| e.target.as_deref() == Some("settings-keep-service-running"));
        match entry {
            Some(e) => crate::settings::search::reveal(s, &e, tx),
            None => s.open_settings_page(crate::settings::pages::Page::General, tx),
        }
    } else if should_quit {
        tx.quit();
    }
    tr.holds_quit
}

/// Called when the user initiates a quit (e.g. closing the last window on Windows/Linux).
/// Returns true when the quit must wait while the one-time first-quit notice is shown.
#[cfg(not(target_arch = "wasm32"))]
pub fn holds_quit(s: &mut App, tx: &mut Tx) -> bool {
    if crate::headless()
        || crate::flag("--launch-trace")
        || crate::flag("--fixture")
        || crate::demo::requested()
        || rune::quitting()
        || !ends_app()
    {
        return false;
    }
    let keep = s.settings.keep_service_running;
    sync_keep_running(keep);
    dispatch(s, Event::QuitRequested { keep_running: keep }, tx)
}

/// Syncs keep_service_running and marks the notice done on quit if the user already turned
/// off keeping the service running (e.g. macOS Cmd+Q before ever seeing the notice).
pub fn on_quit(s: &mut App) {
    let keep = s.settings.keep_service_running;
    sync_keep_running(keep);
    if !keep && !s.settings.background_notice_shown {
        s.change_settings(|st| st.background_notice_shown = true);
    }
}

/// True while the first-quit notice overlay is visible in this window.
pub fn is_open(s: &App) -> bool {
    s.kernel.slots.get(&NOTICE).state == State::Showing
}

fn choose(s: &mut App, choice: Choice, tx: &mut Tx) {
    tx.instant();
    let _ = dispatch(s, Event::DialogClosed { choice }, tx);
}

fn key(s: &mut App, ev: &KeyEvent, tx: &mut Tx) -> bool {
    if !is_open(s) {
        return false;
    }
    match ev.key {
        Key::Enter => {
            choose(s, Choice::Quit, tx);
            true
        }
        Key::Escape => {
            // Matches Electron cancelId: 0 (Quit) in background-notice.ts:41-42.
            choose(s, Choice::Quit, tx);
            true
        }
        _ => false,
    }
}

fn view(s: &App, p: &Pal, cx: &Cx<App>) -> Vec<El> {
    if !is_open(s) {
        return Vec::new();
    }
    let title = t!("desktop.backgroundNotice.message");
    let detail = t!("desktop.backgroundNotice.detail");
    let k = kit(p);
    let open_settings = widgets::button(t!("desktop.backgroundNotice.openSettings"), ButtonVariant::Neutral)
        .on_press_tx(|s: &mut App, tx: &mut Tx| choose(s, Choice::Settings, tx))
        .build(k)
        .key("background-notice-settings");
    let quit = widgets::button(t!("desktop.backgroundNotice.quit"), ButtonVariant::Contrast)
        .on_press_tx(|s: &mut App, tx: &mut Tx| choose(s, Choice::Quit, tx))
        .build(k)
        .key("background-notice-quit")
        .autofocus();
    let inner = col()
        .w_full()
        .clip_rounded(12.0)
        .child(
            col()
                .key("background-notice-header")
                .w_full()
                .padding(16.0)
                .gap(8.0)
                .child(text(title).font(TITLE).color(p.text).wrap().w_full())
                .child(text(detail).font(widgets::BODY).color(p.muted).wrap().w_full()),
        )
        .child(
            row()
                .key("background-notice-footer")
                .w_full()
                .padding(16.0)
                .gap(8.0)
                .child(spacer())
                .child(open_settings)
                .child(quit),
        );
    let card = col()
        .key("background-notice-card")
        .role("dialog")
        .label(title)
        .radius(12.0)
        .fill(p.layer1)
        .elevate(p.overlay)
        .modal_tx(true, |s: &mut App, tx: &mut Tx| choose(s, Choice::Quit, tx))
        .child(inner);
    let card = sized(card, DialogSize::Normal, Fit::Content, dialog_window(cx, *s.zoom));
    widgets::modal("background-notice", p, card, None)
}

/// Kernel feature registration for the first-quit background-service notice overlay.
pub struct BackgroundNoticeFeature;

impl Feature<App> for BackgroundNoticeFeature {
    fn id(&self) -> &'static str {
        "feature.background_notice"
    }

    fn register(&self, builder: &mut KernelBuilder<App>) {
        builder.register_slot(&NOTICE);
        builder.register_overlay(
            OverlayRegistration::<App>::new(OVERLAY_ID)
                .with_render(crate::overlays::layer::ELEVATED_DIALOG, |s, cx| {
                    let p = crate::palette::pal(s.dark(cx));
                    view(s, p, cx)
                })
                .with_key(key)
                .with_dismiss(|s, tx| choose(s, Choice::Quit, tx)),
        );
    }
}

/// Shows the notice directly for --scene background-notice visual and interactive verification.
fn preview_notice(s: &mut App) {
    crate::open_scene(s);
    s.finish_stream();
    s.settings.keep_service_running = true;
    s.settings.background_notice_shown = false;
    let slot = s.kernel.slots.get_mut(&NOTICE);
    slot.state = State::Pending;
    let _ = dispatch(s, Event::QuitRequested { keep_running: true }, &mut Tx::default());
}

rune_register::register!(crate::scenes::SCENE_LIST: crate::scenes::Scene = [
    ("background-notice", preview_notice),
]);
