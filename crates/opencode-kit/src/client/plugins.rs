//! Server plugin, skill, and config-document extension endpoints
//! (`packages/protocol/src/groups/plugin.ts`, `anomalyco/opencode#64511`, `#64525`, `#64527`, `#63125`):
//! - `GET /api/plugin`
//! - `POST /api/plugin/check`
//! - `POST /api/plugin/update`
//! - `GET /api/skill`
//! - `GET /api/config`
//!
//! Older OpenCode servers (`v2.2.6`, `v2.3.0`) may answer `404` / `405` / `501` on newer routes
//! such as `/api/plugin/check` and `/api/plugin/update`. All calls return
//! [`PluginEndpoint::Unsupported`] for those statuses so Settings hides update/Console controls
//! quietly with zero error toasts, while preserving typed [`WireError`]s for auth, captive
//! portals, and genuine server failures.

use std::time::Duration;

use serde_json::{Value, json};

use super::Server;
use crate::integration::location_query;
use crate::model::{PluginInfo, SkillInfo, parse_plugins, parse_skills};
use crate::wire_error::{WireError, WireKind};

/// Outcome of calling an extension endpoint that older servers may not implement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PluginEndpoint<T> {
    Supported(T),
    /// Server answered 404/405/501 (not `PluginNotFoundError`): route is absent on this server.
    Unsupported,
}

impl<T> PluginEndpoint<T> {
    pub fn is_supported(&self) -> bool {
        matches!(self, Self::Supported(_))
    }

    pub fn into_option(self) -> Option<T> {
        match self {
            Self::Supported(value) => Some(value),
            Self::Unsupported => None,
        }
    }
}

/// Whether `err` means the server does not expose this plugin/extension route.
///
/// A `404` tagged `PluginNotFoundError` comes from a server that DOES support the plugin API, so
/// it stays a typed error rather than disabling controls. Captive portals, `401` sign-outs, and
/// transport errors also remain typed errors.
pub fn is_plugin_endpoint_unsupported(err: &WireError) -> bool {
    match &err.kind {
        WireKind::Status { status: 404, tag, .. } => tag.as_deref() != Some("PluginNotFoundError"),
        WireKind::Status { status: 405 | 501, .. } => true,
        _ => false,
    }
}

impl Server {
    /// `GET /api/plugin` (`plugin.list`).
    pub fn list_plugins(&self, dir: Option<&str>) -> Result<PluginEndpoint<Vec<PluginInfo>>, WireError> {
        let path = format!("/api/plugin{}", location_query(dir));
        match self.get(&path) {
            Ok(v) => Ok(PluginEndpoint::Supported(parse_plugins(&v))),
            Err(e) if is_plugin_endpoint_unsupported(&e) => Ok(PluginEndpoint::Unsupported),
            Err(e) => Err(e),
        }
    }

    /// `POST /api/plugin/check` (`plugin.check`): checks one or all package plugins for updates.
    pub fn check_plugin_updates(&self, target: Option<&str>) -> Result<PluginEndpoint<Vec<PluginInfo>>, WireError> {
        let body = match target {
            Some(t) => json!({ "target": t }),
            None => json!({}),
        };
        match self.write_within("POST", "/api/plugin/check", Some(body), Duration::from_secs(60)) {
            Ok(v) => Ok(PluginEndpoint::Supported(parse_plugins(&v))),
            Err(e) if is_plugin_endpoint_unsupported(&e) => Ok(PluginEndpoint::Unsupported),
            Err(e) => Err(e),
        }
    }

    /// `POST /api/plugin/update` (`plugin.update`, 204 No Content): installs updates for `targets`
    /// and asks the server to reload affected locations.
    pub fn update_plugins(&self, targets: &[String]) -> Result<PluginEndpoint<()>, WireError> {
        let body = json!({ "targets": targets });
        match self.write_within("POST", "/api/plugin/update", Some(body), Duration::from_secs(120)) {
            Ok(_) => Ok(PluginEndpoint::Supported(())),
            Err(e) if is_plugin_endpoint_unsupported(&e) => Ok(PluginEndpoint::Unsupported),
            Err(e) => Err(e),
        }
    }

    /// `GET /api/skill` (`skill.list`), waiting out startup 503 while plugins load.
    pub fn list_skills(&self, dir: Option<&str>) -> Result<PluginEndpoint<Vec<SkillInfo>>, WireError> {
        let path = format!("/api/skill{}", location_query(dir));
        match self.get_ready(&path) {
            Ok(v) => Ok(PluginEndpoint::Supported(parse_skills(&v))),
            Err(e) if is_plugin_endpoint_unsupported(&e) => Ok(PluginEndpoint::Unsupported),
            Err(e) => Err(e),
        }
    }

    /// `GET /api/config` (`config.get`) entries array for reading Console extension sources or LSPs.
    pub fn config_entries(&self, dir: Option<&str>) -> Result<PluginEndpoint<Vec<Value>>, WireError> {
        let path = format!("/api/config{}", location_query(dir));
        match self.get(&path) {
            Ok(v) => {
                let entries = v["data"]
                    .as_array()
                    .or_else(|| v.as_array())
                    .cloned()
                    .unwrap_or_default();
                Ok(PluginEndpoint::Supported(entries))
            }
            Err(e) if is_plugin_endpoint_unsupported(&e) => Ok(PluginEndpoint::Unsupported),
            Err(e) => Err(e),
        }
    }

    /// Resolved UI feature flags for `directory` (`GET /api/experimental/flag`,
    /// `packages/protocol/src/groups/flag.ts`, `opencode#66509`). Flags not listed are off.
    pub fn flags(&self, directory: Option<&str>) -> Result<std::collections::BTreeMap<String, bool>, WireError> {
        let path = format!("/api/experimental/flag{}", location_query(directory));
        let v = self.get(&path)?;
        let obj = v.get("data").and_then(Value::as_object).or_else(|| v.as_object());
        Ok(obj.into_iter().flatten().filter_map(|(k, v)| v.as_bool().map(|b| (k.clone(), b))).collect())
    }
}
