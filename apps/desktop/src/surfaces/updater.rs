//! Desktop host adapter for `opencode-feature-updater` (`features/updater`).
//!
//! Keeps `build_channel`, `build_version`, and `build_commit` (including the `.rdata`
//! `OPENCODE_RUNE_BUILD_STAMP` provenance record) inside `apps/desktop` so release
//! verification reads the desktop crate's own build environment. On `wasm32`, the native
//! self-updater crate is omitted and the Luau `Updater` service reports no updater.

#[cfg(not(target_arch = "wasm32"))]
pub use opencode_feature_updater::{
    BuildInfo, StateView, Updater, UpdaterConfig, UpdaterHost, finish_quit, installs_on_quit, poll,
    poll_owed, shared_enabled,
};

#[cfg(target_arch = "wasm32")]
pub fn shared_enabled() -> bool {
    false
}

#[cfg(not(target_arch = "wasm32"))]
use opencode_kernel::{Feature, KernelBuilder};
use rune_luau::{Completer, Data};

use crate::App;

#[cfg(target_arch = "wasm32")]
const NO_UPDATER: &str = "No updater runs in this build";

#[cfg(not(target_arch = "wasm32"))]
fn state_to_data(view: StateView) -> Data {
    let fields = [
        ("phase", Data::String(view.phase.into())),
        ("version", view.version.map_or(Data::Nil, Data::String)),
        ("progress", view.progress.map_or(Data::Nil, Data::Number)),
        ("error", view.error.map_or(Data::Nil, Data::String)),
    ];
    Data::Map(fields.into_iter().filter(|(_, v)| *v != Data::Nil).map(|(k, v)| (k.to_owned(), v)).collect())
}

/// Where the updater stands, as the `UpdaterState` Luau record (`Data::Nil` when unavailable).
pub fn state_data(s: &App) -> Data {
    #[cfg(not(target_arch = "wasm32"))]
    {
        opencode_feature_updater::state_view(s).map_or(Data::Nil, state_to_data)
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = s;
        Data::Nil
    }
}

/// `Updater:CheckAsync()`: checks now and completes `done` with the `UpdaterState` record.
pub fn check_async(s: &App, done: Completer) {
    #[cfg(not(target_arch = "wasm32"))]
    opencode_feature_updater::check_with(s, move |view| {
        done.complete(view.map_or(Data::Nil, state_to_data));
    });
    #[cfg(target_arch = "wasm32")]
    {
        let _ = s;
        done.complete(Data::Map(
            [
                ("phase".to_owned(), Data::String("error".into())),
                ("error".to_owned(), Data::String(NO_UPDATER.into())),
            ]
            .into_iter()
            .collect(),
        ));
    }
}

/// `Updater:InstallAsync()`: restarts into the ready version or opens a required download;
/// completes `done` with `Data::Nil` on hand-off or an error string.
pub fn install_async(s: &App, done: Completer) {
    #[cfg(not(target_arch = "wasm32"))]
    opencode_feature_updater::install_with(s, move |err| {
        done.complete(err.map_or(Data::Nil, Data::String));
    });
    #[cfg(target_arch = "wasm32")]
    {
        let _ = s;
        done.complete(Data::String(NO_UPDATER.into()));
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn sanitize_version(raw: &str) -> String {
    raw.trim().chars().filter(|c| !c.is_control()).take(64).collect()
}

#[cfg(not(target_arch = "wasm32"))]
struct DialogGuard;

#[cfg(not(target_arch = "wasm32"))]
impl Drop for DialogGuard {
    fn drop(&mut self) {
        CHECK_DIALOG_ACTIVE.store(false, std::sync::atomic::Ordering::Release);
    }
}

/// Checks for updates from the app menu on non-beta channels and reports the outcome in a native
/// OS dialog, matching official (`packages/gui-plugins/src/updater/main.ts:69-128`).
pub fn check_with_dialog(s: &mut App) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::sync::atomic::Ordering;
        use opencode_feature_updater::dialog;

        if opencode_feature_updater::is_suppressed() {
            return;
        }
        if CHECK_DIALOG_ACTIVE.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_err() {
            return;
        }
        let guard = DialogGuard;
        let lang = opencode_i18n::locale().id();
        let msg = |key: &str, fallback: &str| {
            crate::plugin_host::sources::builtin_message("updater", lang, key).unwrap_or(fallback).to_owned()
        };
        let err_title = msg("dialog.checkFailed.title", "Update Error");
        let err_msg = msg("dialog.checkFailed.message", "Update check failed.");
        let ok_title = msg("dialog.upToDate.title", "No Updates");
        let ok_msg = msg("dialog.upToDate.message", "You're up to date.");
        let ready_title = msg("dialog.ready.title", "Update Ready");
        let ready_tpl = msg("dialog.ready.message", "Update {{version}} downloaded. Restart now?");
        let avail_title = msg("dialog.available.title", "Update Available");
        let avail_tpl = msg("dialog.available.message", "Brocode {{version}} is available to download.");
        let download_label = msg("action.download", "Download Brocode");
        let restart_label = msg("dialog.restart", "Restart");
        let later_label = msg("dialog.later", "Later");
        let waker = (*s.waker).clone();
        opencode_feature_updater::check_with(s, move |view| {
            let guard = guard;
            let Some(view) = view else { return };
            match view.phase {
                "error" => {
                    std::thread::Builder::new().name("opencode-update-dialog".into()).spawn(move || {
                        let _guard = guard;
                        dialog::alert(dialog::Kind::Error, &err_title, &err_msg);
                    }).ok();
                }
                "up-to-date" => {
                    std::thread::Builder::new().name("opencode-update-dialog".into()).spawn(move || {
                        let _guard = guard;
                        dialog::alert(dialog::Kind::Info, &ok_title, &ok_msg);
                    }).ok();
                }
                "ready" | "installed" => {
                    let version = sanitize_version(view.version.as_deref().unwrap_or_default());
                    let body = ready_tpl.replace("{{version}}", &version);
                    std::thread::Builder::new().name("opencode-update-dialog".into()).spawn(move || {
                        let _guard = guard;
                        if dialog::confirm(&ready_title, &body, &restart_label, &later_label) {
                            PENDING_DIALOG_INSTALL.store(true, Ordering::Release);
                            if let Some(w) = waker {
                                w();
                            }
                        }
                    }).ok();
                }
                "download-required" => {
                    let version = sanitize_version(view.version.as_deref().unwrap_or_default());
                    let body = avail_tpl.replace("{{version}}", &version);
                    std::thread::Builder::new().name("opencode-update-dialog".into()).spawn(move || {
                        let _guard = guard;
                        if dialog::confirm(&avail_title, &body, &download_label, &later_label) {
                            PENDING_DIALOG_INSTALL.store(true, Ordering::Release);
                            if let Some(w) = waker {
                                w();
                            }
                        }
                    }).ok();
                }
                _ => {}
            }
        });
    }
    #[cfg(target_arch = "wasm32")]
    let _ = s;
}

#[cfg(not(target_arch = "wasm32"))]
static CHECK_DIALOG_ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(not(target_arch = "wasm32"))]
static PENDING_DIALOG_INSTALL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(not(target_arch = "wasm32"))]
thread_local! {
    static INSTALL_ON_QUIT_STORE_VER: std::cell::Cell<u64> = const { std::cell::Cell::new(u64::MAX) };
}

/// Syncs `plugin.updater` / `installOnQuit` (`{"enabled": bool}`, default `false`) into
/// `opencode_feature_updater::set_install_on_quit` whenever the store version changes
/// (`packages/gui-plugins/src/updater/main.ts:33-68`, `opencode#66501`).
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn sync_install_on_quit_pref() {
    let ver = crate::plugin_host::services::store::store_version("updater", "installOnQuit");
    let changed = INSTALL_ON_QUIT_STORE_VER.with(|v| {
        if v.get() == ver {
            false
        } else {
            v.set(ver);
            true
        }
    });
    if !changed {
        return;
    }
    let enabled = crate::plugin_host::stores::text("updater", "plugin.updater", "installOnQuit")
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .and_then(|v| v.get("enabled")?.as_bool())
        .unwrap_or(false);
    opencode_feature_updater::set_install_on_quit(enabled);
}

/// Starts the update service after the first frame when this build updates itself.
pub fn start_if_packaged(s: &mut App) {
    #[cfg(not(target_arch = "wasm32"))]
    opencode_feature_updater::start_if_packaged(s);
    #[cfg(target_arch = "wasm32")]
    let _ = s;
}

/// Reactive revision of the updater service (`0` on `wasm32`, where no updater runs).
pub(crate) fn revision(s: &App) -> u64 {
    #[cfg(not(target_arch = "wasm32"))]
    {
        s.kernel.slots.get(&UPDATER).revision()
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = s;
        0
    }
}

/// Untracked revision of the updater service for stamp hashing (`0` on `wasm32`).
pub(crate) fn revision_untracked(s: &App) -> u64 {
    #[cfg(not(target_arch = "wasm32"))]
    {
        s.kernel.slots.untracked(&UPDATER).revision()
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = s;
        0
    }
}

/// Running version shown in Settings › About (bundle version once reported, else build version).
pub(crate) fn display_version(s: &App) -> String {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let updater = s.kernel.slots.get(&UPDATER);
        if updater.version.is_empty() {
            build_version().to_owned()
        } else {
            updater.version.clone()
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = s;
        build_version().to_owned()
    }
}

#[cfg(windows)]
pub mod apply {
    pub fn run_if_asked() {
        // The helper is the new version's own executable (a hard link to it): what it reads of
        // itself while it waits for the old version to exit is in memory when it starts the new one.
        if std::env::args().nth(1).as_deref() == Some("--apply-update") {
            crate::launch::image_pages::warm();
        }
        opencode_feature_updater::apply::run_if_asked(crate::launch::product);
    }
}

pub fn build_channel() -> &'static str {
    match option_env!("OPENCODE_RUNE_BUILD_CHANNEL") {
        Some(ch) if !ch.is_empty() => ch,
        _ => "local",
    }
}

/// This build's version: `OPENCODE_RUNE_VERSION` when the build set it, else the crate's.
pub fn build_version() -> &'static str {
    match option_env!("OPENCODE_RUNE_VERSION") {
        Some(v) if !v.is_empty() => v,
        _ => env!("CARGO_PKG_VERSION"),
    }
}

/// The commit this build was made from (`build.rs`), when known. Kept in `.rdata` as
/// `{"sha":"<commit>"}` for binary provenance checks.
pub fn build_commit() -> Option<&'static str> {
    std::hint::black_box(option_env!("OPENCODE_RUNE_BUILD_STAMP"));
    option_env!("OPENCODE_RUNE_COMMIT").filter(|c| !c.is_empty())
}

/// Configures the extracted updater crate with desktop's provenance and updates path.
pub fn init() {
    #[cfg(not(target_arch = "wasm32"))]
    opencode_feature_updater::configure(UpdaterConfig {
        build: BuildInfo {
            channel: build_channel(),
            version: build_version(),
            commit: build_commit(),
            product_name: crate::launch::window_title(),
        },
        updates_dir: crate::dirs::updates,
        plain_error: crate::status_toast::plain,
        rooted: crate::dirs::rooted,
        harness_writes: crate::harness_writes,
    });
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) static UPDATER: opencode_kernel::Slot<Updater> = opencode_kernel::Slot::new();

#[cfg(not(target_arch = "wasm32"))]
impl UpdaterHost for App {
    fn updater(&self) -> &Updater {
        self.kernel.slots.get(&UPDATER)
    }

    fn updater_mut(&mut self) -> &mut Updater {
        self.kernel.slots.get_mut(&UPDATER)
    }
}

/// Kernel feature registration for the self-updater.
#[cfg(not(target_arch = "wasm32"))]
pub struct UpdaterFeature;

#[cfg(not(target_arch = "wasm32"))]
impl Feature<App> for UpdaterFeature {
    fn id(&self) -> &'static str {
        "feature.updater"
    }

    fn register(&self, builder: &mut KernelBuilder<App>) {
        builder.register_slot(&UPDATER);
        builder.schedule(crate::tick::stage::INGEST_LOCAL, "updater.poll", |s, _, tx| {
            sync_install_on_quit_pref();
            if PENDING_DIALOG_INSTALL.swap(false, std::sync::atomic::Ordering::AcqRel) && !opencode_feature_updater::is_suppressed() {
                let lang = opencode_i18n::locale().id();
                let err_title = crate::plugin_host::sources::builtin_message("updater", lang, "dialog.checkFailed.title")
                    .unwrap_or("Update Error")
                    .to_owned();
                opencode_feature_updater::install_with(s, move |err| {
                    if let Some(err) = err {
                        eprintln!("updater: menu dialog install failed: {err}");
                        if CHECK_DIALOG_ACTIVE.compare_exchange(false, true, std::sync::atomic::Ordering::AcqRel, std::sync::atomic::Ordering::Acquire).is_ok() {
                            let guard = DialogGuard;
                            std::thread::Builder::new().name("opencode-update-dialog".into()).spawn(move || {
                                let _guard = guard;
                                opencode_feature_updater::dialog::alert(opencode_feature_updater::dialog::Kind::Error, &err_title, &err);
                            }).ok();
                        }
                    }
                });
            }
            poll(s, tx)
        });
        builder.schedule(crate::tick::stage::PERSIST_FLUSH, "updater.poll_owed", |s, _, _| {
            poll_owed(s);
            false
        });
        builder.add_close("updater.closing", |s| {
            sync_install_on_quit_pref();
            s.kernel.slots.get_mut(&UPDATER).closing();
        });
    }
}
