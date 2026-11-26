//! Browser (`wasm32`) host stubs for the `Browser` Luau service and `ui.browserSurface`.
//!
//! No CEF sidecar runs in a browser, so `Browser:Available()` is `false`, `Browser:Engine()`
//! reports `"unsupported"`, and no browser tabs exist — matching `browser-pane` without its
//! `engine` feature.

use std::hash::{Hash, Hasher};

use mlua::{Function, Lua, MultiValue, Table, Value};
use rune::{El, Ime, col};
use rune_luau::{Completer, Data, Host, NodeArgs};

use crate::App;
use crate::plugin_host::cx::{self, Dep};
use crate::plugin_host::services::{lazy_signals, map, method};
use crate::plugin_host::vm::Vmx;

const UNSUPPORTED: &str = "The browser isn't available on this platform yet in Brocode";

pub fn build(lua: &Lua, vmx: &std::rc::Rc<Vmx>, t: &Table) -> mlua::Result<()> {
    method(lua, t, "Tabs", |lua, _sid: Option<String>| {
        cx::read(Dep::Browser);
        lua.create_table()
    })?;
    method(lua, t, "Get", |_, _id: String| {
        cx::read(Dep::Browser);
        Ok(Value::Nil)
    })?;
    method(lua, t, "List", |lua, (_sid, open): (String, Vec<String>)| {
        cx::read(Dep::Browser);
        let rows = open
            .into_iter()
            .map(|raw| {
                map([
                    ("id", Data::String(raw)),
                    ("url", Data::String(String::new())),
                    ("title", Data::String(String::new())),
                    ("open", Data::Bool(false)),
                ])
            })
            .collect();
        super::super::diff::lua_data(lua, Data::List(rows))
    })?;
    method(lua, t, "Active", |_, _sid: String| {
        cx::read(Dep::Browser);
        Ok(Value::Nil)
    })?;
    method(lua, t, "Selected", |_, _sid: Option<String>| {
        cx::read(Dep::Browser);
        Ok(Value::Nil)
    })?;
    method(lua, t, "Focused", |_, ()| {
        cx::read(Dep::Browser);
        Ok(Value::Nil)
    })?;
    method(lua, t, "Available", |_, ()| {
        cx::read(Dep::Browser);
        Ok(false)
    })?;
    method(lua, t, "Engine", |lua, ()| {
        cx::read(Dep::Browser);
        mlua::IntoLua::into_lua(
            map([
                ("state", Data::String("unsupported".into())),
                ("message", Data::Nil),
                ("downloads", Data::Bool(false)),
                ("policy", Data::Bool(false)),
                ("preview", Data::Bool(false)),
                ("find", Data::Bool(false)),
                ("extensions", Data::Bool(false)),
                ("cookies", Data::Bool(false)),
                ("clearCache", Data::Bool(false)),
                ("hardReload", Data::Bool(false)),
            ]),
            lua,
        )
    })?;
    method(lua, t, "NormalizeUrl", |_, (_url, _sid): (String, Option<String>)| -> mlua::Result<String> {
        Err(mlua::Error::runtime(UNSUPPORTED))
    })?;
    method(lua, t, "Prepare", |_, _seconds: Option<f64>| Ok(()))?;
    method(lua, t, "TakeAppLinkReview", |_, ()| Ok(Value::Nil))?;
    method(lua, t, "HistoryPrepare", |_, ()| Ok(()))?;
    method(lua, t, "HistorySearch", |lua, (_q, _l): (String, Option<usize>)| lua.create_table())?;
    method(lua, t, "HistoryVisit", |_, _: mlua::MultiValue| Ok(()))?;
    method(lua, t, "HistoryRetitle", |_, _: mlua::MultiValue| Ok(()))?;
    method(lua, t, "HistoryAdopt", |_, _: Value| Ok(()))?;
    method(lua, t, "HistoryForget", |_, _url: String| Ok(false))?;
    method(lua, t, "HistoryClear", |_, ()| Ok(false))?;
    method(lua, t, "HistoryCount", |_, ()| Ok(0i64))?;
    method(lua, t, "HistoryImportSources", |lua, ()| lua.create_table())?;
    method(lua, t, "HistoryImportProfile", |lua, (_b, _p): (String, String)| {
        let tbl = lua.create_table()?;
        tbl.raw_set("error", "Browser history import is available in the desktop app.")?;
        Ok(tbl)
    })?;
    method(lua, t, "ExtAvailable", |_, ()| Ok(false))?;
    method(lua, t, "ExtList", |lua, _: Value| lua.create_table())?;
    method(lua, t, "ExtBrowserSources", |lua, ()| lua.create_table())?;
    method(lua, t, "ExtBrowserList", |lua, _: mlua::MultiValue| lua.create_table())?;
    method(lua, t, "ExtCancelReview", |_, ()| Ok(()))?;
    method(lua, t, "ExtSetEnabled", |lua, _: mlua::MultiValue| {
        mlua::IntoLua::into_lua(map([("ok", Data::Bool(false))]), lua)
    })?;
    method(lua, t, "ExtRemove", |lua, _: mlua::MultiValue| {
        mlua::IntoLua::into_lua(map([("ok", Data::Bool(false))]), lua)
    })?;
    method(lua, t, "ExtSync", |_, _: mlua::MultiValue| Ok(()))?;
    method(lua, t, "ExtActions", |lua, _: mlua::MultiValue| lua.create_table())?;
    method(lua, t, "ExtClick", |_, _: mlua::MultiValue| Ok(()))?;
    method(lua, t, "ExtClosePopup", |_, ()| Ok(()))?;
    for (method_name, async_key) in [
        ("ExtStageStoreAsync", "Browser.ExtStageStoreAsync"),
        ("ExtStageBrowserAsync", "Browser.ExtStageBrowserAsync"),
        ("ExtStageFolderAsync", "Browser.ExtStageFolderAsync"),
        ("ExtApproveAsync", "Browser.ExtApproveAsync"),
    ] {
        let raw: Function = vmx.asyncs.raw_get(async_key)?;
        let wrapped: Function = opencode_plugin_runtime::chunk::load(lua, "local raw = ... return function(_self, ...) return raw(...) end")
            .set_name(format!("=Browser:{method_name}"))
            .call(raw)?;
        t.raw_set(method_name, wrapped)?;
    }
    let raw: Function = vmx.asyncs.raw_get("Browser.OpenAsync")?;
    let open: Function = opencode_plugin_runtime::chunk::load(lua, "local raw = ... return function(self, spec) local id = raw(spec) return if id then self:Get(id) else nil end")
        .set_name("=Browser:OpenAsync")
        .call(raw)?;
    t.raw_set("OpenAsync", open)?;
    lazy_signals(lua, vmx, t, "Browser", &["Changed", "Picked", "SiteBack", "AddressSelectionChanged", "ImportSite"])
}

pub fn asyncs(host: &Host<'_>, t: &Table) -> mlua::Result<()> {
    t.raw_set(
        "Browser.OpenAsync",
        host.async_function("Browser:OpenAsync", |_, _: MultiValue, done: Completer| {
            done.complete(Data::Nil);
            Ok(())
        })?,
    )?;
    for name in [
        "Browser.ExtStageStoreAsync",
        "Browser.ExtStageBrowserAsync",
        "Browser.ExtStageFolderAsync",
        "Browser.ExtApproveAsync",
    ] {
        t.raw_set(
            name,
            host.async_function(name, |_, _: MultiValue, done: Completer| {
                done.complete(map([
                    ("type", Data::String("failed".into())),
                    ("reason", Data::String("load".into())),
                ]));
                Ok(())
            })?,
        )?;
    }
    for name in [
        "BrowserTab.DevtoolsAsync",
        "BrowserTab.SiteCookiesAsync",
        "BrowserTab.ClearSiteAsync",
        "BrowserTab.ClearCacheAsync",
        "BrowserTab.ReadFileAsync",
    ] {
        t.raw_set(
            name,
            host.async_function(name, |_, _: MultiValue, done: Completer| {
                done.complete(map([("error", Data::String(UNSUPPORTED.into()))]));
                Ok(())
            })?,
        )?;
    }
    Ok(())
}

/// Preserves `Dep::Browser` invalidation for `Layout:RegionOpen` even without `browser-pane`.
pub fn version(app: &App) -> u64 {
    let mut h = std::hash::DefaultHasher::new();
    let side = app.kernel.slots.get(&crate::panels::SIDE);
    (
        rune::Tracked::untracked(&app.composer).focused,
        side.focus.is_focused(),
        format!("{:?}", side.active_pane_focus),
    )
        .hash(&mut h);
    app.panel_session_id()
        .and_then(|sid| side.sessions.get(sid))
        .map(|st| st.visibility.open())
        .hash(&mut h);
    h.finish()
}

pub fn pages(_: &App, _include_find: bool) -> Vec<(String, u64)> {
    Vec::new()
}

pub fn view(_: &mut NodeArgs) -> Result<El, String> {
    Ok(col().key("browser-no-page").grow(1.0).w_full())
}

pub fn ime(_: &mut App, _: &Ime) -> bool {
    false
}
