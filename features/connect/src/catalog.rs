//! The integrations and providers the connect dialog and Settings read, by location (Desktop's
//! `useIntegrations` and `useProviders`): `/api/integration` and `/api/provider` per folder, the
//! server's default one under `""`. A sync reads them again with the model list, which the
//! composer's pickers take too.

use std::collections::HashMap;
use std::time::Duration;

use opencode_kit::client::Server;
use opencode_kit::integration::{Integration, location_query};
use rune::Tx;
use serde_json::Value;

use crate::{ConnectHost, ConnectJob, ConnectRequest};

/// A provider the server runs (`/api/provider`).
#[derive(Clone, Debug, PartialEq)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    /// The integration that connects it, where that isn't its own id (Console's providers).
    pub integration: Option<String>,
}

#[derive(Default)]
pub struct Catalogs {
    integrations: HashMap<String, Vec<Integration>>,
    providers: HashMap<String, Vec<ProviderInfo>>,
}

impl Catalogs {
    /// The integrations at `dir` (the server's default when none), once read.
    pub fn integrations(&self, dir: Option<&str>) -> Option<&[Integration]> {
        self.integrations.get(&slot(dir)).map(Vec::as_slice)
    }

    pub fn integration_of(&self, dir: Option<&str>, id: &str) -> Option<&Integration> {
        self.integrations(dir)?.iter().find(|i| i.id == id)
    }

    /// The providers the server runs at `dir`, once read.
    pub fn provider_infos(&self, dir: Option<&str>) -> Option<&[ProviderInfo]> {
        self.providers.get(&slot(dir)).map(Vec::as_slice)
    }

    /// Puts a scene's integrations and providers in the server's default slot, as a sync would.
    pub fn seed(&mut self, integrations: Vec<Integration>, providers: Vec<ProviderInfo>) {
        self.integrations.insert(String::new(), integrations);
        self.providers.insert(String::new(), providers);
    }

    pub fn insert_integrations(&mut self, dir: Option<&str>, list: Vec<Integration>) {
        self.integrations.insert(slot(dir), list);
    }

    pub fn insert_providers(&mut self, dir: Option<&str>, list: Vec<ProviderInfo>) {
        self.providers.insert(slot(dir), list);
    }
}

/// What one catalog sync read from the server; a part the server didn't answer stays as it was.
#[derive(Clone, Debug)]
pub struct CatalogRead {
    pub integrations: Option<Vec<Integration>>,
    pub providers: Option<Value>,
    pub models: Option<(Value, Value)>,
}

/// Continuation run on the UI thread after a catalog sync completes.
#[derive(Clone, Debug)]
pub enum SyncAfter {
    None,
    AccountSettled {
        session: u32,
        attempt: u32,
    },
    ClaudeConfirm {
        session: u32,
        check: u32,
    },
    RefreshConnection {
        dir: Option<String>,
        provider: String,
        session: u32,
        attempt: u32,
        tries: Option<u32>,
    },
}

fn slot(dir: Option<&str>) -> String {
    dir.unwrap_or_default().to_owned()
}

/// brocode: a catalog read without OpenCode's own providers/models (`/api/provider`, `/api/model`).
fn without_opencode(v: &Value, field: &str) -> Value {
    let keep = |x: &Value| !x.get(field).and_then(Value::as_str).unwrap_or_default().starts_with("opencode");
    let mut out = v.clone();
    if out.get("data").is_some_and(Value::is_array) {
        if let Some(arr) = out.get_mut("data").and_then(Value::as_array_mut) {
            arr.retain(keep);
        }
    } else if let Some(arr) = out.as_array_mut() {
        arr.retain(keep);
    }
    out
}

pub fn parse_providers(v: &Value) -> Vec<ProviderInfo> {
    v["data"]
        .as_array()
        .or_else(|| v.as_array())
        .into_iter()
        .flatten()
        .filter(|p| !p["id"].as_str().unwrap_or_default().starts_with("opencode"))
        .map(|p| ProviderInfo {
            id: p["id"].as_str().unwrap_or_default().to_owned(),
            name: p["name"].as_str().unwrap_or_default().to_owned(),
            integration: p["integrationID"].as_str().map(str::to_owned),
        })
        .collect()
}

/// The integrations at `dir` (the server's default when none), once read.
pub fn integrations<'a>(host: &'a dyn ConnectHost, dir: Option<&str>) -> Option<&'a [Integration]> {
    host.catalogs().integrations(dir)
}

pub fn integration_of<'a>(host: &'a dyn ConnectHost, dir: Option<&str>, id: &str) -> Option<&'a Integration> {
    host.catalogs().integration_of(dir, id)
}

/// The providers the server runs at `dir`, once read.
pub fn provider_infos<'a>(host: &'a dyn ConnectHost, dir: Option<&str>) -> Option<&'a [ProviderInfo]> {
    host.catalogs().provider_infos(dir)
}

/// Puts a scene's integrations and providers in the server's default slot, as a sync would.
pub fn seed_catalogs(host: &mut dyn ConnectHost, integrations: Vec<Integration>, providers: Vec<ProviderInfo>) {
    host.catalogs_mut().seed(integrations, providers);
}

/// Reads `dir`'s integrations, providers, and model list again.
pub fn sync_catalogs(host: &mut dyn ConnectHost, dir: Option<String>) {
    sync_catalogs_with(host, dir, SyncAfter::None);
}

pub(crate) fn sync_catalogs_with(host: &mut dyn ConnectHost, dir: Option<String>, after: SyncAfter) {
    host.spawn_job(ConnectJob::Server {
        delay: Duration::ZERO,
        request: ConnectRequest::SyncCatalogs { dir, after },
    });
}

/// Executes the off-thread reads for one catalog sync against `server`.
pub(crate) fn fetch_read(server: &Server, dir: Option<&str>) -> CatalogRead {
    let q = location_query(dir);
    CatalogRead {
        integrations: server.integrations(dir).ok(),
        providers: server.get(&format!("/api/provider{q}")).ok(),
        models: server.get("/api/model").ok().map(|m| (m, server.get("/api/agent").unwrap_or_default())),
    }
}

/// Applies one completed [`CatalogRead`] to the host's catalogs and pickers, returning whether
/// every part answered.
pub(crate) fn apply_read(host: &mut dyn ConnectHost, dir: Option<&str>, read: CatalogRead, tx: &mut Tx) -> bool {
    let ok = read.integrations.is_some() && read.providers.is_some() && read.models.is_some();
    if let Some(list) = read.integrations {
        // brocode: no OpenCode Console/Go/Zen rows in this build's provider list.
        let list: Vec<_> = list.into_iter().filter(|i| !i.id.starts_with("opencode")).collect();
        host.catalogs_mut().insert_integrations(dir, list);
    }
    if let Some(p) = &read.providers {
        let p = without_opencode(p, "id");
        host.catalogs_mut().insert_providers(dir, parse_providers(&p));
        host.load_providers(p.get("data").unwrap_or(&p));
    }
    if let Some((m, agents)) = &read.models {
        let m = without_opencode(m, "providerID");
        host.load_models(m.get("data").unwrap_or(&m));
        host.load_agents(agents.get("data").unwrap_or(agents));
    }
    tx.instant();
    crate::catalogs_changed(host, tx);
    host.on_models_changed(tx);
    ok
}
