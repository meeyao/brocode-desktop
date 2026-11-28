//! Off-thread server jobs scheduled by Settings pages (`SettingsJob`, `SettingsRequest`,
//! `SettingsReply`) and applied back on the UI thread via [`apply_reply`].

use opencode_i18n::t;
use opencode_kit::client::{PluginEndpoint, Server};
use opencode_kit::integration::location_query;
use opencode_kit::model::{ConsoleMcp, PluginInfo, Project, SkillInfo, console_mcps, parse_project, plugin_labels};
use opencode_kit::servers::ServerKey;
use opencode_kit::wire_error::{WireError, WireKind};
use opencode_search::query::percent_encode;
use rune::Tx;
use serde_json::{Value, json};

use crate::host::SettingsHost;
use crate::pages::extensions;
use crate::pages::plugin_update;
use crate::pages::project::Field as ProjectField;
use crate::pages::project_extensions::{Lsp, SkillPairs, configured_lsps_entries};
use crate::pages::servers::global_shell;
use crate::pages::worktrees::{self, Inspection, Skip, Target};

/// Background server request dispatched by [`SettingsHost::spawn_job`].
#[derive(Clone, Debug)]
pub enum SettingsJob {
    /// Run `request` against the window's active server.
    ActiveServer(SettingsRequest),
    /// Run `request` against a specific server identified by `server`.
    ForServer { server: ServerKey, request: SettingsRequest },
}

/// Off-thread server work executed by [`execute_request`].
#[derive(Clone, Debug)]
pub enum SettingsRequest {
    LoadExtensions {
        server: ServerKey,
        active_dir: Option<String>,
    },
    ReloadPlugins {
        server: ServerKey,
    },
    ReloadSkills {
        server: ServerKey,
    },
    ReloadConsoleSources {
        server: ServerKey,
        active_dir: Option<String>,
    },
    CheckPluginUpdates {
        server: ServerKey,
        attempt: u64,
    },
    UpdatePlugin {
        server: ServerKey,
        id: u64,
        target: String,
    },
    LoadProjectExtensions {
        dir: String,
    },
    CheckProjectLocation {
        dir: String,
    },
    PatchProject {
        id: String,
        field: ProjectField,
        body: Value,
    },
    RenameProject {
        id: String,
        name: String,
    },
    DisconnectProvider {
        id: String,
        name: String,
    },
    ActivateAccount {
        credential: String,
        provider: String,
        label: String,
    },
    RemoveAccount {
        credential: String,
        provider: String,
        label: String,
    },
    RenameAccount {
        credential: String,
        label: String,
    },
    LoadServerShells {
        key: ServerKey,
    },
    SetServerShell {
        key: ServerKey,
        value: String,
        previous: String,
    },
    InspectWorktree {
        attempt: u64,
        directory: String,
        active: Option<String>,
        linked: bool,
    },
    DeleteWorktree {
        attempt: u64,
        target: Target,
        force: bool,
        active: Option<String>,
        linked: bool,
    },
}

/// Outcome of `POST /api/plugin/check`.
#[derive(Clone, Debug)]
pub enum PluginCheckResult {
    Supported(Vec<PluginInfo>),
    Unsupported,
    Failed(String),
}

/// Outcome of `POST /api/plugin/update`.
#[derive(Clone, Debug)]
pub enum PluginUpdateResult {
    Succeeded,
    Unsupported,
    Failed(String),
}

/// Result of a [`SettingsRequest`], applied on the UI thread by [`apply_reply`].
#[derive(Clone, Debug)]
pub enum SettingsReply {
    ExtensionsLoaded {
        server: ServerKey,
        plugins: Result<Vec<PluginInfo>, ()>,
        skills: Option<Vec<SkillInfo>>,
        sources: Result<Vec<ConsoleMcp>, ()>,
    },
    PluginsReloaded {
        server: ServerKey,
        plugins: Result<Vec<PluginInfo>, ()>,
    },
    SkillsReloaded {
        server: ServerKey,
        skills: Option<Vec<SkillInfo>>,
    },
    ConsoleSourcesReloaded {
        server: ServerKey,
        sources: Result<Vec<ConsoleMcp>, ()>,
    },
    PluginCheckSettled {
        server: ServerKey,
        attempt: u64,
        result: PluginCheckResult,
    },
    PluginUpdateSettled {
        server: ServerKey,
        id: u64,
        result: PluginUpdateResult,
    },
    ProjectExtensionsLoaded {
        dir: String,
        plugins: Result<(Vec<String>, Vec<String>), ()>,
        skills: Option<SkillPairs>,
        sources: Result<Vec<ConsoleMcp>, ()>,
        lsps: Result<(bool, Vec<Lsp>), ()>,
    },
    ProjectLocationChecked {
        dir: String,
        /// `Some(true)` only on server-reported `LocationNotFoundError`; `None` leaves prior state intact.
        missing: Option<bool>,
    },
    ProjectPatched {
        field: ProjectField,
        updated: Option<Project>,
    },
    ProjectRenamed {
        result: Result<Option<Project>, WireError>,
    },
    ProviderDisconnected {
        name: String,
        result: Result<(), String>,
    },
    AccountWriteSettled {
        title: String,
        description: String,
        result: Result<(), String>,
    },
    ServerShellsLoaded {
        key: ServerKey,
        list: Option<Vec<(String, String, bool)>>,
        current: Option<String>,
    },
    ServerShellSaved {
        key: ServerKey,
        previous: String,
        failed: Option<WireError>,
    },
    WorktreeInspected {
        attempt: u64,
        inspection: Option<Inspection>,
    },
    WorktreeDeleted {
        attempt: u64,
        target: Target,
        result: Result<(), Skip>,
    },
}

fn format_wire_error(err: &WireError) -> String {
    match &err.kind {
        WireKind::Status { message: Some(m), .. } if !m.is_empty() => m.clone(),
        _ => err.to_string(),
    }
}

fn load_plugins(srv: &Server, dir: Option<&str>) -> Result<Vec<PluginInfo>, ()> {
    match srv.list_plugins(dir) {
        Ok(PluginEndpoint::Supported(list)) => Ok(list),
        Ok(PluginEndpoint::Unsupported) => Ok(Vec::new()),
        Err(_) => Err(()),
    }
}

fn load_console_sources(srv: &Server, active_dir: Option<&str>) -> Result<Vec<ConsoleMcp>, ()> {
    let entries = match srv.config_entries(None) {
        Ok(PluginEndpoint::Supported(entries)) => entries,
        Ok(PluginEndpoint::Unsupported) => return Ok(Vec::new()),
        Err(_) => return Err(()),
    };
    let active = match active_dir.filter(|d| !d.is_empty()) {
        Some(dir) => match srv.config_entries(Some(dir)) {
            Ok(PluginEndpoint::Supported(a)) => Some(a),
            Ok(PluginEndpoint::Unsupported) => None,
            Err(_) => return Err(()),
        },
        None => None,
    };
    Ok(console_mcps(&entries, active.as_deref()))
}

/// Executes a [`SettingsRequest`] off the UI thread against `srv`.
pub fn execute_request(srv: &Server, request: SettingsRequest) -> SettingsReply {
    match request {
        SettingsRequest::LoadExtensions { server, active_dir } => {
            let plugins = load_plugins(srv, None);
            // 503 until the server's plugins load; a list that still fails keeps the last one.
            let skills = srv.list_skills(None).ok().map(|ep| ep.into_option().unwrap_or_default());
            let sources = load_console_sources(srv, active_dir.as_deref());
            SettingsReply::ExtensionsLoaded { server, plugins, skills, sources }
        }
        SettingsRequest::ReloadPlugins { server } => {
            let plugins = load_plugins(srv, None);
            SettingsReply::PluginsReloaded { server, plugins }
        }
        SettingsRequest::ReloadSkills { server } => {
            let skills = srv.list_skills(None).ok().map(|ep| ep.into_option().unwrap_or_default());
            SettingsReply::SkillsReloaded { server, skills }
        }
        SettingsRequest::ReloadConsoleSources { server, active_dir } => {
            let sources = load_console_sources(srv, active_dir.as_deref());
            SettingsReply::ConsoleSourcesReloaded { server, sources }
        }
        SettingsRequest::CheckPluginUpdates { server, attempt } => {
            let result = match srv.check_plugin_updates(None) {
                Ok(PluginEndpoint::Supported(list)) => PluginCheckResult::Supported(list),
                Ok(PluginEndpoint::Unsupported) => PluginCheckResult::Unsupported,
                Err(e) => PluginCheckResult::Failed(format_wire_error(&e)),
            };
            SettingsReply::PluginCheckSettled { server, attempt, result }
        }
        SettingsRequest::UpdatePlugin { server, id, target } => {
            let result = match srv.update_plugins(&[target]) {
                Ok(PluginEndpoint::Supported(())) => PluginUpdateResult::Succeeded,
                Ok(PluginEndpoint::Unsupported) => PluginUpdateResult::Unsupported,
                Err(e) => PluginUpdateResult::Failed(format_wire_error(&e)),
            };
            SettingsReply::PluginUpdateSettled { server, id, result }
        }
        SettingsRequest::LoadProjectExtensions { dir } => {
            let plugins = match (load_plugins(srv, None), load_plugins(srv, Some(&dir))) {
                (Ok(global), Ok(project)) => Ok((plugin_labels(&global), plugin_labels(&project))),
                _ => Err(()),
            };
            let global_skills = srv.list_skills(None).ok().map(|ep| ep.into_option().unwrap_or_default());
            let project_skills = srv.list_skills(Some(&dir)).ok().map(|ep| ep.into_option().unwrap_or_default());
            let skills = global_skills.zip(project_skills);
            let global_cfg = srv.config_entries(None);
            let project_cfg = srv.config_entries(Some(&dir));
            let lsps = match &project_cfg {
                Ok(PluginEndpoint::Supported(entries)) => Ok(configured_lsps_entries(entries)),
                Ok(PluginEndpoint::Unsupported) => Ok((false, Vec::new())),
                Err(_) => Err(()),
            };
            let sources = match (global_cfg, project_cfg) {
                (Ok(PluginEndpoint::Unsupported), _) => Ok(Vec::new()),
                (Ok(PluginEndpoint::Supported(entries)), Ok(PluginEndpoint::Supported(active))) => Ok(console_mcps(&entries, Some(&active))),
                (Ok(PluginEndpoint::Supported(entries)), Ok(PluginEndpoint::Unsupported)) => Ok(console_mcps(&entries, None)),
                _ => Err(()),
            };
            SettingsReply::ProjectExtensionsLoaded { dir, plugins, skills, sources, lsps }
        }
        SettingsRequest::CheckProjectLocation { dir } => {
            let path = format!("/api/location{}", location_query(Some(&dir)));
            let missing = match srv.get(&path) {
                Ok(_) => Some(false),
                Err(WireError { kind: WireKind::Status { tag: Some(tag), .. }, .. }) if tag == "LocationNotFoundError" => Some(true),
                // Generic errors / timeouts / offline do not prove the folder is missing (`location-machine.ts:86`).
                Err(_) => None,
            };
            SettingsReply::ProjectLocationChecked { dir, missing }
        }
        SettingsRequest::PatchProject { id, field, body } => {
            let path = format!("/api/project/{}", percent_encode(&id));
            let updated = srv
                .write("PATCH", &path, Some(body))
                .ok()
                .and_then(|v| parse_project(v.get("data").unwrap_or(&v)));
            SettingsReply::ProjectPatched { field, updated }
        }
        SettingsRequest::RenameProject { id, name } => {
            let path = format!("/api/project/{}", percent_encode(&id));
            let result = srv
                .write("PATCH", &path, Some(json!({ "name": name })))
                .map(|v| parse_project(v.get("data").unwrap_or(&v)));
            SettingsReply::ProjectRenamed { result }
        }
        SettingsRequest::DisconnectProvider { id, name } => {
            let result = srv.integration(&id, None).and_then(|i| {
                let creds: Vec<String> = i.connections.iter().filter(|c| c.credential).map(|c| c.id.clone()).collect();
                if creds.is_empty() {
                    return Err(t!("provider.disconnect.toast.noCredentials.description", provider = name));
                }
                creds.iter().try_for_each(|c| srv.credential_remove(c))
            });
            SettingsReply::ProviderDisconnected { name, result }
        }
        SettingsRequest::ActivateAccount { credential, provider, label } => {
            let result = srv.credential_activate(&credential);
            SettingsReply::AccountWriteSettled {
                title: t!("settings.providers.account.switched.title", provider = provider),
                description: t!("settings.providers.account.switched.description", account = label),
                result,
            }
        }
        SettingsRequest::RemoveAccount { credential, provider, label } => {
            let result = srv.credential_remove(&credential);
            SettingsReply::AccountWriteSettled {
                title: t!("settings.providers.account.removed.title", account = label),
                description: t!("settings.providers.account.removed.description", provider = provider),
                result,
            }
        }
        SettingsRequest::RenameAccount { credential, label } => {
            let result = srv.credential_rename(&credential, &label);
            SettingsReply::AccountWriteSettled {
                title: String::new(),
                description: String::new(),
                result,
            }
        }
        SettingsRequest::LoadServerShells { key } => {
            let shells = srv.get("/api/config/shell").ok();
            let config = srv.get("/api/config").ok();
            let list = shells.as_ref().and_then(Value::as_array).map(|arr| {
                arr.iter()
                    .filter_map(|v| {
                        Some((
                            v["path"].as_str()?.to_owned(),
                            v["name"].as_str()?.to_owned(),
                            v["acceptable"].as_bool().unwrap_or(true),
                        ))
                    })
                    .collect()
            });
            let current = config.as_ref().and_then(Value::as_array).map(|entries| global_shell(entries));
            SettingsReply::ServerShellsLoaded { key, list, current }
        }
        SettingsRequest::SetServerShell { key, value, previous } => {
            let body = json!({ "shell": if value.is_empty() { Value::Null } else { Value::String(value) } });
            let failed = srv.write("PATCH", "/api/experimental/config", Some(body)).err();
            SettingsReply::ServerShellSaved { key, previous, failed }
        }
        SettingsRequest::InspectWorktree { attempt, directory, active, linked } => {
            let inspection = worktrees::inspect_on_server(srv, &directory, active.as_deref(), linked);
            SettingsReply::WorktreeInspected { attempt, inspection }
        }
        SettingsRequest::DeleteWorktree { attempt, target, force, active, linked } => {
            // A bulk delete checks each worktree first; a single one was checked in its
            // confirmation and deletes with force.
            let blocked = if force {
                None
            } else {
                match worktrees::inspect_on_server(srv, &target.directory, active.as_deref(), linked) {
                    Some(i) if i.active || i.linked || i.dirty => Some(Skip::Blocked(i)),
                    Some(_) => None,
                    None => Some(Skip::Failed(None)),
                }
            };
            let result = match blocked {
                Some(reason) => Err(reason),
                None => {
                    let body = json!({ "projectID": target.project, "directory": target.directory, "force": force });
                    srv.write("DELETE", "/api/worktree", Some(body))
                        .map(|_| ())
                        .map_err(|e| Skip::Failed(Some(e.to_string())))
                }
            };
            SettingsReply::WorktreeDeleted { attempt, target, result }
        }
    }
}

/// Applies a completed [`SettingsReply`] back onto `host` on the UI thread.
pub fn apply_reply(host: &mut dyn SettingsHost, reply: SettingsReply, tx: &mut Tx) {
    match reply {
        SettingsReply::ExtensionsLoaded { server, plugins, skills, sources } => {
            tx.instant();
            if host.settings_pages().extensions.server != server {
                return;
            }
            let flags = plugins.as_ref().ok().map(|list| plugin_update::package_flags(list));
            {
                let ext = &mut host.settings_pages_mut().extensions;
                ext.plugins = Some(plugins);
                if skills.is_some() {
                    ext.skills = skills;
                }
                ext.sources.apply(sources);
            }
            if let Some(outdated) = flags {
                extensions::dispatch_update(host, plugin_update::Event::ListLoaded { outdated }, tx);
            }
        }
        SettingsReply::PluginsReloaded { server, plugins } => {
            tx.instant();
            if host.settings_pages().extensions.server != server {
                return;
            }
            let flags = plugins.as_ref().ok().map(|list| plugin_update::package_flags(list));
            host.settings_pages_mut().extensions.plugins = Some(plugins);
            if let Some(outdated) = flags {
                extensions::dispatch_update(host, plugin_update::Event::ListLoaded { outdated }, tx);
            }
        }
        SettingsReply::SkillsReloaded { server, skills } => {
            tx.instant();
            if host.settings_pages().extensions.server != server {
                return;
            }
            if skills.is_some() {
                host.settings_pages_mut().extensions.skills = skills;
            }
        }
        SettingsReply::ConsoleSourcesReloaded { server, sources } => {
            tx.instant();
            if host.settings_pages().extensions.server != server {
                return;
            }
            host.settings_pages_mut().extensions.sources.apply(sources);
        }
        SettingsReply::PluginCheckSettled { server, attempt, result } => {
            tx.instant();
            if host.settings_pages().extensions.server != server {
                return;
            }
            let event = match result {
                PluginCheckResult::Supported(list) => plugin_update::Event::CheckSucceeded {
                    attempt,
                    outdated: plugin_update::package_flags(&list),
                },
                PluginCheckResult::Unsupported => plugin_update::Event::CheckUnsupported { attempt },
                PluginCheckResult::Failed(message) => plugin_update::Event::CheckFailed { attempt, message },
            };
            extensions::dispatch_update(host, event, tx);
        }
        SettingsReply::PluginUpdateSettled { server, id, result } => {
            tx.instant();
            if host.settings_pages().extensions.server != server {
                return;
            }
            let event = match result {
                PluginUpdateResult::Succeeded => plugin_update::Event::UpdateSucceeded { id },
                PluginUpdateResult::Unsupported => plugin_update::Event::UpdateUnsupported { id },
                PluginUpdateResult::Failed(message) => plugin_update::Event::UpdateFailed { id, message },
            };
            extensions::dispatch_update(host, event, tx);
        }
        SettingsReply::ProjectExtensionsLoaded { dir, plugins, skills, sources, lsps } => {
            tx.instant();
            let st = &mut host.settings_pages_mut().project.extensions;
            if st.dir != dir {
                return;
            }
            st.apply_loaded(plugins, skills, sources, lsps);
        }
        SettingsReply::ProjectLocationChecked { dir, missing } => {
            if host.settings_pages().project.dir != dir {
                return;
            }
            if let Some(missing) = missing {
                tx.instant();
                host.settings_pages_mut().project.missing = missing;
            }
        }
        SettingsReply::ProjectPatched { field, updated } => {
            tx.instant();
            match updated {
                Some(project) => host.apply_project(project),
                None => {
                    host.settings_pages_mut().project.failed.push(field);
                    host.toast_say(t!("common.requestFailed").into(), t!("rune.settings.project.saveFailed").into());
                }
            }
        }
        SettingsReply::ProjectRenamed { result } => {
            tx.instant();
            match result {
                Ok(Some(updated)) => host.apply_project(updated),
                Ok(None) => {}
                Err(e) => host.toast_failed_wire(t!("rune.settings.project.saveFailed").into(), &e),
            }
        }
        SettingsReply::ProviderDisconnected { name, result } => {
            match result {
                Ok(()) => host.toast_succeed(
                    t!("provider.disconnect.toast.disconnected.title", provider = name),
                    t!("provider.disconnect.toast.disconnected.description", provider = name),
                ),
                Err(e) => host.toast_fail(t!("common.requestFailed").into(), e),
            }
            host.sync_catalogs(None);
        }
        SettingsReply::AccountWriteSettled { title, description, result } => {
            match result {
                Ok(()) if !title.is_empty() => host.toast_succeed(title, description),
                Ok(()) => {}
                Err(e) => host.toast_fail(t!("common.requestFailed").into(), e),
            }
            host.sync_catalogs(None);
        }
        SettingsReply::ServerShellsLoaded { key, list, current } => {
            tx.instant();
            let st = host.settings_pages_mut().servers.shells.entry(key).or_default();
            if let Some(list) = list {
                st.list = list;
            }
            if let Some(current) = current {
                st.current = current;
            }
        }
        SettingsReply::ServerShellSaved { key, previous, failed } => {
            if let Some(e) = failed {
                tx.instant();
                host.settings_pages_mut().servers.shells.entry(key).or_default().current = previous;
                host.toast_failed_wire(t!("common.requestFailed").into(), &e);
            }
        }
        SettingsReply::WorktreeInspected { attempt, inspection } => {
            tx.instant();
            worktrees::dispatch(
                host,
                match inspection {
                    Some(inspection) => worktrees::Event::InspectSucceeded { attempt, inspection },
                    None => worktrees::Event::InspectFailed { attempt },
                },
            );
        }
        SettingsReply::WorktreeDeleted { attempt, target, result } => {
            tx.instant();
            let key = target.key.clone();
            worktrees::dispatch(
                host,
                match result {
                    Ok(()) => worktrees::Event::DeleteSucceeded { attempt, target },
                    Err(reason) => worktrees::Event::DeleteSkipped { attempt, reason },
                },
            );
            // The deleted row closes up on its own curve (`.settings-workspaces-row-motion`).
            if host.settings_pages().worktrees.delete.row(&key).1 {
                tx.animated();
                tx.motion_curve(worktrees::COLLAPSE);
            }
        }
    }
}
