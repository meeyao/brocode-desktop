//! The host signals that follow app state rather than an event: `Sessions.Changed`,
//! `Sessions.RoutedChanged`, `Models.Changed`, `Theme.Changed`, `I18n.Changed`,
//! `App.LocaleChanged`, `Server.Connected`, `Servers.Changed`, each store's, contract's and
//! editor's `Changed`, an editor's `FocusChanged`, and `Browser.Changed`. Each poll compares what a signal follows with what it last saw and fires on a
//! change; a plugin that made none of them costs one map lookup.

use std::hash::{Hash, Hasher};

use rune_luau::Data;

use super::session::Stamp;
use super::store::{contract_generation, store_version};
use super::{map, session_version};
use crate::App;

fn hash(v: impl Hash) -> u64 {
    let mut h = std::hash::DefaultHasher::new();
    v.hash(&mut h);
    h.finish()
}

/// Inputs of `session_version`. A change outside the transcript can affect several rows
/// (in particular a running child's ancestors or its family's cost). Cell identity matters
/// as well as version: checking out a server swaps its cells into the window.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct SessionInputs {
    server: opencode_kit::servers::ServerKey,
    shared: [Stamp; 5],
    transcript: Stamp,
}

impl SessionInputs {
    fn read(app: &App) -> Self {
        Self {
            // Polling a parked server checks out and restores these cells within one turn.
            server: crate::servers::runtime::active(app).clone(),
            shared: [Stamp::of(&app.sessions), Stamp::of(&app.projects), Stamp::of(&app.running), Stamp::of(&app.inboxes), Stamp::of(&app.context_windows)],
            transcript: Stamp::of(&app.transcripts),
        }
    }
}

/// Fires plugin `i`'s watched signals whose state moved. True when any fired.
pub fn fire_watched(app: &App, i: usize, vm: u64) -> bool {
    let half = app.plugins.list[i].half.clone();
    let names: Vec<String> = half.signals.borrow().keys().filter(|(v, _)| *v == vm).map(|(_, n)| n.clone()).collect();
    if names.is_empty() {
        return false;
    }
    let mut fired = false;
    for name in names {
        if name == "Sessions.Changed" {
            let inputs = SessionInputs::read(app);
            let previous = half.sessions_read.replace(Some(inputs.clone()));
            if previous.as_ref() == Some(&inputs) {
                continue;
            }
            let only = previous.filter(|p| p.server == inputs.server && p.shared == inputs.shared && p.transcript.0 == inputs.transcript.0)
                .and_then(|p| app.transcript_edits.only(inputs.transcript.0, p.transcript.1, inputs.transcript.1));
            let mut key_buf = String::with_capacity(64);
            let mut compare = |id: &str, v| {
                key_buf.clear();
                key_buf.push_str("Sessions.Changed:");
                key_buf.push_str(id);
                let changed = {
                    let mut seen = half.seen.borrow_mut();
                    match seen.get_mut(&key_buf) {
                        Some(prev) if *prev == v => false,
                        Some(prev) => {
                            *prev = v;
                            true
                        }
                        None => {
                            seen.insert(key_buf.clone(), v);
                            false
                        }
                    }
                };
                if changed {
                    fired |= super::super::fire_named(app, i, &name, session_data(app, id));
                }
            };
            if let Some(id) = only {
                if let Some(info) = app.session(id) {
                    compare(id, super::session::session_version_for(app, info, app.subtree_running(id)));
                }
            } else {
                let running_ancestors = running_ancestors(app);
                // Snapshot every version before dispatching the first change.
                let versions: Vec<_> = app
                    .sessions
                    .iter()
                    .map(|s| (s.id.as_str(), super::session::session_version_for(app, s, running_ancestors.contains(s.id.as_str()))))
                    .collect();
                for (id, v) in versions { compare(id, v); }
            }
            continue;
        }
        // Each page whose address, title or loading moved, or that closed, by its id.
        if name == "Browser.Changed" {
            let include_find = half.is_builtin("browser");
            let pages = super::super::native::browser::pages(app, include_find);
            let mut seen = half.seen.borrow_mut();
            let gone: Vec<String> = seen.keys().filter_map(|k| k.strip_prefix("Browser.Changed:")).filter(|id| !pages.iter().any(|(p, _)| p == id)).map(str::to_owned).collect();
            let mut moved: Vec<String> = Vec::new();
            for id in gone {
                seen.remove(&format!("Browser.Changed:{id}"));
                moved.push(id);
            }
            for (id, v) in pages {
                if seen.insert(format!("Browser.Changed:{id}"), v) != Some(v) {
                    moved.push(id);
                }
            }
            drop(seen);
            for id in moved {
                fired |= super::super::fire_named(app, i, &name, Data::String(id));
            }
            continue;
        }
        let Some((v, payload)) = observe(app, &half.id, &name) else { continue };
        let was = half.seen.borrow_mut().insert(name.clone(), v);
        if was.is_some_and(|w| w != v) {
            fired |= super::super::fire_named(app, i, &name, payload);
        }
    }
    fired
}

/// What signal `name` of plugin `plugin` follows, as a version, and the payload it fires with.
/// `Sessions.Changed`, which follows each session apart, isn't one.
fn observe(app: &App, plugin: &str, name: &str) -> Option<(u64, Data)> {
    match name {
        // The routed session, and whether its data has loaded: a window routed at launch
        // (a restored tab) knows the id before the session, which `Routed` returns only then.
        "Sessions.RoutedChanged" => {
            let id = app.active_session_id();
            Some((hash((id, id.is_some_and(|id| app.session(id).is_some()))), id.map_or(Data::Nil, |id| session_data(app, id))))
        }
        "Models.Changed" => Some((super::session::models_version(app), Data::Nil)),
        "Composer.PickerChanged" => {
            let kind = app.composer_ext.pickers.open().map(crate::pickers::Kind::key);
            Some((hash(kind), kind.map_or(Data::Nil, |k| Data::String(k.to_owned()))))
        }
        "Theme.Changed" => {
            let p = crate::palette::pal(app.plugins.dark.get());
            Some((hash((p.theme, p.generation, p.dark)), map([("scheme", Data::String(if p.dark { "dark" } else { "light" }.into()))])))
        }
        "I18n.Changed" | "App.LocaleChanged" => Some((hash(opencode_i18n::locale().id()), Data::String(opencode_i18n::locale().id().to_owned()))),
        // The event connection of the server the window works against (the routed tab's): it
        // fires as that connection drops or comes back, and with the new one's state as the
        // window moves to another server.
        "Server.Connected" => {
            let connected = app.live() && !crate::reconnect::down(app);
            Some((hash((app.server_key(), connected)), Data::Bool(connected)))
        }
        "Servers.Changed" => Some((super::servers_version(app), Data::Nil)),
        "ctx.FocusChanged" => {
            let focused = !app.plugins.window_blurred.get();
            Some((u64::from(focused), Data::Bool(focused)))
        }
        "Updater.StateChanged" => Some((crate::updater::revision(app), crate::updater::state_data(app))),
        "Browser.AddressSelectionChanged" if plugin == crate::surfaces::browser::PLUGIN => {
            let (start, end, len, composing, focused) = super::super::editors::selection_in(app, plugin, 0);
            Some((
                hash((start, end, len, composing, focused)),
                map([
                    ("start", Data::Number(start as f64)),
                    ("end", Data::Number(end as f64)),
                    ("len", Data::Number(len as f64)),
                    ("composing", Data::Bool(composing)),
                    ("focused", Data::Bool(focused)),
                ]),
            ))
        }
        n => {
            let editor = |suffix: &str| n.strip_prefix("editor.").and_then(|s| s.strip_suffix(suffix)).and_then(|s| s.parse::<usize>().ok());
            if let Some(index) = editor(".FocusChanged") {
                let focused = super::super::editors::focused_in(app, plugin, index);
                Some((hash(focused), Data::Bool(focused)))
            } else if let Some(index) = editor(".Changed") {
                let text = super::super::editors::text_in(app, plugin, index);
                Some((hash(&text), Data::String(text)))
            } else if let Some(store) = n.strip_prefix("Store.").and_then(|s| s.strip_suffix(".Changed")) {
                Some((store_version(plugin, store), Data::Nil))
            } else if let Some(session) = n.strip_prefix("Diff.").and_then(|s| s.strip_suffix(".Changed")) {
                Some((hash(super::super::native::diff::version(app, session)), Data::Nil))
            } else {
                n.strip_prefix("Contracts.").and_then(|s| s.strip_suffix(".Changed")).map(|id| (contract_generation(id), Data::Nil))
            }
        }
    }
}

/// Records post-handler stamps for a plugin editor's `Changed` (and the browser's
/// `AddressSelectionChanged`) after synchronous `input_changed` dispatch so the next poll does not
/// fire a duplicate change or a spurious caret-moved event after `SetAddressSelection`.
pub fn sync_editor_watches(app: &App, p: &crate::plugin_host::Plugin, index: usize) {
    let changed = format!("editor.{index}.Changed");
    let mut seen = p.half.seen.borrow_mut();
    if let Some((v, _)) = observe(app, &p.half.id, &changed) {
        seen.insert(changed, v);
    }
    if p.half.is_builtin("browser") && index == 0 {
        if let Some((v, _)) = observe(app, &p.half.id, "Browser.AddressSelectionChanged") {
            seen.insert("Browser.AddressSelectionChanged".into(), v);
        }
    }
}

/// Running sessions and their ancestors (up to 32 levels), precomputed once per bulk
/// `Sessions.Changed` sweep so each catalog session checks a set instead of walking parents.
pub(super) fn running_ancestors(app: &App) -> std::collections::HashSet<&str> {
    let mut out = std::collections::HashSet::new();
    for r in app.running.iter() {
        for cur in std::iter::successors(Some(r.as_str()), |cur| app.session(cur)?.parent.as_deref()).take(32) {
            out.insert(cur);
        }
    }
    out
}

/// Takes what signal `name` of plugin half `half` follows as it is when the plugin connects to
/// it, so a change between its setup and the next poll (a launch's routed session loading)
/// fires it rather than becoming its first value.
pub fn baseline(app: &App, half: &crate::plugin_host::Half, name: &str) {
    let mut seen = half.seen.borrow_mut();
    if name == "Sessions.Changed" {
        let running = running_ancestors(app);
        for s in &app.sessions {
            seen.entry(format!("Sessions.Changed:{}", s.id))
                .or_insert_with(|| super::session::session_version_for(app, s, running.contains(s.id.as_str())));
        }
        return;
    }
    if let Some((v, _)) = observe(app, &half.id, name) {
        seen.entry(name.to_owned()).or_insert(v);
    }
}

/// A session as `Data`, the record `Sessions:Get` returns.
pub fn session_data(app: &App, id: &str) -> Data {
    let Some(info) = app.session(id) else { return Data::Nil };
    map([
        ("id", Data::String(info.id.clone())),
        ("directory", Data::String(info.directory.clone())),
        ("title", Data::String(info.title.clone())),
        ("busy", Data::Bool(app.subtree_running(&info.id))),
        ("version", Data::Integer((session_version(app, id) >> 11) as i64)),
        ("cost", Data::Number(app.session_cost(id))),
    ])
}
