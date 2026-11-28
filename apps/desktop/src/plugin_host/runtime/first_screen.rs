//! Which built-in plugins draw on this launch's first screen, decided from the boot state before
//! any of them starts: those start before the first frame, so nothing they show arrives a frame
//! late, and the rest start after it (`super::boot::first_frame`). Each plugin's contributions come from
//! its source at build time (`sources::contributions`).
//!
//! - **Every route:** a server source whose servers were listed at the last run (the sidebar and
//!   Home show their rows; the plugin's state for them would change after the frame otherwise),
//!   and the debug plugin while its bar (a `window.bottom` slot) is shown.
//! - **Not before the frame on any route:** the updater's titlebar pill shows only once the
//!   updater's worker, started after the first frame, has an update ready, so it's hidden in the
//!   first frame and the second either way.
//! - **A session:** the slots in its header (the debug bar's only while it's shown, above); with a
//!   cached transcript on screen, the code block plugins whose languages its saved turns hold a
//!   block of (a link handler draws nothing: opening a link starts it); and every plugin with a
//!   tab in the session's open side or bottom pane (the tab strip shows each one's title), plus
//!   the file viewers when the Files plugin has one.
//! - **A session whose side panel is open on New tab:** the plugins with an entry there that
//!   answers a function (a row's `when`, a panel's `enabled`). An entry that is all in its
//!   plugin's record draws from it, and a plugin with no entry there doesn't start for the view.
//!   A plugin with no record of this build starts.
//! - **Not before the frame:** a composer pill. The one built-in with one (Goal) shows it once the
//!   server has answered for the session's goal, which is after the first frame however early it
//!   starts; the composer asks for it as it draws (`surfaces::pill_views`), so it starts at the
//!   next turn.
//! - **Home and a draft:** nothing else.

use opencode_kit::servers::ServerKey;
use opencode_storage::Lookup;

use crate::App;
use crate::panels::SessionPane;
use crate::route::Route;

use super::sources;

/// The built-in plugins `app`'s first screen shows. `record` is a built-in's record of this build
/// in this language, when the boot record has one.
pub fn plugins(app: &App, record: impl Fn(&str) -> Option<opencode_plugin_runtime::dormant::Record>) -> Vec<&'static str> {
    let has = |id: &str, kinds: &[&str]| sources::contributions(id).iter().any(|k| kinds.contains(k));
    let mut out: Vec<&'static str> = sources::builtin_ids().filter(|id| has(id, &["ServerSource"]) && crate::plugin_host::server_sources::remembers(id)).collect();
    if debug_bar_shown() {
        out.push("debug");
    }
    if let Some(open) = &app.plugins.dialog
        && let Some(id) = sources::builtin_ids().find(|id| *id == &*open.plugin)
    {
        out.push(id);
    }
    let Route::Session { id: session, .. } = &*app.route else { return out };
    // The saved turns on screen, as the storage's boot packet holds them: what a code block
    // plugin's languages are looked for in. They are the app's own server's (`features/restore`).
    let saved = app.launch.cold_transcript.as_ref().map(|cold| crate::storage::boot_preview(ServerKey::sidecar().as_str(), &cold.id).map(|(_, bytes)| bytes));
    let draws_block = |id: &str| match (&saved, sources::code_languages(id)) {
        (None, _) => false,
        (Some(Some(bytes)), Some(languages)) => languages.iter().any(|language| fenced(bytes, language)),
        // Turns this can't read, or languages the build doesn't know: it starts, as it did.
        (Some(_), _) => true,
    };
    // The debug plugin's one slot is its bar (`window.bottom`), counted above: hidden, it draws nothing.
    // The browser plugin's slots live only on the side panel's New tab (`session.panel.new*`), counted in `empty_side` below.
    // A link handler draws nothing: it answers when a link is opened, which starts it (`commands::open_link`).
    out.extend(sources::builtin_ids().filter(|id| (*id != "debug" && *id != "browser" && has(id, &["Slot"])) || (has(id, &["CodeBlock"]) && draws_block(id))));
    let side_state = app.kernel.slots.get(&crate::panels::SIDE);
    // A subagent's page draws its root's strip (`App::panel_root`).
    let side = side_state.sessions.get(app.panel_root(session));
    let side_open = if app.settings.shared_side_panel {
        side_state.shared_open.unwrap_or(false)
    } else {
        side.is_some_and(|st| st.visibility.open())
    };
    let with_tabs: Vec<&str> = side
        .map(|side| {
            let tabs = &side.tab_state.tabs;
            let shown = |key: &String| match tabs.panes.get(key).copied().unwrap_or(SessionPane::Side) {
                SessionPane::Bottom => side.bottom_open,
                _ => side_open,
            };
            tabs.all.iter().filter(|k| shown(k)).filter_map(|k| crate::panels::tabs::split_panel_key(k).map(|(plugin, _)| plugin)).collect()
        })
        .unwrap_or_default();
    let files = with_tabs.contains(&"file");
    let has_side_tab = side.is_some_and(|side| {
        side.tab_state.tabs.all.iter().any(|k| side.tab_state.tabs.panes.get(k).copied().unwrap_or(SessionPane::Side) != SessionPane::Bottom)
    });
    let empty_side = side_open && !has_side_tab;
    if !with_tabs.is_empty() || empty_side {
        let launcher = |id: &str| (has(id, &["Panel", "MenuItem"]) || id == "browser") && record(id).is_none_or(|record| super::dormant::launcher_needs(&record));
        out.extend(sources::builtin_ids().filter(|id| with_tabs.contains(id) || (files && has(id, &["FileViewer"])) || (empty_side && launcher(id))));
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// Whether `saved` (a session's saved turns, `opencode_kit::model::preview`: their text is in it
/// as written) holds the opening fence of a `language` code block: three or more backticks or
/// tildes, then the language, any case, ending the word. A longer language that starts with it
/// doesn't count; text that only quotes such a fence does, and then the plugin starts as it did.
fn fenced(saved: &[u8], language: &str) -> bool {
    let language = language.as_bytes();
    let mut at = 0;
    while let Some(found) = saved[at..].windows(3).position(|w| w == b"```" || w == b"~~~") {
        let fence = saved[at + found];
        let mut rest = &saved[at + found..];
        let run = rest.iter().take_while(|b| **b == fence).count();
        rest = &rest[run..];
        let pad = rest.iter().take_while(|b| **b == b' ' || **b == b'\t').count();
        rest = &rest[pad..];
        let named = rest.len() >= language.len() && rest[..language.len()].eq_ignore_ascii_case(language);
        if named && rest.get(language.len()).is_none_or(|b| !(b.is_ascii_alphanumeric() || b"-_+#.".contains(b))) {
            return true;
        }
        at += found + run;
    }
    false
}

/// Whether the debug bar is up at launch: the debug plugin's `bar` value says so, or it isn't in
/// memory to say otherwise.
pub(super) fn debug_bar_shown() -> bool {
    let ns = opencode_storage::plugin_namespace("debug");
    let value = match crate::storage::plugin_value(&ns, "bar") {
        Lookup::Pending => crate::storage::boot_packet().map_or(Lookup::Pending, |boot| boot.plugin_value(&ns, "bar")),
        ready => ready,
    };
    match value {
        Lookup::Pending => true,
        Lookup::Ready(v) => v.and_then(|v| serde_json::from_str::<serde_json::Value>(&v).ok()).and_then(|v| v.get("visible")?.as_bool()).unwrap_or(false),
    }
}
