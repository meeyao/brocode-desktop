//! What the composer and the session header say about the model before the server answers: the
//! models the window's sessions run with, their providers' names, the agents and the configured
//! model, kept in the app's saved state (the boot record) and loaded before the first frame.
//!
//! Without it the first frame knows a session's model only by its ids: the composer's button
//! says `provider-id / model-id` and widens to the names when the catalog lands, the room beside
//! it narrows, and the header's usage button shows a cost (an unknown model's rule) that goes
//! again with the catalog, which moves the title's box. With the server answering in 15 ms that
//! is a jump in the second frame; with none, the ids stay.
//!
//! OpenCode Desktop keeps no such copy: its window shows a splash until its server is ready
//! (`packages/desktop/src/renderer/startup/machine.ts`).
//!
//! It holds what those two places draw from, as the server's lists name it, so the catalog's own
//! loaders read it back: a model's name, efforts, context window and whether its tokens cost
//! money. Not what a list of models needs (what it reads, its speeds, whether it is free): the
//! server's catalog replaces all of it when it lands, and a model that is another's speed shows
//! under its own name until then.
//!
//! Written with the saved session list (`saved_list`): when the first reads land and as a
//! window hides, closes or quits.

use opencode_kit::model::ModelRef;
use serde_json::{Value, json};

use crate::App;

/// Desktop's global storage name: what every window shares, all of it in the boot record.
const GLOBAL: &str = "opencode.global.dat";
const KEY: &str = "rune.launch.catalog";
/// The server's `/api/config` as it last answered, for [`restore`].
const CONFIG: &str = "rune.launch.config";
const VERSION: u64 = 1;
/// More models than a window's tabs run with.
const MODELS: usize = 16;

/// Before the first frame of a window whose server answers after it: the saved catalog.
pub(crate) fn restore(s: &mut App) {
    if !s.persists() || !crate::servers::runtime::active(s).is_sidecar() || !s.composer_ext.pickers.models.is_empty() {
        return;
    }
    let Some(saved) = crate::storage::json(GLOBAL, KEY).filter(|v| v["v"].as_u64() == Some(VERSION)) else { return };
    rune::marks::mark("state.catalog");
    let pickers = &mut s.composer_ext.pickers;
    pickers.load_providers(&saved["providers"]);
    pickers.load_config(&saved["config"]);
    pickers.load_models(&saved["models"]);
    pickers.load_agents(&saved["agents"]);
}

/// Before the first frame: the server's `/api/config` as it last answered, so a new session
/// starts on the model the server names (brocode/pro), never on an id guessed from nothing. Its
/// providers, when there is no catalog copy, keep another provider's models out of the pickers
/// until the server's list lands.
pub(crate) fn restore_config(s: &mut App) {
    if !s.persists() || !crate::servers::runtime::active(s).is_sidecar() {
        return;
    }
    let Some(config) = crate::storage::json(GLOBAL, CONFIG) else { return };
    let pickers = &mut s.composer_ext.pickers;
    if pickers.provider_order.is_empty()
        && let Some(ids) = config.as_array().into_iter().flatten().find_map(|e| e["info"]["providers"].as_object()).map(|p| p.keys().cloned().collect::<Vec<String>>())
    {
        let providers: Vec<Value> = ids.into_iter().map(|id| json!({ "id": id })).collect();
        pickers.load_providers(&Value::Array(providers));
    }
    pickers.load_config(&config);
}

/// The server's `/api/config` as it last answered, for the next window's [`restore`]: which
/// model new sessions run with and which providers exist, before its server says.
pub(crate) fn remember_config(config: &Value) {
    crate::storage::set_json(GLOBAL, CONFIG, config);
}

/// Keeps what the window's sessions run with, for the next launch. Only from a catalog the
/// server gave: a window still showing the saved list writes nothing.
pub(crate) fn save(s: &App) {
    let own = crate::servers::runtime::active(s);
    let pickers = &s.composer_ext.pickers;
    if !s.persists() || !own.is_sidecar() || s.launch.saved_list.is_some() || !matches!(s.launch.phase(own.as_str()), Some(super::super::Phase::Connected)) || pickers.models.is_empty() {
        return;
    }
    // The models on screen first: the open session's pick, the tabs' sessions', then what a new
    // session would start with.
    let mut wanted: Vec<ModelRef> = Vec::new();
    let mut want = |m: ModelRef| {
        if !m.id.is_empty() && !wanted.iter().any(|w| w.id == m.id && w.provider == m.provider) {
            wanted.push(m);
        }
    };
    want(crate::pickers::PickerHost::selection(s).model);
    for tab in s.tabs.iter().filter(|t| !t.is_draft()) {
        if let Some(info) = s.sessions.iter().find(|x| x.id == tab.id) {
            want(info.model.clone());
        }
    }
    pickers.fallback_model().into_iter().for_each(&mut want);
    pickers.configured.iter().cloned().for_each(&mut want);
    let mut agents: Vec<&String> = pickers.agent_models.keys().collect();
    agents.sort();
    agents.into_iter().filter_map(|a| pickers.agent_models.get(a)).cloned().for_each(&mut want);
    let paid = |m: &opencode_kit::model::Model| pickers.paid.contains(&m.key);
    let models: Vec<Value> = wanted
        .iter()
        .take(MODELS)
        .filter_map(|m| pickers.models.iter().find(|x| x.id == m.id && x.provider == m.provider))
        .map(|m| {
            let mut v = json!({
                "id": m.id,
                "providerID": m.provider,
                "name": if m.latest { format!("{} (latest)", m.name) } else { m.name.clone() },
                "variants": m.variants.iter().map(|id| json!({ "id": id })).collect::<Vec<_>>(),
                // Whether its tokens cost money is all the header's usage asks of the cost, and
                // whether its input does all that makes one of OpenCode's models free.
                "cost": [{ "input": if paid(m) && !m.free { 1.0 } else { 0.0 }, "output": if paid(m) { 1.0 } else { 0.0 } }],
            });
            if let Some(context) = pickers.limits.get(&m.key) {
                v["limit"] = json!({ "context": context });
            }
            v
        })
        .collect();
    if models.is_empty() {
        return;
    }
    let providers: Vec<Value> = pickers.provider_order.iter().map(|id| json!({ "id": id, "name": pickers.providers.get(id).unwrap_or(id) })).collect();
    let agents: Vec<Value> = pickers
        .agents
        .iter()
        .map(|id| match pickers.agent_models.get(id) {
            Some(m) => json!({ "id": id, "model": { "id": m.id, "providerID": m.provider, "variant": m.variant } }),
            None => json!({ "id": id }),
        })
        .collect();
    // The server's own `/api/config` here, not only the models it said: the agents it names and
    // which providers it has decide a new window's model before its server answers.
    let mut config: Vec<Value> = crate::storage::json(GLOBAL, CONFIG).into_iter().collect();
    config.extend(pickers.configured.iter().map(|m| json!({ "type": "document", "info": { "model": { "providerID": m.provider, "model": m.id, "variant": m.variant } } })));
    let value = json!({ "v": VERSION, "providers": providers, "config": config, "models": models, "agents": agents });
    // An unchanged catalog writes nothing (`storage::set`).
    crate::storage::set_json(GLOBAL, KEY, &value);
}
