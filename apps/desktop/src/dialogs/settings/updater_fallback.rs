//! Native fallback for Settings › Preferences › Updates (`packages/gui-plugins/src/updater/section.tsx`
//! + `action.ts`).
//!
//! Shown ONLY while the bundled `updater` Luau plugin has not produced its own live section on
//! `page_id == "general"` (normally a single frame while `updater` wakes from `Dormant`, `boot.later`,
//! or `Life::Loading`, or until recovery/reload if `updater` is `Life::Errored`), and NEVER when the
//! user disabled `updater` or launched with `--no-plugins`.
//!
//! Neither control is inert:
//! - The **Release notes** switch reads and writes the exact same `plugin.updater` / `releaseNotes`
//!   store row (`{"enabled": bool}`) that `plugins/updater/init.luau` reads and watches.
//! - The **Check now** / **Install and restart** button mirrors `action.ts` (`Check now` disabled
//!   while `state_view` is `None` / `View::Starting`; `Updates unavailable` disabled when `disabled`;
//!   enabled actions wake `updater` when possible or invoke `opencode_feature_updater::{check_with,
//!   install_with}` directly with the same loading/outcome toasts).

use rune::{El, Tx};

use crate::App;
use crate::palette::Pal;

/// Appends the native Updates fallback section when `updater` has not produced its live section,
/// logging one deduplicated diagnostic line naming every gating state.
pub(super) fn maybe_push(app: &App, p: &Pal, sections: &mut Vec<El>) {
    if crate::plugin_host::has_live_settings_section(app, "updater", "general") {
        return;
    }
    let no_plugins = crate::flag("--no-plugins");
    let disabled = app.plugins.is_disabled("updater");
    log_omission(app, no_plugins, disabled);
    if !no_plugins && !disabled {
        sections.push(view(app, p));
    }
}

fn updater_phase(app: &App) -> Option<&'static str> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        opencode_feature_updater::state_view(app).map(|v| v.phase)
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = app;
        None
    }
}

fn log_omission(app: &App, no_plugins: bool, disabled: bool) {
    thread_local! {
        static LAST: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
    }
    let found = app.plugins.find("updater");
    let life = found.map(|i| app.plugins.list[i].life.label()).unwrap_or("absent");
    let why = found.and_then(|i| app.plugins.list[i].life.why()).unwrap_or("");
    let in_later = app.plugins.is_deferred("updater");
    let storage_ready = crate::storage::is_ready();
    let stores_ready = crate::plugin_host::stores::answers_for("updater");
    let phase = updater_phase(app).unwrap_or("none");
    let key = format!(
        "no_plugins={no_plugins} disabled={disabled} life={life} why={why:?} in_later={in_later} storage_ready={storage_ready} stores_ready={stores_ready} updater_phase={phase}"
    );
    LAST.with_borrow_mut(|last| {
        if *last != key {
            *last = key.clone();
            eprintln!("settings: updater section omitted by plugin_host ({key})");
        }
    });
}

fn msg(key: &str, fallback: &str) -> String {
    let lang = opencode_i18n::locale().id();
    crate::plugin_host::sources::builtin_message("updater", lang, key)
        .unwrap_or(fallback)
        .to_owned()
}

fn release_notes_enabled() -> bool {
    crate::plugin_host::stores::text("updater", "plugin.updater", "releaseNotes")
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .and_then(|v| v.get("enabled")?.as_bool())
        .unwrap_or(true)
}

#[cfg(not(target_arch = "wasm32"))]
fn install_on_quit_enabled() -> bool {
    crate::plugin_host::stores::text("updater", "plugin.updater", "installOnQuit")
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .and_then(|v| v.get("enabled")?.as_bool())
        .unwrap_or(false)
}

/// Why updates are off: the isolated-run text when the updater said so (`ISOLATED`), else the
/// package-manager text `action.ts` shows.
fn disabled_description(error: Option<&str>) -> String {
    #[cfg(not(target_arch = "wasm32"))]
    if error == Some(opencode_feature_updater::ISOLATED) {
        return msg("check.isolated", "Updates are off in this isolated run (RUNE_DIRS_ROOT).");
    }
    let _ = error;
    msg("check.unavailable", "This build is updated outside Brocode, for example by your package manager.")
}

fn updater_error(app: &App) -> Option<String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        opencode_feature_updater::state_view(app).and_then(|v| v.error)
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = app;
        None
    }
}

fn view(app: &App, p: &Pal) -> El {
    let phase = updater_phase(app);
    let (label_key, label_fallback, desc_key, desc_fallback, run) = match phase {
        None => ("action.checkNow", "Check now", "check.description", "Manually check for updates and install if available", None),
        Some("checking") => ("action.checking", "Checking…", "check.description", "Manually check for updates and install if available", None),
        Some("downloading") => ("action.downloading", "Downloading…", "check.description", "Manually check for updates and install if available", None),
        Some("ready") => ("action.installRestart", "Install and restart", "check.description", "Manually check for updates and install if available", Some("install")),
        Some("download-required") => ("action.download", "Download Brocode", "check.description", "Manually check for updates and install if available", Some("install")),
        Some("installing") => ("action.installing", "Installing…", "check.description", "Manually check for updates and install if available", None),
        Some("disabled") => (
            "action.unavailable",
            "Updates unavailable",
            "check.unavailable",
            "This build is updated outside Brocode, for example by your package manager.",
            None,
        ),
        Some(_) => ("action.checkNow", "Check now", "check.description", "Manually check for updates and install if available", Some("check")),
    };
    let notes_on = release_notes_enabled();
    let notes_title = msg("releaseNotes.title", "Release notes");
    let notes_desc = msg("releaseNotes.description", "Show What's New popups after updates");
    let check_title = msg("check.title", "Check for updates");
    let check_desc = if phase == Some("disabled") { disabled_description(updater_error(app).as_deref()) } else { msg(desc_key, desc_fallback) };
    let btn_label = msg(label_key, label_fallback);
    let section_title = msg("section.title", "Updates");

    let notes_control = crate::settings::switch(p, "release-notes-switch", notes_on).on_press(move |s: &mut App| {
        let next = !notes_on;
        crate::plugin_host::stores::set("plugin.updater", "releaseNotes", format!("{{\"enabled\":{next}}}"));
        crate::plugin_host::services::store::store_changed("updater", "releaseNotes");
        rune::Tracked::touch(&s.plugins);
    });
    let mut btn = opencode_ui::widgets::button(btn_label, rune_kit::ButtonVariant::Neutral).disabled(run.is_none());
    if let Some(action) = run {
        btn = btn.on_press_tx(move |s: &mut App, tx: &mut Tx| run_action(s, tx, action));
    }
    let check_control = btn.build(opencode_ui::palette::kit(p)).key("check-updates-button");

    let notes_row = crate::settings::setting(p, "settings-release-notes", &notes_title, &notes_desc, notes_control);
    let check_row = crate::settings::setting(p, "settings-check-updates", &check_title, &check_desc, check_control);
    let lit_notes = crate::settings::search::reveal_of(app, "settings-release-notes");
    let lit_check = crate::settings::search::reveal_of(app, "settings-check-updates");
    let mut rows = vec![crate::settings::pages::revealed(p, lit_notes, 20.0, notes_row)];
    #[cfg(not(target_arch = "wasm32"))]
    {
        let quit_disabled = phase.is_none() || phase == Some("disabled");
        let quit_on = !quit_disabled && install_on_quit_enabled();
        let quit_title = msg("installOnQuit.title", "Install updates on quit");
        let quit_desc = msg(
            "installOnQuit.description",
            "Automatically install downloaded updates when you quit Brocode",
        );
        let mut quit_sw = rune_kit::Switch::new(quit_on)
            .web()
            .disabled(quit_disabled)
            .build(opencode_ui::palette::kit(p))
            .key("install-on-quit-switch")
            .padding(4.0)
            .labelled_by("title")
            .described_by("about");
        if !quit_disabled {
            quit_sw = quit_sw.on_press(move |s: &mut App| {
                let next = !quit_on;
                crate::plugin_host::stores::set("plugin.updater", "installOnQuit", format!("{{\"enabled\":{next}}}"));
                crate::plugin_host::services::store::store_changed("updater", "installOnQuit");
                opencode_feature_updater::set_install_on_quit(next);
                rune::Tracked::touch(&s.plugins);
            });
        }
        let quit_row = crate::settings::setting(p, "settings-install-on-quit", &quit_title, &quit_desc, quit_sw);
        let lit_quit = crate::settings::search::reveal_of(app, "settings-install-on-quit");
        rows.push(crate::settings::pages::revealed(p, lit_quit, 20.0, quit_row));
    }
    rows.push(crate::settings::pages::revealed(p, lit_check, 20.0, check_row));
    let card = crate::settings::list(p, "settings-updates", rows);
    rune::col()
        .key("plugin-page-updater.updates")
        .w_full()
        .child(crate::settings::pages::section(p, &format!("plugin-section-{section_title}"), Some(&section_title), card))
}

fn run_action(s: &mut App, tx: &mut Tx, action: &'static str) {
    if action == "check" && crate::plugin_host::ensure_plugin_running(s, "updater") {
        crate::plugin_host::run_command(s, "updater.check", tx);
        return;
    }
    #[cfg(not(target_arch = "wasm32"))]
    match action {
        "check" => {
            let toast_id = crate::status_toast::waiting(s, &msg("action.checking", "Checking…"));
            let deliver = s.deliverer();
            opencode_feature_updater::check_with(s, move |view| {
                deliver(Box::new(move |app: &mut App, tx: &mut Tx| {
                    crate::status_toast::dismiss_id(app, toast_id);
                    tx.instant();
                    let Some(view) = view else { return };
                    match view.phase {
                        "disabled" => crate::status_toast::say(app, msg("action.unavailable", "Updates unavailable"), disabled_description(view.error.as_deref())),
                        "up-to-date" => {
                            let ver = crate::updater::display_version(app);
                            let desc = msg("toast.latest.description", "You're running the latest version of Brocode.")
                                .replace("{{version}}", &ver);
                            crate::status_toast::succeed(app, msg("toast.latest.title", "You're up to date"), desc);
                        }
                        "ready" => {
                            let ver = view.version.as_deref().unwrap_or_default();
                            let desc = msg("dialog.ready.message", "Update {{version}} downloaded. Restart now?")
                                .replace("{{version}}", ver);
                            crate::status_toast::say(app, msg("dialog.ready.title", "Update Ready"), desc);
                        }
                        "download-required" => {
                            let ver = view.version.as_deref().unwrap_or_default();
                            let desc = msg("dialog.available.message", "Brocode {{version}} is available to download.")
                                .replace("{{version}}", ver);
                            crate::status_toast::say(app, msg("dialog.available.title", "Update Available"), desc);
                        }
                        "installed" => {
                            let ver = view.version.as_deref().unwrap_or_default();
                            let desc = msg(
                                "toast.installed.description",
                                "Version {{version}} is installed. It will run the next time you open Brocode.",
                            )
                            .replace("{{version}}", ver);
                            crate::status_toast::say(app, msg("section.title", "Updates"), desc);
                        }
                        "error" => crate::status_toast::fail(
                            app,
                            msg("common.requestFailed", "Request failed"),
                            view.error.unwrap_or_else(|| msg("dialog.checkFailed.message", "Update check failed.")),
                        ),
                        _ => {}
                    }
                }));
            });
        }
        "install" => {
            let deliver = s.deliverer();
            opencode_feature_updater::install_with(s, move |err| {
                let Some(err) = err else { return };
                deliver(Box::new(move |app: &mut App, tx: &mut Tx| {
                    tx.instant();
                    crate::status_toast::fail(app, msg("common.requestFailed", "Request failed"), err);
                }));
            });
        }
        _ => {}
    }
}
