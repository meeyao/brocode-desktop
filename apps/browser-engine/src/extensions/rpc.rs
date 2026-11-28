//! Extension RPC method dispatcher, Rust-side permission enforcement, tab/window queries,
//! action icon resolution (`path` + `ImageData` -> PNG), and navigation URL policy.

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::path::Path;

use base64::Engine as _;
use serde_json::{Value, json};

use super::manifest::{
    ExtensionManifest, closest_icon, extension_id_from_url, extension_label, extension_page_url,
    glob_match, is_extension_scheme, is_private_url, matches_host, read_icon_data_url,
};
use super::png::rgba_to_png_data_url;
use super::{ExtEffect, ExtTab, ExtensionHost, SessionKind, WINDOW_ID, WINDOW_ID_CURRENT};

const MAX_NAV_URL_BYTES: usize = 8192;

fn format_notification_body(opts: Option<&serde_json::Map<String, Value>>) -> String {
    let Some(obj) = opts else { return String::new() };
    let clean_field = |k: &str| {
        obj.get(k)
            .and_then(Value::as_str)
            // Control characters and the bidi controls that could reorder the text to pass for
            // the app's or another extension's words (`U+061C`, `U+200E`/`F`, `U+202A`-`U+202E`, `U+2066`-`U+2069`).
            .map(|s| {
                s.chars()
                    .filter(|c| !c.is_control() || *c == ' ')
                    .filter(|c| !matches!(c, '\u{061C}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'))
                    .take(256)
                    .collect::<String>()
            })
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
    };
    let parts: Vec<String> = ["title", "message", "contextMessage"]
        .into_iter()
        .filter_map(clean_field)
        .collect();
    parts.join("\n").chars().take(512).collect()
}

/// Exact `privacy.*` defaults from official `extension-api/stubs.ts:6-23` (`edacc868`).
fn privacy_setting_default(path: &str) -> Option<Value> {
    Some(match path {
        "privacy.network.networkPredictionEnabled" => Value::Bool(true),
        "privacy.network.webRTCIPHandlingPolicy" => Value::String("default".into()),
        "privacy.services.alternateErrorPagesEnabled" => Value::Bool(false),
        "privacy.services.autofillAddressEnabled" => Value::Bool(true),
        "privacy.services.autofillCreditCardEnabled" => Value::Bool(true),
        "privacy.services.autofillEnabled" => Value::Bool(true),
        "privacy.services.passwordSavingEnabled" => Value::Bool(true),
        "privacy.services.safeBrowsingEnabled" => Value::Bool(true),
        "privacy.services.searchSuggestEnabled" => Value::Bool(true),
        "privacy.services.spellingServiceEnabled" => Value::Bool(false),
        "privacy.services.translationServiceEnabled" => Value::Bool(false),
        "privacy.websites.hyperlinkAuditingEnabled" => Value::Bool(true),
        "privacy.websites.referrersEnabled" => Value::Bool(true),
        "privacy.websites.thirdPartyCookiesAllowed" => Value::Bool(true),
        "privacy.websites.doNotTrackEnabled" => Value::Bool(false),
        "privacy.websites.protectedContentEnabled" => Value::Bool(true),
        _ => return None,
    })
}

fn parse_privacy_call(name: &str) -> Option<(&str, &str)> {
    let (path, op) = name.rsplit_once('.')?;
    if !matches!(op, "get" | "set" | "clear") || privacy_setting_default(path).is_none() {
        return None;
    }
    Some((path, op))
}

/// Exact finite allow-list of supported shimmed event names (prevents unbounded `subscribers` OOM;
/// `review-rd274.md` §4).
pub(super) fn is_supported_event(name: &str) -> bool {
    matches!(
        name,
        "tabs.onCreated"
            | "tabs.onRemoved"
            | "tabs.onUpdated"
            | "tabs.onActivated"
            | "windows.onCreated"
            | "windows.onRemoved"
            | "windows.onFocusChanged"
            | "windows.onBoundsChanged"
            | "action.onClicked"
            | "webNavigation.onBeforeNavigate"
            | "webNavigation.onCommitted"
            | "webNavigation.onDOMContentLoaded"
            | "webNavigation.onCompleted"
            | "webNavigation.onErrorOccurred"
            | "webNavigation.onReferenceFragmentUpdated"
            | "webNavigation.onHistoryStateUpdated"
            | "contextMenus.onClicked"
            | "notifications.onClicked"
            | "notifications.onClosed"
            | "commands.onCommand"
    )
}

pub(super) fn is_event_allowed(manifest: &ExtensionManifest, name: &str) -> bool {
    if !is_supported_event(name) {
        return false;
    }
    let required: Option<&str> = if name.starts_with("webNavigation.") {
        Some("webNavigation")
    } else if name.starts_with("contextMenus.") {
        Some("contextMenus")
    } else if name.starts_with("notifications.") {
        Some("notifications")
    } else {
        None
    };
    required.is_none_or(|perm| manifest.has_permission(perm))
}

/// Exact method allow-list enforced in Rust before every shimmed RPC call (`review-rd274.md` §4).
pub(super) fn is_api_allowed(manifest: &ExtensionManifest, name: &str) -> bool {
    let required: Option<&str> = match name {
        "webNavigation.getFrame" | "webNavigation.getAllFrames" => Some("webNavigation"),
        "contextMenus.create"
        | "contextMenus.update"
        | "contextMenus.remove"
        | "contextMenus.removeAll" => Some("contextMenus"),
        "notifications.create"
        | "notifications.update"
        | "notifications.clear"
        | "notifications.getAll"
        | "notifications.getPermissionLevel" => Some("notifications"),
        "cookies.getAllCookieStores" | "cookies.getPartitionKey" => Some("cookies"),
        "sidePanel.setOptions"
        | "sidePanel.getOptions"
        | "sidePanel.setPanelBehavior"
        | "sidePanel.getPanelBehavior"
        | "sidePanel.open"
        | "sidePanel.close" => Some("sidePanel"),
        "fontSettings.getFontList"
        | "fontSettings.getFont"
        | "fontSettings.setFont"
        | "fontSettings.clearFont" => Some("fontSettings"),
        "history.search"
        | "history.getVisits"
        | "history.addUrl"
        | "history.deleteUrl"
        | "history.deleteRange"
        | "history.deleteAll" => Some("history"),
        "bookmarks.get"
        | "bookmarks.getChildren"
        | "bookmarks.getRecent"
        | "bookmarks.getTree"
        | "bookmarks.getSubTree"
        | "bookmarks.search"
        | "bookmarks.create"
        | "bookmarks.move"
        | "bookmarks.update"
        | "bookmarks.remove"
        | "bookmarks.removeTree" => Some("bookmarks"),
        "sessions.getRecentlyClosed"
        | "sessions.getDevices"
        | "sessions.restore" => Some("sessions"),
        "search.query" => Some("search"),
        "downloads.download"
        | "downloads.search"
        | "downloads.pause"
        | "downloads.resume"
        | "downloads.cancel"
        | "downloads.getFileIcon"
        | "downloads.open"
        | "downloads.show"
        | "downloads.showDefaultFolder"
        | "downloads.erase"
        | "downloads.removeFile"
        | "downloads.acceptDanger"
        | "downloads.setUiOptions"
        | "downloads.setShelfEnabled" => Some("downloads"),
        "management.getSelf"
        | "management.getAll"
        | "management.uninstallSelf"
        | "management.getPermissionWarningsByManifest" => None,
        "management.get"
        | "management.setEnabled"
        | "management.uninstall"
        | "management.launchApp"
        | "management.createAppShortcut"
        | "management.setLaunchType"
        | "management.getPermissionWarningsById"
        | "management.generateAppForLink" => None,
        "identity.getAuthToken"
        | "identity.getProfileUserInfo"
        | "identity.getAccounts"
        | "identity.removeCachedAuthToken"
        | "identity.clearAllCachedAuthTokens"
        | "identity.launchWebAuthFlow"
        | "identity.getRedirectURL" => Some("identity"),
        "tabs.query"
        | "tabs.get"
        | "tabs.getCurrent"
        | "tabs.create"
        | "tabs.update"
        | "tabs.remove"
        | "tabs.reload"
        | "tabs.highlight"
        | "windows.get"
        | "windows.getCurrent"
        | "windows.getLastFocused"
        | "windows.getAll"
        | "windows.create"
        | "windows.update"
        | "windows.remove"
        | "action.setTitle"
        | "action.getTitle"
        | "action.setIcon"
        | "action.setPopup"
        | "action.getPopup"
        | "action.setBadgeText"
        | "action.getBadgeText"
        | "action.setBadgeBackgroundColor"
        | "action.getBadgeBackgroundColor"
        | "action.setBadgeTextColor"
        | "action.getBadgeTextColor"
        | "action.enable"
        | "action.disable"
        | "action.isEnabled"
        | "action.getUserSettings"
        | "action.openPopup"
        | "permissions.getAll"
        | "permissions.contains"
        | "permissions.request"
        | "permissions.remove"
        | "commands.getAll"
        | "webRequest.handlerBehaviorChanged"
        | "extension.isAllowedFileSchemeAccess"
        | "extension.isAllowedIncognitoAccess"
        | "extension.setUpdateUrlData"
        | "runtime.openOptionsPage" => None,
        other if parse_privacy_call(other).is_some() => Some("privacy"),
        _ => return false,
    };
    required.is_none_or(|perm| manifest.has_permission(perm))
}

impl ExtensionHost {
    pub(super) fn dispatch_rpc(
        &mut self,
        sid: &str,
        ext_id: &str,
        req_id: i64,
        name: &str,
        args: &[Value],
    ) -> Vec<ExtEffect> {
        let mut effects = Vec::new();
        let Some(ext) = self.extensions.get(ext_id).cloned() else {
            self.reply_err(sid, req_id, "Extension is not loaded.");
            return effects;
        };
        if !is_api_allowed(&ext.manifest, name) {
            self.reply_err(sid, req_id, &format!("chrome.{name} is not available in this browser."));
            return effects;
        }

        let result: Result<Value, String> = match name {
            "tabs.query" => {
                let query = args.first().and_then(Value::as_object);
                Ok(Value::Array(self.answer_tabs_query(ext_id, &ext.manifest, query)))
            }
            "tabs.get" => {
                let wanted = args.first().and_then(Value::as_i64).map(|n| n as i32);
                match wanted.and_then(|id| self.find_visible_tab_by_ext_cid(ext_id, id)) {
                    Some(t) => Ok(self.tab_json(ext_id, t, &ext.manifest, false)),
                    None => Err(format!("No tab with id: {}.", wanted.unwrap_or(-1))),
                }
            }
            "tabs.getCurrent" => {
                // Official `extension-api/tabs.ts:345-349`: returns the calling extension page's
                // own pane tab, or `undefined` (`null` over JSON) from service workers and popups.
                let found = self
                    .sessions
                    .get(sid)
                    .filter(|s| s.kind == SessionKind::ExtensionPage)
                    .and_then(|s| {
                        let url = s.url.clone();
                        let cid = s.chromium_tab_id;
                        cid.and_then(|id| self.find_visible_tab_by_ext_cid(ext_id, id))
                            .filter(|t| extension_id_from_url(&t.url) == Some(ext_id))
                            .or_else(|| {
                                self.visible_tabs().find(|t| {
                                    t.url == url && extension_id_from_url(&t.url) == Some(ext_id)
                                })
                            })
                    })
                    .cloned();
                match found {
                    Some(t) => Ok(self.tab_json(ext_id, &t, &ext.manifest, false)),
                    None => Ok(Value::Null),
                }
            }
            "tabs.create" => {
                if !self.allow_open_tab(ext_id) {
                    Err("Too many tabs opened by this extension.".into())
                } else {
                    let opts = args.first().and_then(Value::as_object);
                    let active = opts.and_then(|o| o.get("active")).and_then(Value::as_bool).unwrap_or(true);
                    let raw_url = opts.and_then(|o| o.get("url")).and_then(Value::as_str).unwrap_or("about:blank");
                    match resolve_extension_nav_url(ext_id, raw_url) {
                        Ok(url) => {
                            effects.push(ExtEffect::OpenTab {
                                ext_id: ext_id.to_owned(),
                                url,
                                active,
                                reply_to: Some((sid.to_owned(), req_id, ext_id.to_owned())),
                            });
                            return effects;
                        }
                        Err(e) => Err(e),
                    }
                }
            }
            "tabs.update" => self.rpc_tabs_update(sid, ext_id, req_id, &ext.manifest, args, &mut effects),
            "tabs.remove" => {
                let ids: Vec<i32> = match args.first() {
                    Some(Value::Number(n)) => n.as_i64().map(|v| vec![v as i32]).unwrap_or_default(),
                    Some(Value::Array(arr)) => arr.iter().take(16).filter_map(|v| v.as_i64().map(|n| n as i32)).collect(),
                    _ => Vec::new(),
                };
                for cid in ids {
                    if let Some(t) = self.find_visible_tab_by_ext_cid(ext_id, cid) {
                        effects.push(ExtEffect::CloseTab { engine_tab_id: t.engine_tab_id });
                    }
                }
                Ok(Value::Null)
            }
            "tabs.reload" => {
                let (tid_opt, opts) = if args.first().is_some_and(Value::is_number) {
                    (args[0].as_i64().map(|n| n as i32), args.get(1).and_then(Value::as_object))
                } else {
                    (None, args.first().and_then(Value::as_object))
                };
                let ignore_cache = opts.and_then(|o| o.get("bypassCache")).and_then(Value::as_bool).unwrap_or(false);
                let tab = match tid_opt {
                    Some(cid) => self.find_tab_pos_by_ext_cid(ext_id, cid).and_then(|i| self.tabs.get(i)),
                    None => self.tabs.iter().find(|t| t.active),
                };
                if let Some(t) = tab {
                    if t.agent_driven {
                        Err("Extensions cannot reload agent-driven tabs.".into())
                    } else {
                        effects.push(ExtEffect::ReloadTab { engine_tab_id: t.engine_tab_id, ignore_cache });
                        Ok(Value::Null)
                    }
                } else {
                    Err("No tab to reload.".into())
                }
            }
            "tabs.highlight" => {
                let idx_opt = args
                    .first()
                    .and_then(|v| v.get("tabs"))
                    .and_then(|t| t.as_u64().or_else(|| t.as_array()?.first()?.as_u64()))
                    .map(|n| n as usize);
                if let Some(idx) = idx_opt
                    && let Some(t) = self.visible_tabs().nth(idx)
                {
                    effects.push(ExtEffect::FocusTab { engine_tab_id: t.engine_tab_id });
                }
                Ok(self.window_json(ext_id, &ext.manifest, true))
            }
            "windows.get" => {
                let id = args.first().and_then(Value::as_i64).unwrap_or(-1) as i32;
                let populate = args.get(1).and_then(|v| v.get("populate")).and_then(Value::as_bool).unwrap_or(false);
                if id == WINDOW_ID || id == WINDOW_ID_CURRENT {
                    Ok(self.window_json(ext_id, &ext.manifest, populate))
                } else {
                    Err(format!("No window with id: {id}."))
                }
            }
            "windows.getCurrent" | "windows.getLastFocused" => {
                let populate = args.first().and_then(|v| v.get("populate")).and_then(Value::as_bool).unwrap_or(false);
                Ok(self.window_json(ext_id, &ext.manifest, populate))
            }
            "windows.getAll" => {
                let populate = args.first().and_then(|v| v.get("populate")).and_then(Value::as_bool).unwrap_or(false);
                Ok(Value::Array(vec![self.window_json(ext_id, &ext.manifest, populate)]))
            }
            "windows.create" => {
                if !self.allow_open_tab(ext_id) {
                    Err("Too many tabs opened by this extension.".into())
                } else {
                    let url = args
                        .first()
                        .and_then(|v| v.get("url"))
                        .and_then(|u| u.as_str().or_else(|| u.as_array()?.first()?.as_str()))
                        .unwrap_or("about:blank");
                    match resolve_extension_nav_url(ext_id, url) {
                        Ok(u) => {
                            effects.push(ExtEffect::OpenTab {
                                ext_id: ext_id.to_owned(),
                                url: u,
                                active: true,
                                reply_to: None,
                            });
                            Ok(self.window_json(ext_id, &ext.manifest, true))
                        }
                        Err(e) => Err(e),
                    }
                }
            }
            "windows.update" => {
                if let Some(state) = args.get(1).and_then(|v| v.get("state")).and_then(Value::as_str)
                    && state != "normal"
                {
                    Err("Extensions cannot change the size of app windows.".into())
                } else {
                    Ok(self.window_json(ext_id, &ext.manifest, false))
                }
            }
            "windows.remove" => Err("Extensions cannot close app windows.".into()),

            n if n.starts_with("action.") => self.rpc_action(ext_id, &ext.dir, &ext.manifest, n, args, &mut effects),

            "runtime.openOptionsPage" => {
                if !self.allow_open_tab(ext_id) {
                    Err("Too many tabs opened by this extension.".into())
                } else if let Some(page) = ext.manifest.options_page.as_deref()
                    && let Some(url) = extension_page_url(ext_id, page)
                {
                    effects.push(ExtEffect::OpenTab {
                        ext_id: ext_id.to_owned(),
                        url,
                        active: true,
                        reply_to: None,
                    });
                    Ok(Value::Null)
                } else {
                    Err("Could not create an options page.".into())
                }
            }

            "webNavigation.getFrame" => {
                let tid = args.first().and_then(|v| v.get("tabId")).and_then(Value::as_i64).map(|n| n as i32);
                let fid = args.first().and_then(|v| v.get("frameId")).and_then(Value::as_i64).unwrap_or(0) as i32;
                match tid.and_then(|id| self.find_visible_tab_by_ext_cid(ext_id, id)) {
                    Some(t) => Ok(self.web_nav.get_frame(t.engine_tab_id, &t.url, fid).unwrap_or(Value::Null)),
                    None => Err(format!("No tab with id: {}.", tid.unwrap_or(-1))),
                }
            }
            "webNavigation.getAllFrames" => {
                let tid = args.first().and_then(|v| v.get("tabId")).and_then(Value::as_i64).map(|n| n as i32);
                match tid.and_then(|id| self.find_visible_tab_by_ext_cid(ext_id, id)) {
                    Some(t) => Ok(Value::Array(self.web_nav.all_frames(t.engine_tab_id, &t.url))),
                    None => Err(format!("No tab with id: {}.", tid.unwrap_or(-1))),
                }
            }

            "contextMenus.create" => {
                if let Some(props) = args.first() {
                    self.context_menus
                        .lock()
                        .unwrap()
                        .create(ext_id, &ext.manifest.name, props)
                        .map(|()| Value::Null)
                } else {
                    Err("Missing context menu properties.".into())
                }
            }
            "contextMenus.update" => {
                if let (Some(id_val), Some(props)) = (args.first(), args.get(1)) {
                    self.context_menus
                        .lock()
                        .unwrap()
                        .update(ext_id, id_val, props)
                        .map(|()| Value::Null)
                } else {
                    Err("Missing context menu item id or properties.".into())
                }
            }
            "contextMenus.remove" => {
                if let Some(id_val) = args.first() {
                    self.context_menus.lock().unwrap().remove(ext_id, id_val);
                }
                Ok(Value::Null)
            }
            "contextMenus.removeAll" => {
                self.context_menus.lock().unwrap().clear_extension(ext_id);
                Ok(Value::Null)
            }

            "notifications.create" => {
                // Official `extension-api/notifications.ts:68-73`: accepts `(id, options)` or `(options)`.
                let (raw_id, opts) = if args.len() >= 2 {
                    (args[0].as_str(), args.get(1).and_then(Value::as_object))
                } else if args.first().is_some_and(Value::is_object) {
                    (None, args.first().and_then(Value::as_object))
                } else {
                    (args.first().and_then(Value::as_str), None)
                };
                let id = raw_id
                    .map(extension_label)
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| format!("n-{}", self.alloc_id()));
                if self.notifications_enabled && self.allow_notification(ext_id) {
                    let body = format_notification_body(opts);
                    let ext_name = {
                        let clean = extension_label(&ext.manifest.name);
                        if clean.is_empty() { ext_id.to_owned() } else { clean }
                    };
                    let evicted = {
                        let list = self.notifications.entry(ext_id.to_owned()).or_default();
                        list.retain(|existing| existing != &id);
                        let oldest = (list.len() >= 32).then(|| list.remove(0));
                        list.push(id.clone());
                        oldest
                    };
                    if let Some(oldest) = evicted {
                        effects.push(ExtEffect::NotificationClear {
                            ext_id: ext_id.to_owned(),
                            notif_id: oldest.clone(),
                        });
                        self.emit_event("notifications.onClosed", vec![json!(oldest), json!(false)], Some(ext_id));
                    }
                    effects.push(ExtEffect::NotificationShow {
                        ext_id: ext_id.to_owned(),
                        ext_name,
                        notif_id: id.clone(),
                        body,
                    });
                }
                Ok(Value::String(id))
            }
            "notifications.update" => {
                let id = args.first().and_then(Value::as_str).map(extension_label).unwrap_or_default();
                let opts = args.get(1).and_then(Value::as_object);
                let exists = !id.is_empty()
                    && self.notifications.get(ext_id).is_some_and(|list| list.iter().any(|x| x == &id));
                if exists && self.notifications_enabled && self.allow_notification(ext_id) {
                    let body = format_notification_body(opts);
                    let ext_name = {
                        let clean = extension_label(&ext.manifest.name);
                        if clean.is_empty() { ext_id.to_owned() } else { clean }
                    };
                    effects.push(ExtEffect::NotificationShow {
                        ext_id: ext_id.to_owned(),
                        ext_name,
                        notif_id: id,
                        body,
                    });
                }
                Ok(Value::Bool(exists))
            }
            "notifications.clear" => {
                let id = args.first().and_then(Value::as_str).map(extension_label).unwrap_or_default();
                let removed = !id.is_empty()
                    && self.notifications.get_mut(ext_id).is_some_and(|list| {
                        let before = list.len();
                        list.retain(|x| x != &id);
                        list.len() != before
                    });
                if removed {
                    effects.push(ExtEffect::NotificationClear { ext_id: ext_id.to_owned(), notif_id: id.clone() });
                    self.emit_event("notifications.onClosed", vec![json!(id), json!(false)], Some(ext_id));
                }
                Ok(Value::Bool(removed))
            }
            "notifications.getAll" => {
                let mut map = serde_json::Map::new();
                if let Some(list) = self.notifications.get(ext_id) {
                    for id in list {
                        map.insert(id.clone(), Value::Bool(true));
                    }
                }
                Ok(Value::Object(map))
            }
            "notifications.getPermissionLevel" => {
                Ok(json!(if self.notifications_enabled { "granted" } else { "denied" }))
            }

            "permissions.getAll" => {
                let perms: Vec<&String> = ext
                    .manifest
                    .permissions
                    .iter()
                    .filter(|p| {
                        !p.contains("://")
                            && p.as_str() != "<all_urls>"
                            && super::manifest::is_supported_permission(p)
                    })
                    .collect();
                Ok(json!({ "permissions": perms, "origins": ext.manifest.hosts }))
            }
            "permissions.contains" | "permissions.request" => {
                let req = args.first().and_then(Value::as_object);
                // Deny by default: never grant optional permissions or any stripped/unsupported
                // permission outside `ALLOWED_CHROMIUM_PERMS` U `SHIMMED_SUPPORTED_PERMS`.
                let ok_perms = req
                    .and_then(|o| o.get("permissions"))
                    .and_then(Value::as_array)
                    .is_none_or(|arr| {
                        arr.iter().all(|v| {
                            v.as_str().is_some_and(|p| {
                                super::manifest::is_supported_permission(p) && ext.manifest.has_permission(p)
                            })
                        })
                    });
                let ok_origins = req
                    .and_then(|o| o.get("origins"))
                    .and_then(Value::as_array)
                    .is_none_or(|arr| {
                        arr.iter().all(|v| {
                            v.as_str().is_some_and(|o| {
                                !o.trim().to_ascii_lowercase().starts_with("file:")
                                    && ext.manifest.hosts.iter().any(|h| {
                                        h == o || h == "<all_urls>" || h == "*://*/*"
                                    })
                            })
                        })
                    });
                Ok(Value::Bool(ok_perms && ok_origins))
            }
            "permissions.remove" => Ok(Value::Bool(false)),

            "cookies.getAllCookieStores" => {
                let tab_ids: Vec<i32> = self
                    .visible_tabs()
                    .filter_map(|t| self.ext_chromium_tab_id(ext_id, t.engine_tab_id))
                    .collect();
                Ok(json!([{ "id": "0", "tabIds": tab_ids }]))
            }
            "cookies.getPartitionKey" => Ok(json!({ "partitionKey": {} })),

            "fontSettings.getFontList" => Ok(json!([])),
            "fontSettings.getFont" => Ok(json!({ "fontId": "", "levelOfControl": "not_controllable" })),
            "fontSettings.setFont" | "fontSettings.clearFont" => Ok(Value::Null),
            "sidePanel.setOptions" | "sidePanel.setPanelBehavior" | "sidePanel.close" => Ok(Value::Null),
            "sidePanel.getOptions" => Ok(json!({ "enabled": false })),
            "sidePanel.getPanelBehavior" => Ok(json!({ "openPanelOnActionClick": false })),
            "sidePanel.open" => Err("This browser has no side panel.".into()),
            // Startup stubs matching official `extension-api/stubs.ts:88-145` (`edacc868`).
            "history.search" | "history.getVisits" => Ok(json!([])),
            "history.addUrl" | "history.deleteUrl" | "history.deleteRange" | "history.deleteAll" => Ok(Value::Null),
            "bookmarks.get"
            | "bookmarks.getChildren"
            | "bookmarks.getRecent"
            | "bookmarks.getSubTree"
            | "bookmarks.search" => Ok(json!([])),
            "bookmarks.getTree" => Ok(json!([{ "id": "0", "title": "", "children": [] }])),
            "bookmarks.create"
            | "bookmarks.move"
            | "bookmarks.update"
            | "bookmarks.remove"
            | "bookmarks.removeTree" => Err("This browser has no bookmarks.".into()),
            "sessions.getRecentlyClosed" | "sessions.getDevices" => Ok(json!([])),
            "sessions.restore" => Err("This browser keeps no closed sessions.".into()),
            "search.query" => Err("This browser has no search engine for extensions.".into()),
            "downloads.download" => Err("Extensions cannot download files in this browser.".into()),
            "downloads.search" | "downloads.erase" => Ok(json!([])),
            "downloads.pause"
            | "downloads.resume"
            | "downloads.cancel"
            | "downloads.getFileIcon"
            | "downloads.open"
            | "downloads.show"
            | "downloads.showDefaultFolder"
            | "downloads.removeFile"
            | "downloads.acceptDanger"
            | "downloads.setUiOptions"
            | "downloads.setShelfEnabled" => Ok(Value::Null),
            "management.getSelf" => {
                let perms: Vec<&String> = ext
                    .manifest
                    .permissions
                    .iter()
                    .filter(|p| {
                        !p.contains("://")
                            && p.as_str() != "<all_urls>"
                            && super::manifest::is_supported_permission(p)
                    })
                    .collect();
                let options_url = ext
                    .manifest
                    .options_page
                    .as_deref()
                    .and_then(|p| extension_page_url(ext_id, p))
                    .unwrap_or_default();
                Ok(json!({
                    "id": ext_id,
                    "name": ext.manifest.name,
                    "shortName": ext.manifest.name,
                    "description": "",
                    "version": ext.manifest.version,
                    "enabled": true,
                    "type": "extension",
                    "installType": "normal",
                    "mayDisable": false,
                    "mayEnable": false,
                    "isApp": false,
                    "offlineEnabled": false,
                    "homepageUrl": "",
                    "updateUrl": "",
                    "optionsUrl": options_url,
                    "permissions": perms,
                    "hostPermissions": ext.manifest.hosts,
                }))
            }
            "management.getAll" | "management.getPermissionWarningsByManifest" => Ok(json!([])),
            "management.get"
            | "management.setEnabled"
            | "management.uninstall"
            | "management.uninstallSelf"
            | "management.launchApp"
            | "management.createAppShortcut"
            | "management.setLaunchType"
            | "management.getPermissionWarningsById"
            | "management.generateAppForLink" => {
                Err("Extensions cannot manage other extensions in this browser.".into())
            }
            "identity.getAuthToken" => Err("This browser has no browser account to sign in with.".into()),
            "identity.getProfileUserInfo" => Ok(json!({ "email": "", "id": "" })),
            "identity.getAccounts" => Ok(json!([])),
            "identity.removeCachedAuthToken" | "identity.clearAllCachedAuthTokens" => Ok(Value::Null),
            "identity.launchWebAuthFlow" => Err("This browser cannot open a sign-in window for extensions.".into()),
            "identity.getRedirectURL" => {
                let raw_path = args.first().and_then(Value::as_str).unwrap_or("");
                let clean: String = raw_path
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(2048)
                    .collect();
                Ok(Value::String(format!("https://{ext_id}.chromiumapp.org/{clean}")))
            }
            "commands.getAll" => {
                let list: Vec<Value> = ext
                    .manifest
                    .commands
                    .iter()
                    .map(|c| json!({ "name": c.name, "description": c.description, "shortcut": "" }))
                    .collect();
                Ok(Value::Array(list))
            }
            "extension.isAllowedFileSchemeAccess" | "extension.isAllowedIncognitoAccess" => Ok(Value::Bool(false)),
            "webRequest.handlerBehaviorChanged" | "extension.setUpdateUrlData" => Ok(Value::Null),
            other if parse_privacy_call(other).is_some() => {
                let (path, op) = parse_privacy_call(other).unwrap();
                if op == "get" {
                    let val = privacy_setting_default(path).unwrap_or(Value::Bool(false));
                    Ok(json!({ "value": val, "levelOfControl": "not_controllable" }))
                } else {
                    Ok(Value::Null)
                }
            }
            _ => Err(format!("chrome.{name} is not available in this browser.")),
        };

        match result {
            Ok(val) => self.reply_ok(sid, req_id, val),
            Err(msg) => self.reply_err(sid, req_id, &msg),
        }
        effects
    }

    fn rpc_tabs_update(
        &mut self,
        _sid: &str,
        ext_id: &str,
        _req_id: i64,
        manifest: &ExtensionManifest,
        args: &[Value],
        effects: &mut Vec<ExtEffect>,
    ) -> Result<Value, String> {
        let (tid_opt, change) = if args.len() >= 2 {
            (args[0].as_i64().map(|n| n as i32), args[1].as_object())
        } else {
            (None, args.first().and_then(Value::as_object))
        };
        let target_idx = match tid_opt {
            Some(cid) => self.find_tab_pos_by_ext_cid(ext_id, cid),
            None => self.tabs.iter().position(|t| t.active),
        };
        let Some(idx) = target_idx else {
            return Err("No tab to update.".into());
        };
        if self.tabs[idx].agent_driven {
            return Err("Extensions cannot modify agent-driven tabs.".into());
        }
        if is_extension_scheme(&self.tabs[idx].url)
            && extension_id_from_url(&self.tabs[idx].url) != Some(ext_id)
        {
            return Err("Extensions cannot navigate another extension's page.".into());
        }
        let engine_tab_id = self.tabs[idx].engine_tab_id;
        if let Some(ch) = change {
            if let Some(raw_url) = ch.get("url").and_then(Value::as_str) {
                let url = resolve_extension_nav_url(ext_id, raw_url)?;
                // Never upgrade a detached web tab into a `chrome-extension://` document in-place
                // (prevents bypassing per-target `Fetch.enable` and shim attach; extensions open
                // their own pages via `chrome.tabs.create`; `review-rd274.md` Round-2 §1d).
                if is_extension_scheme(&url)
                    && extension_id_from_url(&self.tabs[idx].url) != Some(ext_id)
                {
                    return Err("Cannot navigate a web tab to an extension page; use chrome.tabs.create.".into());
                }
                effects.push(ExtEffect::NavigateTab { engine_tab_id, url });
            }
            if ch.get("active").and_then(Value::as_bool) == Some(true)
                || ch.get("highlighted").and_then(Value::as_bool) == Some(true)
            {
                effects.push(ExtEffect::FocusTab { engine_tab_id });
            }
            if let Some(muted) = ch.get("muted").and_then(Value::as_bool) {
                self.tabs[idx].muted = muted;
            }
        }
        Ok(self.tab_json(ext_id, &self.tabs[idx], manifest, false))
    }

    fn rpc_action(
        &mut self,
        ext_id: &str,
        dir: &Path,
        manifest: &ExtensionManifest,
        name: &str,
        args: &[Value],
        effects: &mut Vec<ExtEffect>,
    ) -> Result<Value, String> {
        match name {
            "action.setTitle" => {
                if !self.allow_action_update(ext_id) {
                    return Ok(Value::Null);
                }
                let title: String = args
                    .first()
                    .and_then(|v| v.get("title"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .chars()
                    .take(256)
                    .collect();
                let tid = arg_tab_id(args.first());
                self.actions.entry(ext_id.to_owned()).or_default().apply(tid, |a| a.title = Some(title));
                if let Some(snap) = self.action_snapshot(ext_id) {
                    effects.push(ExtEffect::ActionUpdated(snap));
                }
                Ok(Value::Null)
            }
            "action.getTitle" => {
                let tid = arg_tab_id(args.first());
                let title = self
                    .actions
                    .get(ext_id)
                    .and_then(|s| s.for_tab(tid).title)
                    .or_else(|| manifest.action.as_ref().and_then(|a| a.default_title.clone()))
                    .unwrap_or_else(|| manifest.name.clone());
                Ok(Value::String(title))
            }
            "action.setIcon" => {
                if !self.allow_action_update(ext_id) {
                    return Ok(Value::Null);
                }
                let tid = arg_tab_id(args.first());
                if let Some(url) = args.first().and_then(|d| resolve_action_icon(dir, d)) {
                    self.actions.entry(ext_id.to_owned()).or_default().apply(tid, |a| a.icon = Some(url));
                    if let Some(snap) = self.action_snapshot(ext_id) {
                        effects.push(ExtEffect::ActionUpdated(snap));
                    }
                }
                Ok(Value::Null)
            }
            "action.setPopup" => {
                if !self.allow_action_update(ext_id) {
                    return Ok(Value::Null);
                }
                let popup: String = args
                    .first()
                    .and_then(|v| v.get("popup"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .chars()
                    .take(1024)
                    .collect();
                if !popup.is_empty() && extension_page_url(ext_id, &popup).is_none() {
                    return Err("The popup must be a page of this extension.".into());
                }
                let tid = arg_tab_id(args.first());
                self.actions.entry(ext_id.to_owned()).or_default().apply(tid, |a| a.popup = Some(popup));
                if let Some(snap) = self.action_snapshot(ext_id) {
                    effects.push(ExtEffect::ActionUpdated(snap));
                }
                Ok(Value::Null)
            }
            "action.getPopup" => {
                let tid = arg_tab_id(args.first());
                let rel = self
                    .actions
                    .get(ext_id)
                    .and_then(|s| s.for_tab(tid).popup)
                    .or_else(|| manifest.action.as_ref().and_then(|a| a.default_popup.clone()))
                    .unwrap_or_default();
                Ok(Value::String(extension_page_url(ext_id, &rel).unwrap_or_default()))
            }
            "action.setBadgeText" => {
                if !self.allow_action_update(ext_id) {
                    return Ok(Value::Null);
                }
                let text: String = args
                    .first()
                    .and_then(|v| v.get("text"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .chars()
                    .take(32)
                    .collect();
                let tid = arg_tab_id(args.first());
                self.actions.entry(ext_id.to_owned()).or_default().apply(tid, |a| a.badge_text = Some(text));
                if let Some(snap) = self.action_snapshot(ext_id) {
                    effects.push(ExtEffect::ActionUpdated(snap));
                }
                Ok(Value::Null)
            }
            "action.getBadgeText" => {
                let tid = arg_tab_id(args.first());
                Ok(Value::String(self.actions.get(ext_id).and_then(|s| s.for_tab(tid).badge_text).unwrap_or_default()))
            }
            "action.setBadgeBackgroundColor" => {
                if !self.allow_action_update(ext_id) {
                    return Ok(Value::Null);
                }
                if let Some(c) = args.first().and_then(|v| v.get("color")).and_then(parse_badge_color) {
                    let tid = arg_tab_id(args.first());
                    self.actions.entry(ext_id.to_owned()).or_default().apply(tid, |a| a.badge_color = Some(c));
                    if let Some(snap) = self.action_snapshot(ext_id) {
                        effects.push(ExtEffect::ActionUpdated(snap));
                    }
                }
                Ok(Value::Null)
            }
            "action.getBadgeBackgroundColor" => {
                let tid = arg_tab_id(args.first());
                let c = self.actions.get(ext_id).and_then(|s| s.for_tab(tid).badge_color).unwrap_or_else(|| "rgba(0, 0, 0, 0)".into());
                Ok(Value::String(c))
            }
            "action.setBadgeTextColor" => {
                if !self.allow_action_update(ext_id) {
                    return Ok(Value::Null);
                }
                if let Some(c) = args.first().and_then(|v| v.get("color")).and_then(parse_badge_color) {
                    let tid = arg_tab_id(args.first());
                    self.actions.entry(ext_id.to_owned()).or_default().apply(tid, |a| a.badge_text_color = Some(c));
                    if let Some(snap) = self.action_snapshot(ext_id) {
                        effects.push(ExtEffect::ActionUpdated(snap));
                    }
                }
                Ok(Value::Null)
            }
            "action.getBadgeTextColor" => {
                let tid = arg_tab_id(args.first());
                let c = self.actions.get(ext_id).and_then(|s| s.for_tab(tid).badge_text_color).unwrap_or_else(|| "#ffffff".into());
                Ok(Value::String(c))
            }
            "action.enable" => {
                if !self.allow_action_update(ext_id) {
                    return Ok(Value::Null);
                }
                let tid = args.first().and_then(Value::as_i64).map(|n| n as i32);
                self.actions.entry(ext_id.to_owned()).or_default().apply(tid, |a| a.enabled = Some(true));
                if let Some(snap) = self.action_snapshot(ext_id) {
                    effects.push(ExtEffect::ActionUpdated(snap));
                }
                Ok(Value::Null)
            }
            "action.disable" => {
                if !self.allow_action_update(ext_id) {
                    return Ok(Value::Null);
                }
                let tid = args.first().and_then(Value::as_i64).map(|n| n as i32);
                self.actions.entry(ext_id.to_owned()).or_default().apply(tid, |a| a.enabled = Some(false));
                if let Some(snap) = self.action_snapshot(ext_id) {
                    effects.push(ExtEffect::ActionUpdated(snap));
                }
                Ok(Value::Null)
            }
            "action.isEnabled" => {
                let tid = args.first().and_then(Value::as_i64).map(|n| n as i32);
                Ok(Value::Bool(self.actions.get(ext_id).and_then(|s| s.for_tab(tid).enabled).unwrap_or(true)))
            }
            "action.getUserSettings" => Ok(json!({ "isOnToolbar": true })),
            "action.openPopup" => Err("A popup opens only when the user clicks the extension.".into()),
            _ => Err(format!("chrome.{name} is not available in this browser.")),
        }
    }

    pub(super) fn answer_tabs_query(
        &self,
        ext_id: &str,
        manifest: &ExtensionManifest,
        query: Option<&serde_json::Map<String, Value>>,
    ) -> Vec<Value> {
        let active_q = query.and_then(|q| q.get("active")).and_then(Value::as_bool);
        let highlighted_q = query.and_then(|q| q.get("highlighted")).and_then(Value::as_bool);
        let cur_win_q = query.and_then(|q| q.get("currentWindow")).and_then(Value::as_bool);
        let last_win_q = query.and_then(|q| q.get("lastFocusedWindow")).and_then(Value::as_bool);
        let win_id_q = query.and_then(|q| q.get("windowId")).and_then(Value::as_i64).map(|n| n as i32);
        let index_q = query.and_then(|q| q.get("index")).and_then(Value::as_u64);
        let title_q = query.and_then(|q| q.get("title")).and_then(Value::as_str);
        let status_q = query.and_then(|q| q.get("status")).and_then(Value::as_str);
        let discarded_q = query.and_then(|q| q.get("discarded")).and_then(Value::as_bool);
        let frozen_q = query.and_then(|q| q.get("frozen")).and_then(Value::as_bool);
        let url_patterns: Vec<String> = match query.and_then(|q| q.get("url")) {
            Some(Value::String(s)) => vec![s.clone()],
            Some(Value::Array(arr)) => arr.iter().filter_map(Value::as_str).map(str::to_owned).collect(),
            _ => Vec::new(),
        };

        let mut out = Vec::new();
        for (idx, tab) in self.visible_tabs().enumerate() {
            let revealed = manifest.reveals_tab(&tab.url);
            if !revealed && (!url_patterns.is_empty() || title_q.is_some()) {
                continue;
            }
            if active_q.is_some_and(|a| a != tab.active) || highlighted_q.is_some_and(|h| h != tab.active) {
                continue;
            }
            if cur_win_q == Some(false) || last_win_q == Some(false) {
                continue;
            }
            if let Some(wid) = win_id_q
                && wid != WINDOW_ID
                && wid != WINDOW_ID_CURRENT
            {
                continue;
            }
            if index_q.is_some_and(|i| i as usize != idx) {
                continue;
            }
            if !url_patterns.is_empty() && !matches_host(&url_patterns, &tab.url) {
                continue;
            }
            if let Some(tp) = title_q
                && !glob_match(tp, &tab.title)
            {
                continue;
            }
            let status = if tab.loading { "loading" } else { "complete" };
            if status_q.is_some_and(|s| s != status) {
                continue;
            }
            if discarded_q.is_some_and(|d| d != tab.lifecycle.is_discarded())
                || frozen_q.is_some_and(|f| f != tab.lifecycle.is_frozen())
            {
                continue;
            }
            out.push(self.tab_json(ext_id, tab, manifest, false));
        }
        out
    }

    pub(super) fn tab_json(
        &self,
        ext_id: &str,
        tab: &ExtTab,
        manifest: &ExtensionManifest,
        force_reveal: bool,
    ) -> Value {
        let idx = self
            .visible_tabs()
            .position(|t| t.engine_tab_id == tab.engine_tab_id)
            .unwrap_or(0);
        let id = self
            .ext_chromium_tab_id(ext_id, tab.engine_tab_id)
            .unwrap_or(tab.engine_tab_id as i32);
        let status = if tab.loading { "loading" } else { "complete" };
        let mut obj = json!({
            "id": id,
            "index": idx,
            "windowId": WINDOW_ID,
            "active": tab.active,
            "highlighted": tab.active,
            "selected": tab.active,
            "pinned": false,
            "incognito": false,
            "discarded": tab.lifecycle.is_discarded(),
            "autoDiscardable": true,
            "frozen": tab.lifecycle.is_frozen(),
            "groupId": -1,
            "status": status,
            "audible": tab.audible,
            "mutedInfo": { "muted": tab.muted },
            "width": tab.width,
            "height": tab.height,
        });
        let revealed = !is_private_url(&tab.url) && (force_reveal || manifest.reveals_tab(&tab.url));
        if revealed && let Some(m) = obj.as_object_mut() {
            m.insert("url".into(), Value::String(tab.url.clone()));
            m.insert("title".into(), Value::String(tab.title.clone()));
        }
        obj
    }

    pub(super) fn window_json(&self, ext_id: &str, manifest: &ExtensionManifest, populate: bool) -> Value {
        let mut obj = json!({
            "id": WINDOW_ID,
            "focused": self.window_focused,
            "top": 0,
            "left": 0,
            "width": self.window_width,
            "height": self.window_height,
            "incognito": false,
            "type": "normal",
            "state": "normal",
            "alwaysOnTop": false,
        });
        if populate && let Some(m) = obj.as_object_mut() {
            let tabs: Vec<Value> = self
                .visible_tabs()
                .map(|t| self.tab_json(ext_id, t, manifest, false))
                .collect();
            m.insert("tabs".into(), Value::Array(tabs));
        }
        obj
    }
}

fn arg_tab_id(arg: Option<&Value>) -> Option<i32> {
    arg?.get("tabId")?.as_i64().map(|n| n as i32)
}

fn parse_badge_color(val: &Value) -> Option<String> {
    if let Some(s) = val.as_str() {
        let trimmed = s.trim();
        if trimmed.is_empty() || trimmed.len() > 32 {
            return None;
        }
        if let Some(hex) = trimmed.strip_prefix('#') {
            if matches!(hex.len(), 3 | 4 | 6 | 8) && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Some(trimmed.to_owned());
            }
            return None;
        }
        let lower = trimmed.to_ascii_lowercase();
        if (lower.starts_with("rgb(") || lower.starts_with("rgba("))
            && lower.ends_with(')')
            && lower.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'(' | b')' | b',' | b'.' | b' ' | b'%'))
        {
            return Some(lower);
        }
        if lower.bytes().all(|b| b.is_ascii_lowercase()) {
            return Some(lower);
        }
        return None;
    }
    let arr = val.as_array()?;
    let r = arr.first().and_then(Value::as_u64).unwrap_or(0).min(255);
    let g = arr.get(1).and_then(Value::as_u64).unwrap_or(0).min(255);
    let b = arr.get(2).and_then(Value::as_u64).unwrap_or(0).min(255);
    let a = arr.get(3).and_then(Value::as_u64).unwrap_or(255).min(255);
    let alpha = ((a as f64 / 255.0) * 100.0).round() / 100.0;
    Some(format!("rgba({r}, {g}, {b}, {alpha})"))
}

fn resolve_action_icon(dir: &Path, details: &Value) -> Option<String> {
    if let Some(img_val) = details.get("imageData") {
        let chosen = if img_val.get("__imageData").and_then(Value::as_bool) == Some(true) {
            Some(img_val)
        } else if let Some(map) = img_val.as_object() {
            let mut sizes: Vec<(u32, &Value)> = map
                .iter()
                .filter_map(|(k, v)| Some((k.parse::<u32>().ok()?, v)))
                .collect();
            sizes.sort_by_key(|(sz, _)| *sz);
            sizes
                .iter()
                .find(|(sz, _)| *sz >= 32)
                .or_else(|| sizes.last())
                .map(|(_, v)| *v)
        } else {
            None
        };
        if let Some(px) = chosen
            && let (Some(w), Some(h), Some(b64)) = (
                px.get("width").and_then(Value::as_u64),
                px.get("height").and_then(Value::as_u64),
                px.get("rgba_b64").and_then(Value::as_str),
            )
            && w <= 128
            && h <= 128
            && b64.len() <= 96 * 1024
            && let Ok(rgba) = base64::engine::general_purpose::STANDARD.decode(b64)
        {
            return rgba_to_png_data_url(w as u32, h as u32, &rgba);
        }
    }
    if let Some(path_val) = details.get("path") {
        let rel = match path_val {
            Value::String(s) => Some(s.clone()),
            Value::Object(m) => {
                let map: HashMap<String, String> = m
                    .iter()
                    .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_owned())))
                    .collect();
                closest_icon(&map)
            }
            _ => None,
        };
        if let Some(r) = rel {
            return read_icon_data_url(dir, &r);
        }
    }
    None
}

fn resolve_extension_nav_url(ext_id: &str, raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.len() > MAX_NAV_URL_BYTES {
        return Err("URL exceeds maximum allowed length.".into());
    }
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("about:blank") {
        return Ok("about:blank".into());
    }
    if browser_policy::is_file_scheme(trimmed) {
        return Err("Extensions do not have file access.".into());
    }
    if crate::extensions::manifest::is_extension_scheme(trimmed) {
        if crate::extensions::manifest::extension_id_from_url(trimmed) != Some(ext_id) {
            return Err("Extensions can open only their own extension pages.".into());
        }
        return Ok(trimmed.to_owned());
    }
    if trimmed.starts_with("//") || trimmed.starts_with("\\\\") {
        return Err(format!("This browser does not open {trimmed}."));
    }
    if !trimmed.contains("://") {
        // Reject opaque schemes (`javascript:`, `data:`, `blob:`, `vbscript:`, `about:`, ...).
        let before_delim = trimmed.split(['/', '?', '#']).next().unwrap_or("");
        if before_delim.contains(':') {
            return Err(format!("This browser does not open {trimmed}."));
        }
        return extension_page_url(ext_id, trimmed)
            .ok_or_else(|| format!("This browser does not open {trimmed}."));
    }
    if browser_policy::destination_origin(trimmed).is_some() {
        return Ok(trimmed.to_owned());
    }
    Err(format!("This browser does not open {trimmed}."))
}
