//! `BrowserTab`'s DevTools-backed async methods: `DevtoolsAsync` (the allow-listed CDP call),
//! `SiteCookiesAsync` and `ClearSiteAsync`. Each is sent on the next poll and answered by
//! `browser::poll` from the engine's reply, or fails after `DEVTOOLS_TIMEOUT`.

use mlua::{Lua, MultiValue, Value};
use rune::Tx;
use rune::time::Instant;
use rune_luau::{Completer, Data};

use super::{App, DEVTOOLS_TIMEOUT, map, not_running, scope, with_app};

/// Sends the allow-listed DevTools command on the next poll; `poll` answers it with `{ result }`,
/// or with `{ error }` when the policy or engine refused it, isn't running, or took too long.
pub(super) fn devtools(lua: &Lua, a: MultiValue, done: Completer) -> mlua::Result<()> {
    let (id, cdp, params, target): (String, String, Value, Option<String>) = mlua::FromLuaMulti::from_lua_multi(a, lua)?;
    let scope = scope::TabScope::capture(lua);
    let params = match <Data as mlua::FromLua>::from_lua(params, lua)?.to_json() {
        serde_json::Value::Object(o) => serde_json::Value::Object(o),
        _ => serde_json::json!({}),
    };
    with_app("BrowserTab:DevtoolsAsync", |app| {
        app.later(Box::new(move |s: &mut App, _: &mut Tx| {
            let failed = |message: String| map([("error", Data::String(message))]);
            let Some(id) = id.parse::<u64>().ok().filter(|&id| s.workspace_panes.browser.host.tab(id).is_some()) else {
                return done.complete(failed(format!("No browser tab with tabID {id}")));
            };
            if let Err(err) = scope.check_tab(s, id, true) {
                return done.complete(failed(err));
            }
            if let Some(res) = s.workspace_panes.browser.host.tab(id).map(|t| t.residency())
                && matches!(res, browser_pane::TabResidency::Hibernated { .. } | browser_pane::TabResidency::Waking)
            {
                if res.is_hibernated() && !s.wake_browser_tab(id, s.active_browser_tab_id() == Some(id)) {
                    return done.complete(failed("Browser tab could not be woken.".to_owned()));
                }
                return done.complete(failed("Tab is waking; retry once loaded.".to_owned()));
            }
            if !s.workspace_panes.browser.host.has_live_sidecar() {
                return done.complete(failed(not_running(&s.workspace_panes.browser.host.status())));
            }
            // Every `BrowserTab:DevtoolsAsync` caller (agent MCP tools and third-party Luau plugins)
            // marks the tab controlled so OS file choosers, HTTP auth, and permissions stay guarded.
            let driver = scope.driver(s).unwrap_or(browser_pane::TabDriver::Agent);
            let is_agent = matches!(driver, browser_pane::TabDriver::Agent);
            s.workspace_panes.browser.host.set_tab_driver(id, driver);
            if is_agent {
                s.mark_browser_tab_unseen(id);
            }
            let is_dialog = cdp == "Page.handleJavaScriptDialog";
            if !is_dialog && s.workspace_panes.browser.host.tab(id).is_some_and(|t| t.prompts.dialog.open().is_some()) {
                return done.complete(failed(format!("[browser.dialog_open] A JavaScript dialog is open in tab {id}. Answer or dismiss the dialog before continuing.")));
            }
            s.workspace_panes.browser.host.freeze_tab(id, false);
            match s.workspace_panes.browser.host.send_cdp(id, cdp, params, target) {
                Ok(request) => {
                    s.workspace_panes.browser.devtools.insert(request, crate::surfaces::browser::PendingDevtools { done, deadline: Instant::now() + DEVTOOLS_TIMEOUT, tab_id: id, is_dialog });
                }
                Err(err) => done.complete(failed(err)),
            }
        }));
        Ok(())
    })
}

/// The tab's site's cookie count (Site settings), answered by `poll` as a DevTools call is.
pub(super) fn site_cookies(lua: &Lua, a: MultiValue, done: Completer) -> mlua::Result<()> {
    let (id,): (String,) = mlua::FromLuaMulti::from_lua_multi(a, lua)?;
    let scope = scope::TabScope::capture(lua);
    with_app("BrowserTab:SiteCookiesAsync", |app| {
        app.later(Box::new(move |s: &mut App, _: &mut Tx| {
            let failed = |message: String| map([("error", Data::String(message))]);
            let Some(id) = id.parse::<u64>().ok().filter(|&id| s.workspace_panes.browser.host.tab(id).is_some()) else {
                return done.complete(failed(format!("No browser tab with tabID {id}")));
            };
            if let Err(err) = scope.check_tab(s, id, true) {
                return done.complete(failed(err));
            }
            if let Some(res) = s.workspace_panes.browser.host.tab(id).map(|t| t.residency())
                && matches!(res, browser_pane::TabResidency::Hibernated { .. } | browser_pane::TabResidency::Waking)
            {
                if res.is_hibernated() && !s.wake_browser_tab(id, s.active_browser_tab_id() == Some(id)) {
                    return done.complete(failed("Browser tab could not be woken.".to_owned()));
                }
                return done.complete(failed("Tab is waking; retry once loaded.".to_owned()));
            }
            if !s.workspace_panes.browser.host.has_live_sidecar() {
                return done.complete(failed(not_running(&s.workspace_panes.browser.host.status())));
            }
            s.workspace_panes.browser.host.freeze_tab(id, false);
            match s.workspace_panes.browser.host.site_cookies(id) {
                Ok(request) => {
                    s.workspace_panes.browser.devtools.insert(request, crate::surfaces::browser::PendingDevtools { done, deadline: Instant::now() + DEVTOOLS_TIMEOUT, tab_id: id, is_dialog: false });
                }
                Err(err) => done.complete(failed(err)),
            }
        }));
        Ok(())
    })
}

/// Clears the tab's site's storage and cookies (Site settings), answered by `poll` as a DevTools call is.
pub(super) fn clear_site(lua: &Lua, a: MultiValue, done: Completer) -> mlua::Result<()> {
    let (id,): (String,) = mlua::FromLuaMulti::from_lua_multi(a, lua)?;
    let scope = scope::TabScope::capture(lua);
    with_app("BrowserTab:ClearSiteAsync", |app| {
        app.later(Box::new(move |s: &mut App, _: &mut Tx| {
            let failed = |message: String| map([("error", Data::String(message))]);
            let Some(id) = id.parse::<u64>().ok().filter(|&id| s.workspace_panes.browser.host.tab(id).is_some()) else {
                return done.complete(failed(format!("No browser tab with tabID {id}")));
            };
            if let Err(err) = scope.check_tab(s, id, true) {
                return done.complete(failed(err));
            }
            if let Some(res) = s.workspace_panes.browser.host.tab(id).map(|t| t.residency())
                && matches!(res, browser_pane::TabResidency::Hibernated { .. } | browser_pane::TabResidency::Waking)
            {
                if res.is_hibernated() && !s.wake_browser_tab(id, s.active_browser_tab_id() == Some(id)) {
                    return done.complete(failed("Browser tab could not be woken.".to_owned()));
                }
                return done.complete(failed("Tab is waking; retry once loaded.".to_owned()));
            }
            if !s.workspace_panes.browser.host.has_live_sidecar() {
                return done.complete(failed(not_running(&s.workspace_panes.browser.host.status())));
            }
            scope.mark_tab_driven(s, id);
            s.workspace_panes.browser.host.freeze_tab(id, false);
            match s.workspace_panes.browser.host.clear_site(id) {
                Ok(request) => {
                    s.workspace_panes.browser.devtools.insert(request, crate::surfaces::browser::PendingDevtools { done, deadline: Instant::now() + DEVTOOLS_TIMEOUT, tab_id: id, is_dialog: false });
                }
                Err(err) => done.complete(failed(err)),
            }
        }));
        Ok(())
    })
}
