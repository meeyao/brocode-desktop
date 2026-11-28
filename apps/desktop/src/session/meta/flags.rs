//! Server UI feature flags (`GET /api/experimental/flag?location[directory]=…`,
//! `packages/app/src/shell/routes/session-ui-provider.tsx:266`, `opencode#66509`).
//!
//! Each `(server, directory)` slot moves explicitly through `Idle -> Pending -> Loaded(map)`;
//! flags not listed by the server default to `false`.

use std::collections::{BTreeMap, HashMap};

use opencode_kit::client::Request;
use opencode_kit::servers::ServerKey;

use crate::{App, Route, Source};

/// Per-window resolved server UI feature flags, stored in `opencode_kernel::StateSlots`.
pub(crate) static FLAGS: opencode_kernel::Slot<LocationFlags> = opencode_kernel::Slot::new();

#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum FlagEntry {
    #[default]
    Idle,
    Pending,
    Loaded(BTreeMap<String, bool>),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Key {
    server: ServerKey,
    directory: Option<String>,
}

#[derive(Default)]
pub(crate) struct LocationFlags {
    entries: HashMap<Key, FlagEntry>,
}

enum Event {
    Ensure(Key),
    Loaded { key: Key, flags: BTreeMap<String, bool> },
    ClearServer(ServerKey),
}

enum Effect {
    Fetch(Option<String>),
}

fn step(state: &mut LocationFlags, event: Event) -> Option<Effect> {
    match event {
        Event::Ensure(key) => match state.entries.get(&key) {
            Some(FlagEntry::Pending | FlagEntry::Loaded(_)) => None,
            None | Some(FlagEntry::Idle) => {
                let dir = key.directory.clone();
                state.entries.insert(key, FlagEntry::Pending);
                Some(Effect::Fetch(dir))
            }
        },
        Event::Loaded { key, flags } => {
            state.entries.insert(key, FlagEntry::Loaded(flags));
            None
        }
        Event::ClearServer(server) => {
            state.entries.retain(|k, _| k.server != server);
            None
        }
    }
}

/// Polls flag loading for the active session's directory on the active live server.
pub(crate) fn poll(s: &mut App) -> bool {
    let requests = match &*s.source {
        Source::Live { requests: Some(r), .. } => r.clone(),
        _ => return false,
    };
    let directory = match &*s.route {
        Route::Session { id, .. } => s.session(id).map(|info| info.directory.clone()).filter(|d| !d.is_empty()),
        _ => None,
    };
    let key = Key { server: crate::servers::runtime::active(s).clone(), directory };
    let effect = step(s.kernel.slots.get_mut(&FLAGS), Event::Ensure(key));
    if let Some(Effect::Fetch(dir)) = effect {
        requests.send(Request::Flags { directory: dir }).ok();
    }
    false
}

/// Records a `Request::Flags` reply for the active server.
pub(crate) fn answered(s: &mut App, directory: Option<String>, flags: BTreeMap<String, bool>) {
    let key = Key { server: crate::servers::runtime::active(s).clone(), directory };
    let _ = step(s.kernel.slots.get_mut(&FLAGS), Event::Loaded { key, flags });
}

/// Drops cached flags for the active server so reconnect or reload reads fresh values.
pub(crate) fn reset_server(s: &mut App) {
    let server = crate::servers::runtime::active(s).clone();
    let _ = step(s.kernel.slots.get_mut(&FLAGS), Event::ClearServer(server));
}

/// Sets a feature flag directly for stories and headless verification scenes.
pub(crate) fn set_fixture(s: &mut App, directory: Option<String>, name: &str, enabled: bool) {
    let key = Key { server: crate::servers::runtime::active(s).clone(), directory };
    let store = s.kernel.slots.get_mut(&FLAGS);
    match store.entries.get_mut(&key) {
        Some(FlagEntry::Loaded(map)) => {
            map.insert(name.to_owned(), enabled);
        }
        _ => {
            let mut map = BTreeMap::new();
            map.insert(name.to_owned(), enabled);
            store.entries.insert(key, FlagEntry::Loaded(map));
        }
    }
}

/// Reads boolean flag `name` for `session`'s directory on the active server (`false` if unset).
pub(crate) fn flag(s: &App, session: &str, name: &str) -> bool {
    let directory = s.session(session).map(|info| info.directory.clone()).filter(|d| !d.is_empty());
    let store = s.kernel.slots.get(&FLAGS);
    let lookup = |dir: Option<String>| {
        let key = Key { server: crate::servers::runtime::active(s).clone(), directory: dir };
        match store.entries.get(&key) {
            Some(FlagEntry::Loaded(map)) => map.get(name).copied(),
            _ => None,
        }
    };
    lookup(directory).or_else(|| lookup(None)).unwrap_or(false)
}

/// The server's `full_codemode` flag (`packages/app/src/session/timeline/message-timeline.tsx:126`).
pub(crate) fn full_codemode(s: &App, session: &str) -> bool {
    flag(s, session, "full_codemode")
}
