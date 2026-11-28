//! The native menu bar (macOS), as Desktop builds it from `DESKTOP_MENU` (`desktop-menu.ts`,
//! `native/menu.ts`): the app menu's plugin items after About (the Updater's Check for Updates), then Settings, Reload
//! Webview and Export Logs; File; Edit's Delete among the standard items; View; Go; and Help.
//! Rune's standard Edit and Window items and the app menu's About, Hide and Quit stay AppKit's.
//! An item a command binds shows the keys the command has now, so a change in Settings >
//! Shortcuts reaches the menu bar at once; the app's first frame puts the saved changes there
//! too. A language switch rebuilds it the same way, retitling these items and, through Rune's
//! labels (`language::label`), its menus and standard Edit items.

use std::sync::Mutex;

use opencode_i18n::t;
use rune::{AppHandle, MenuItem, Tx};

use crate::App;
use crate::app_menu::{Act, EditKey, run};
use crate::keymap::Bind;

static HANDLES: Mutex<Vec<AppHandle<App>>> = Mutex::new(Vec::new());

/// `item` with `bind`'s first chord as its key equivalent, none while the command is unbound.
pub fn keyed(item: MenuItem<App>, bind: Bind) -> MenuItem<App> {
    match bind.shortcut() {
        Some(keys) => item.key_equivalent(keys),
        None => item,
    }
}

/// `keyed` for a built-in plugin's command.
fn keyed_plugin(item: MenuItem<App>, bind: crate::keymap::PluginBind) -> MenuItem<App> {
    match bind.shortcut() {
        Some(keys) => item.key_equivalent(keys),
        None => item,
    }
}

/// `keyed`, but showing Desktop's accelerator `chord` while the command still answers to it
/// (New Session's ⌘N, where the command's first chord is ⌘T).
fn keyed_as(item: MenuItem<App>, bind: Bind, chord: &str) -> MenuItem<App> {
    let wanted = chord.parse::<rune::Chord>().ok().map(|c| c.to_string());
    if bind.keybind().chords().iter().any(|c| Some(c.to_string()) == wanted)
        && let Ok(keys) = chord.parse()
    {
        return item.key_equivalent(keys);
    }
    keyed(item, bind)
}

/// `item` with Desktop's fixed accelerator `keys`, for an action no command binds.
fn fixed(item: MenuItem<App>, keys: &str) -> MenuItem<App> {
    match keys.parse() {
        Ok(keys) => item.key_equivalent(keys),
        Err(_) => item,
    }
}

fn items() -> Vec<MenuItem<App>> {
    [app(), file(), edit(), crate::find::menu_items(), view(), crate::history::menu_items(), go(), help()].into_iter().flatten().collect()
}

/// The app menu after About: the items Luau plugins add there ([`plugin_items`]: the Updater's
/// Check for Updates, Desktop's `MenubarItem { menu: "app", after: "about" }`); then Settings,
/// Reload Webview (Rune's Reload, which reads the server again), Restart (Desktop's
/// `app.relaunch`: the app quits, saving every window, and starts again) and Export Logs.
/// Install CLI isn't here: Desktop's installs the CLI it bundles (`desktop-cli.ts`), and Rune bundles none; it attaches to the CLI's service.
fn app() -> Vec<MenuItem<App>> {
    plugin_items().into_iter().chain([
        keyed(MenuItem::new(t!("desktop.menu.settings"), |s: &mut App, tx: &mut Tx| run(s, Act::Settings, tx)).id("App/Settings"), Bind::Settings).keys_first(),
        MenuItem::new(t!("desktop.menu.reloadWebview"), |s: &mut App, tx: &mut Tx| run(s, Act::Reload, tx)).id("App/ReloadWebview"),
        MenuItem::new(t!("desktop.menu.restart"), |_: &mut App, tx: &mut Tx| {
            crate::background_notice::mark_relaunch();
            tx.relaunch();
        })
        .id("App/Restart"),
        MenuItem::new(t!("desktop.menu.exportLogs"), |s: &mut App, tx: &mut Tx| run(s, Act::ExportLogs, tx)).id("App/ExportLogs"),
    ])
    .collect()
}

/// The app menu's items Luau plugins add (`MenubarItem { menu: "app" }`). Plugins load after
/// the menu is first built, so the first frame's [`refresh`] puts them there. A native item runs
/// a plain `fn`, so each slot runs the plugin item at its place in the list.
fn plugin_items() -> Vec<MenuItem<App>> {
    const SLOTS: [fn(&mut App, &mut Tx); 8] = [plugin_slot::<0>, plugin_slot::<1>, plugin_slot::<2>, plugin_slot::<3>, plugin_slot::<4>, plugin_slot::<5>, plugin_slot::<6>, plugin_slot::<7>];
    app_items().into_iter().zip(SLOTS).map(|((key, label, enabled), slot)| MenuItem::new(label, slot).id(format!("App/{key}")).enabled(enabled)).collect()
}

fn app_items() -> Vec<(&'static str, String, bool)> {
    crate::plugin_host::commands::menubar_items().into_iter().filter(|(_, menu, _, _, _)| menu == "app").map(|(key, _, label, _, enabled)| (key, label, enabled)).collect()
}

fn plugin_slot<const I: usize>(s: &mut App, tx: &mut Tx) {
    if let Some((key, _, true)) = app_items().get(I) {
        crate::plugin_host::run_menubar(s, key, tx);
    }
}

/// New Window and Close Window take ⇧⌘N and ⇧⌘W before the window sees them, as Electron's
/// menu accelerators do (no command binds them on macOS; Close Tab's ⌘W would take ⇧⌘W).
/// Close Tab is Safari's: ⌘W goes to the window first (a focused panel's tab closes), and comes
/// ahead of the Window menu's Close, so it never closes the last window.
fn file() -> Vec<MenuItem<App>> {
    vec![
        keyed_as(MenuItem::new(t!("desktop.menu.newSession"), |s: &mut App, tx: &mut Tx| run(s, Act::NewSession, tx)).id("File/NewSession").menu("File"), Bind::TabNew, "mod+n").keys_first(),
        fixed(MenuItem::new(t!("desktop.menu.newWindow"), |s: &mut App, tx: &mut Tx| run(s, Act::NewWindow, tx)).id("File/NewWindow").menu("File"), "mod+shift+n"),
        MenuItem::separator().menu("File"),
        keyed(MenuItem::new(t!("command.tab.close"), |s: &mut App, tx: &mut Tx| run(s, Act::CloseTab, tx)).id("File/CloseTab").menu("File"), Bind::TabClose).keys_first(),
        fixed(MenuItem::new(t!("desktop.menu.closeWindow"), |s: &mut App, tx: &mut Tx| run(s, Act::CloseWindow, tx)).id("File/CloseWindow").menu("File"), "mod+shift+w"),
        // AppKit's Close All, behind Close Window while Option is held, as official Desktop's File menu has it.
        fixed(MenuItem::new(t!("rune.menu.file.closeAll"), |s: &mut App, tx: &mut Tx| run(s, Act::CloseAll, tx)).id("File/CloseAll").menu("File").alternate(), "mod+alt+shift+w"),
    ]
}

/// Delete, between the standard Paste and Select All; it sends the focused field Delete.
fn edit() -> Vec<MenuItem<App>> {
    vec![MenuItem::new(t!("desktop.menu.delete"), |s: &mut App, tx: &mut Tx| run(s, Act::Edit(EditKey::Delete), tx)).id("Edit/Delete").menu("Edit").after("paste:")]
}

/// Reload, Toggle Developer Tools and Toggle Full Screen take the keys Electron's roles give
/// them; Developer Tools are Rune's knobs.
fn view() -> Vec<MenuItem<App>> {
    vec![
        keyed(MenuItem::new(t!("desktop.menu.toggleSidebar"), |s: &mut App, tx: &mut Tx| run(s, Act::ToggleSidebar, tx)).id("View/ToggleSidebar").menu("View"), Bind::Sidebar).keys_first(),
        keyed_plugin(MenuItem::new(t!("desktop.menu.toggleTerminal"), |s: &mut App, tx: &mut Tx| run(s, Act::ToggleTerminal, tx)).id("View/ToggleTerminal").menu("View"), crate::keymap::TERMINAL_TOGGLE).keys_first(),
        MenuItem::separator().menu("View"),
        fixed(MenuItem::new(t!("desktop.menu.reload"), |s: &mut App, tx: &mut Tx| run(s, Act::Reload, tx)).id("View/Reload").menu("View"), "mod+r").keys_first(),
        fixed(MenuItem::new(t!("desktop.menu.toggleDeveloperTools"), |s: &mut App, tx: &mut Tx| run(s, Act::DevTools, tx)).id("View/ToggleDeveloperTools").menu("View"), "mod+alt+i").keys_first(),
        MenuItem::separator().menu("View"),
        fixed(MenuItem::new(t!("desktop.menu.actualSize"), |s: &mut App, tx: &mut Tx| run(s, Act::ActualSize, tx)).id("View/ActualSize").menu("View"), "mod+0").keys_first(),
        fixed(MenuItem::new(t!("desktop.menu.zoomIn"), |s: &mut App, tx: &mut Tx| run(s, Act::ZoomIn, tx)).id("View/ZoomIn").menu("View"), "mod+=").keys_first(),
        fixed(MenuItem::new(t!("desktop.menu.zoomOut"), |s: &mut App, tx: &mut Tx| run(s, Act::ZoomOut, tx)).id("View/ZoomOut").menu("View"), "mod+-").keys_first(),
        MenuItem::separator().menu("View"),
        fixed(MenuItem::new(t!("desktop.menu.toggleFullScreen"), |s: &mut App, tx: &mut Tx| run(s, Act::FullScreen, tx)).id("View/ToggleFullScreen").menu("View"), "ctrl+mod+f").keys_first(),
    ]
}

/// After Back and Forward (`history::menu_items`).
fn go() -> Vec<MenuItem<App>> {
    vec![
        MenuItem::separator().menu("Go"),
        keyed(MenuItem::new(t!("desktop.menu.previousSession"), |s: &mut App, tx: &mut Tx| run(s, Act::PreviousSession, tx)).id("Go/PreviousSession").menu("Go"), Bind::SessionPrev).keys_first(),
        keyed(MenuItem::new(t!("desktop.menu.nextSession"), |s: &mut App, tx: &mut Tx| run(s, Act::NextSession, tx)).id("Go/NextSession").menu("Go"), Bind::SessionNext).keys_first(),
    ]
}

fn help() -> Vec<MenuItem<App>> {
    vec![
        MenuItem::new(t!("desktop.menu.documentation"), |s: &mut App, tx: &mut Tx| run(s, Act::Open("https://github.com/meeyao/brocode-desktop#readme"), tx)).id("Help/Documentation").menu("Help"),
        MenuItem::new(t!("desktop.menu.exportLogs"), |s: &mut App, tx: &mut Tx| run(s, Act::ExportLogs, tx)).id("Help/ExportLogs").menu("Help"),
        MenuItem::separator().menu("Help"),
        MenuItem::new(t!("desktop.menu.shareFeedback"), |s: &mut App, tx: &mut Tx| {
            run(s, Act::Open("https://github.com/meeyao/brocode-desktop/issues/new"), tx)
        })
        .id("Help/ShareFeedback")
        .menu("Help"),
        MenuItem::new(t!("desktop.menu.reportBug"), |s: &mut App, tx: &mut Tx| run(s, Act::Open("https://github.com/meeyao/brocode-desktop/issues/new"), tx))
            .id("Help/ReportBug")
            .menu("Help"),
    ]
}

/// Every window is an App with these items; the key window's are on the menu bar (Rune), and
/// they act in that window.
pub fn install(app: rune::App<App>) -> rune::App<App> {
    let app = items().into_iter().fold(app, |a, i| a.menu_item(i));
    HANDLES.lock().unwrap_or_else(|e| e.into_inner()).push(app.handle());
    app
}

/// Puts the commands' current keys and the language's words in every window's items, so the
/// next window to take focus brings them too; a closed window's App answers `NotRunning` and
/// keeps nothing. Off macOS, where the menu is drawn in the window from the keymap each time
/// it opens, Rune has no native menu and this does nothing.
pub fn refresh() {
    for handle in HANDLES.lock().unwrap_or_else(|e| e.into_inner()).iter() {
        handle.set_menu(items()).ok();
    }
}
