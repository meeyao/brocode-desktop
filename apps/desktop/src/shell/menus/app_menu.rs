//! Desktop's app menu off macOS (`windows-menu.tsx`): ≡ at the titlebar row's start opens
//! "OpenCode" and its File, Edit, View, Go, Window and Help submenus (`desktop-menu.ts`, the
//! entries Windows shows), drawn in the window in menu-v2's standard look. An item shows its
//! command's keys or the menu's own accelerator, and runs what Desktop's does.

use crate::popup::MenuButton as _;
use opencode_ui::focus::FocusExt;
use rune::{El, Key, KeyEvent, Mods, Tx, icon};

use rune_theme_opencode::icons as ic;

use crate::App;
use crate::keymap::Bind;
use crate::palette::Pal;
use crate::popup::{self, Item, Menu};

/// The button's key, which the menu opens under.
const KEY: &str = "app-menu";
/// The menu's width (`.desktop-app-menu`).
const W: f32 = 160.0;

/// What an item does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Act {
    NewSession,
    Settings,
    NewWindow,
    CloseTab,
    CloseWindow,
    /// macOS's File › Close All, Close Window's Option alternate.
    CloseAll,
    /// An Edit item: the key it sends to the focused field.
    Edit(EditKey),
    ToggleSidebar,
    ToggleTerminal,
    Reload,
    DevTools,
    ActualSize,
    ZoomIn,
    ZoomOut,
    FullScreen,
    Back,
    Forward,
    PreviousSession,
    NextSession,
    Minimize,
    Maximize,
    ExportLogs,
    Open(&'static str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EditKey {
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    Delete,
    SelectAll,
}

/// An item's keys: its command's, as the keymap binds it here, or the menu's accelerator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Keys {
    None,
    Command(Bind),
    /// A built-in plugin's command.
    Plugin(crate::keymap::PluginBind),
    Fixed(&'static str),
}

/// One item, or a separator (`None`).
type Row = Option<(opencode_i18n::Key, Act, Keys)>;

const fn it(title: opencode_i18n::Key, act: Act, keys: Keys) -> Row {
    Some((title, act, keys))
}

/// `DESKTOP_MENU`, as Windows shows it: the app menu is macOS's alone. A submenu's English
/// name is what plugins' menubar items name it by.
const MENUS: [(&str, opencode_i18n::Key, &[Row]); 6] = [
    ("File", opencode_i18n::key!("desktop.menu.file"), &[
        it(opencode_i18n::key!("desktop.menu.newSession"), Act::NewSession, Keys::Command(Bind::TabNew)),
        it(opencode_i18n::key!("desktop.menu.settings"), Act::Settings, Keys::Command(Bind::Settings)),
        it(opencode_i18n::key!("desktop.menu.newWindow"), Act::NewWindow, Keys::Fixed("Ctrl+Shift+N")),
        None,
        it(opencode_i18n::key!("desktop.menu.closeWindow"), Act::CloseWindow, Keys::Fixed("Ctrl+Shift+W")),
    ]),
    ("Edit", opencode_i18n::key!("desktop.menu.edit"), &[
        it(opencode_i18n::key!("desktop.menu.undo"), Act::Edit(EditKey::Undo), Keys::Fixed("Ctrl+Z")),
        it(opencode_i18n::key!("desktop.menu.redo"), Act::Edit(EditKey::Redo), Keys::Fixed("Ctrl+Y")),
        None,
        it(opencode_i18n::key!("desktop.menu.cut"), Act::Edit(EditKey::Cut), Keys::Fixed("Ctrl+X")),
        it(opencode_i18n::key!("desktop.menu.copy"), Act::Edit(EditKey::Copy), Keys::Fixed("Ctrl+C")),
        it(opencode_i18n::key!("desktop.menu.paste"), Act::Edit(EditKey::Paste), Keys::Fixed("Ctrl+V")),
        it(opencode_i18n::key!("desktop.menu.delete"), Act::Edit(EditKey::Delete), Keys::None),
        it(opencode_i18n::key!("desktop.menu.selectAll"), Act::Edit(EditKey::SelectAll), Keys::Fixed("Ctrl+A")),
    ]),
    ("View", opencode_i18n::key!("desktop.menu.view"), &[
        it(opencode_i18n::key!("desktop.menu.toggleSidebar"), Act::ToggleSidebar, Keys::Command(Bind::Sidebar)),
        it(opencode_i18n::key!("desktop.menu.toggleTerminal"), Act::ToggleTerminal, Keys::Plugin(crate::keymap::TERMINAL_TOGGLE)),
        None,
        it(opencode_i18n::key!("desktop.menu.reload"), Act::Reload, Keys::None),
        it(opencode_i18n::key!("desktop.menu.toggleDeveloperTools"), Act::DevTools, Keys::None),
        None,
        it(opencode_i18n::key!("desktop.menu.actualSize"), Act::ActualSize, Keys::Fixed("Ctrl+0")),
        it(opencode_i18n::key!("desktop.menu.zoomIn"), Act::ZoomIn, Keys::Fixed("Ctrl++")),
        it(opencode_i18n::key!("desktop.menu.zoomOut"), Act::ZoomOut, Keys::Fixed("Ctrl+-")),
        None,
        it(opencode_i18n::key!("desktop.menu.toggleFullScreen"), Act::FullScreen, Keys::None),
    ]),
    ("Go", opencode_i18n::key!("desktop.menu.go"), &[
        it(opencode_i18n::key!("desktop.menu.back"), Act::Back, Keys::Command(Bind::GoBack)),
        it(opencode_i18n::key!("desktop.menu.forward"), Act::Forward, Keys::Command(Bind::GoForward)),
        None,
        it(opencode_i18n::key!("desktop.menu.previousSession"), Act::PreviousSession, Keys::Command(Bind::SessionPrev)),
        it(opencode_i18n::key!("desktop.menu.nextSession"), Act::NextSession, Keys::Command(Bind::SessionNext)),
    ]),
    ("Window", opencode_i18n::key!("desktop.menu.window"), &[
        it(opencode_i18n::key!("desktop.menu.minimize"), Act::Minimize, Keys::None),
        it(opencode_i18n::key!("desktop.menu.maximize"), Act::Maximize, Keys::None),
        None,
        it(opencode_i18n::key!("desktop.menu.closeWindow"), Act::CloseWindow, Keys::None),
    ]),
    ("Help", opencode_i18n::key!("desktop.menu.help"), &[
        it(opencode_i18n::key!("desktop.menu.documentation"), Act::Open("https://github.com/meeyao/brocode-desktop#readme"), Keys::None),
        it(opencode_i18n::key!("desktop.menu.exportLogs"), Act::ExportLogs, Keys::None),
        None,
        it(opencode_i18n::key!("desktop.menu.shareFeedback"), Act::Open("https://github.com/meeyao/brocode-desktop/issues/new"), Keys::None),
        it(opencode_i18n::key!("desktop.menu.reportBug"), Act::Open("https://github.com/meeyao/brocode-desktop/issues/new"), Keys::None),
    ]),
];

/// The button at the titlebar row's start (`IconButton` ghost-muted, large) in its 36pt slot
/// (`h-7 w-9 px-1`): lit as pressed while its menu is open.
pub fn button(s: &App, p: &Pal) -> El {
    rune::row().key("app-menu-slot").size(36.0, 28.0).center().child(glyph_button(s, p))
}

fn glyph_button(s: &App, p: &Pal) -> El {
    let open = s.popup.open_at(KEY);
    rune::el()
        .key(KEY)
        .role("button")
        .label_static(opencode_i18n::t!("desktop.menu.ariaLabel"))
        .expanded(open)
        .size(28.0, 28.0)
        .radius(6.0)
        .focus_inset(p)
        .center()
        .fill(if open { p.pressed } else { rune::Color::TRANSPARENT })
        .hover(if open { rune::Color::TRANSPARENT } else { p.hover })
        .press(p.pressed)
        .hover_motion(crate::view::INSTANT, crate::view::INSTANT)
        // Desktop remembers the focused field and puts it back for the Edit items.
        .keeps_text_selection()
        .menu_button(KEY, |s: &mut App, tx: &mut Tx| {
            tx.instant();
            open_menu(s, tx);
        })
        .child(icon(&ic::MENU, 16.0).color(if open { p.icon } else { p.icon_muted }))
}

/// Opens the menu under its button.
fn open_menu(s: &mut App, tx: &mut Tx) {
    popup::open_drawn(s, menu(s), KEY, tx, |s: &mut App, picked, tx| {
        if let Some(key) = picked.and_then(|p| p.strip_prefix(PLUGIN)) {
            crate::plugin_host::run_menubar(s, key, tx);
            return;
        }
        if let Some(act) = picked.and_then(find) {
            run(s, act, tx);
        }
    });
}

/// The prefix of a plugin's item id (`MenubarItem`): `app-menu-plugin-<plugin>.<id>`.
const PLUGIN: &str = "app-menu-plugin-";

/// The menu as it stands: a command's item is disabled while the command can't run. Plugins'
/// items follow each submenu's own, after a separator.
fn menu(s: &App) -> Menu {
    let plugin_items = crate::plugin_host::menubar_items();
    let menus = MENUS.iter().enumerate().map(|(m, (name, title, rows))| {
        let title = opencode_i18n::I18N.text(*title);
        let sub = rows.iter().enumerate().fold(Menu::new(title), |menu, (r, row)| match row {
            None => menu.separator(),
            Some((title, act, keys)) => menu.item(Item::new(id(m, r), opencode_i18n::I18N.text(*title)).shortcut(label(*keys)).enabled(enabled(s, *act))),
        });
        let theirs: Vec<_> = plugin_items.iter().filter(|(menu, _, _, _)| menu.eq_ignore_ascii_case(name)).collect();
        let sub = if theirs.is_empty() { sub } else { sub.separator() };
        let sub = theirs.into_iter().fold(sub, |menu, (_, key, title, is_enabled)| menu.item(Item::new(format!("{PLUGIN}{key}"), title.clone()).enabled(*is_enabled)));
        (m, title, sub)
    });
    menus.fold(Menu::new(opencode_i18n::t!("desktop.menu.app")).standard(W).remembers_focus().group(opencode_i18n::t!("desktop.menu.app")), |menu, (m, title, sub)| {
        menu.submenu(format!("app-menu-{m}"), title, sub)
    })
}

fn id(m: usize, r: usize) -> String {
    format!("app-menu-{m}-{r}")
}

/// The item an id names.
fn find(id: &str) -> Option<Act> {
    let mut parts = id.strip_prefix("app-menu-")?.split('-').map(str::parse::<usize>);
    let (m, r) = (parts.next()?.ok()?, parts.next()?.ok()?);
    MENUS.get(m)?.2.get(r)?.map(|(_, act, _)| act)
}

fn label(keys: Keys) -> String {
    match keys {
        Keys::None => String::new(),
        Keys::Command(bind) => bind.label(),
        Keys::Plugin(bind) => bind.label(),
        Keys::Fixed(keys) => keys.to_owned(),
    }
}

/// Toggle Terminal needs a session, as Desktop's `terminal.toggle` command does, and Toggle
/// Sidebar a sidebar, which top tabs don't have.
fn enabled(s: &App, act: Act) -> bool {
    match act {
        Act::ToggleTerminal => s.active_session_id().is_some(),
        Act::ToggleSidebar => s.shows_sidebar(),
        _ => true,
    }
}

pub(crate) fn run(s: &mut App, act: Act, tx: &mut Tx) {
    // An item that acts in the window brings back the last window Close Window hid.
    let revealed = !matches!(act, Act::CloseWindow | Act::CloseAll | Act::Open(_) | Act::ExportLogs) && s.reveal();
    match act {
        Act::NewSession => s.new_session(tx),
        Act::Settings => {
            tx.instant();
            s.open_settings();
        }
        Act::NewWindow => {
            // A hidden last window came back instead.
            if !revealed {
                crate::windows::new_window(s);
            }
        }
        Act::CloseTab => s.close_tab_key(tx),
        Act::CloseWindow => s.close_window(tx),
        Act::CloseAll => s.close_all_windows(tx),
        Act::Edit(key) => {
            // Back to the field focused as the menu opened, then the key (`windows-menu.tsx:69`).
            if let Some(field) = s.popup.remembered().map(str::to_owned) {
                tx.focus(field, false);
            }
            tx.send_key(edit_key(key));
        }
        Act::ToggleSidebar => {
            s.toggle_sidebar(tx);
        }
        Act::ToggleTerminal => s.toggle_terminal(tx),
        Act::Reload => {
            s.resync_shared();
            s.resync_view();
        }
        Act::DevTools => {
            tx.instant();
            s.diag.knobs.toggle();
        }
        Act::ActualSize => s.zoom_by(0, tx),
        Act::ZoomIn => s.zoom_by(1, tx),
        Act::ZoomOut => s.zoom_by(-1, tx),
        Act::FullScreen => tx.toggle_full_screen(),
        Act::Back => {
            s.navigate(false, tx);
        }
        Act::Forward => {
            s.navigate(true, tx);
        }
        Act::PreviousSession => s.select_adjacent(-1, tx),
        Act::NextSession => s.select_adjacent(1, tx),
        Act::Minimize => tx.minimize(),
        Act::Maximize => tx.toggle_maximized(),
        Act::ExportLogs => s.export_logs(),
        Act::Open(url) => {
            crate::open_in::open_external(tx, url);
        }
    }
}

/// The key an Edit item sends, as Electron's `webContents` edit commands act on the focused
/// field: Rune's text fields take the primary modifier's letters, and Delete.
fn edit_key(key: EditKey) -> KeyEvent {
    let primary = Mods { cmd: true, ctrl: !cfg!(target_os = "macos"), ..Mods::default() };
    let (key, mods) = match key {
        EditKey::Undo => (Key::Command('z'), primary),
        EditKey::Redo => (Key::Command('z'), Mods { shift: true, ..primary }),
        EditKey::Cut => (Key::Command('x'), primary),
        EditKey::Copy => (Key::Command('c'), primary),
        EditKey::Paste => (Key::Command('v'), primary),
        EditKey::Delete => (Key::Delete, Mods::default()),
        EditKey::SelectAll => (Key::Command('a'), primary),
    };
    KeyEvent { key, mods }
}
