//! The contract's host services, as `ctx:GetService(name)` returns them. Each is a table of
//! methods built once per VM; its signals (`Sessions.Changed`) are made when first read, so the
//! host fires only what a plugin listens to. Reads in a render record what they read
//! ([`super::cx::read`]); changes are queued ([`super::cx::ask`]) and carried out after the entry.
//!
//! LU-2 implements: Sessions, Models, Server, Servers, Http, Process, Store, Contracts, Console,
//! Toast, Popover, Dialog, Menu, Layout, Links, Commands, Theme, App, System, Notifications,
//! Workspace (files and downloads), Diagnostics (metrics) and I18n; LU-4 Power, Qr and Askpass.
//! The rest answer with an error naming the package that brings them (see `PENDING`).

pub(super) mod app;
pub mod askpass;
pub mod browser_contract;
pub mod diag;
mod i18n;
pub mod images;
mod net;
pub mod power;
#[cfg(feature = "office")]
mod office;
pub mod process;
mod qr;
mod session;
pub mod store;
mod ui;
mod updater;
mod watch;

use std::hash::{Hash, Hasher};
use std::rc::Rc;

use mlua::{Function, Lua, MultiValue, Table, Value};
use rune_luau::Host;

pub use opencode_plugin_runtime::luau::{json_to_data, json_to_lua, map};
pub(super) use opencode_plugin_runtime::luau::{method, record, snapshot};

use super::cx::{self, Dep};
use super::plugin::{Half, Kind};
use super::vm::Vmx;
use crate::App;

pub use i18n::locale_changed;
pub use session::session_version;
pub(crate) use session::CachedUsage;
pub use store::store_version;
pub use ui::panel_state as ui_panel_state;
pub use watch::{fire_watched, sync_editor_watches};
pub(crate) use watch::SessionInputs;

/// Services later packages bring, and which.
const PENDING: &[(&str, &str)] = &[];

/// Window-half-only services.
const WINDOW_ONLY: &[&str] = &["Popover", "Dialog", "Menu", "Layout", "Links", "Commands", "Diff", "Office", "Browser", "Composer", "Images"];

/// App-half-only services.
const APP_ONLY: &[&str] = &["Askpass"];

pub use opencode_plugin_runtime::contribs::Held;

/// Lets go of what a half's VMs other than `keep_vm` started or hold: processes, display locks
/// and prompt endpoints (the half turned off, failed or reloaded).
pub fn reap(half: &Half, keep_vm: Option<u64>) {
    process::reap(half, keep_vm);
    let gone: Vec<Held> = {
        let mut held = half.held.borrow_mut();
        let (keep, gone): (Vec<_>, Vec<_>) = held.drain(..).partition(|(vm, _)| Some(*vm) == keep_vm);
        *held = keep;
        gone.into_iter().map(|(_, h)| h).collect()
    };
    for h in gone {
        match h {
            Held::Power(id) => power::release(id),
            Held::Askpass(id) => askpass::close(id),
        }
    }
}

/// `ctx:GetService(name)`, built once per VM.
pub fn get(lua: &Lua, vmx: &Rc<Vmx>, name: &str) -> mlua::Result<Value> {
    let cache: Table = match lua.named_registry_value::<Option<Table>>("opencode.services")? {
        Some(t) => t,
        None => {
            let t = lua.create_table()?;
            lua.set_named_registry_value("opencode.services", &t)?;
            t
        }
    };
    if let Some(found) = cache.raw_get::<Option<Table>>(name)? {
        return Ok(Value::Table(found));
    }
    if vmx.half.kind == Kind::App && WINDOW_ONLY.contains(&name) {
        return Err(mlua::Error::runtime(format!("{name} is a window service: the app half has no window")));
    }
    if vmx.half.kind == Kind::Window && APP_ONLY.contains(&name) {
        return Err(mlua::Error::runtime(format!("{name} is an app service: get it in the app half (app.luau)")));
    }
    let t = lua.create_table()?;
    match name {
        "Sessions" | "Models" | "Server" | "Servers" | "Theme" => session::build(lua, vmx, name, &t)?,
        "Http" => net::build(lua, vmx, name, &t)?,
        "Workspace" => {
            net::build(lua, vmx, name, &t)?;
            super::native::build(lua, vmx, name, &t)?;
        }
        "Process" => process::build(lua, vmx, &t)?,
        "Store" | "Contracts" | "Console" => store::build(lua, vmx, name, &t)?,
        "Diagnostics" => diag::build(lua, vmx, &t)?,
        "Power" => power::build(lua, vmx, &t)?,
        "Qr" => qr::build(lua, vmx, &t)?,
        "Askpass" => askpass::build(lua, vmx, &t)?,
        "Toast" | "Popover" | "Dialog" | "Menu" | "Layout" | "Links" | "Commands" | "Notifications" => ui::build(lua, vmx, name, &t)?,
        "App" | "System" => app::build(lua, vmx, name, &t)?,
        "I18n" => i18n::build(lua, vmx, &t)?,
        "Updater" => updater::build(lua, vmx, &t)?,
        "Images" => images::build(lua, vmx, &t)?,
        #[cfg(feature = "office")]
        "Office" => office::build(lua, vmx, &t)?,
        "Diff" | "Terminal" | "Browser" | "Composer" => super::native::build(lua, vmx, name, &t)?,
        _ => {
            let why = PENDING.iter().find(|(n, _)| *n == name).map_or_else(|| format!("there is no service {name:?}"), |(_, pkg)| format!("{name} isn't in this app yet ({pkg} brings it)"));
            return Err(mlua::Error::runtime(why));
        }
    }
    cache.raw_set(name, &t)?;
    Ok(Value::Table(t))
}

/// The host's async functions, made with the VM (they need its `Host`).
pub fn asyncs(host: &Host<'_>, t: &Table, half: &Rc<Half>) -> mlua::Result<()> {
    net::asyncs(host, t, half)?;
    process::asyncs(host, t, half)?;
    diag::asyncs(host, t, half)?;
    #[cfg(feature = "office")]
    office::asyncs(host, t, half)?;
    updater::asyncs(host, t, half)?;
    super::native::asyncs(host, t)?;
    ui::asyncs(host, t, half)
}

/// Adds the async function `name` (made in [`asyncs`]) as method `method` of `t`.
pub(super) fn async_method(lua: &Lua, vmx: &Rc<Vmx>, t: &Table, method: &str, name: &str) -> mlua::Result<()> {
    let f: Function = vmx.asyncs.raw_get(name)?;
    let wrap: Function = opencode_plugin_runtime::chunk::load(lua, "local f = ... ; return function(_, ...) return f(...) end").call(f)?;
    t.raw_set(method, wrap)
}

/// A host signal: made now, fired by the host through [`fire`] under `name`.
pub fn signal(_: &Lua, vmx: &Rc<Vmx>, name: &str) -> mlua::Result<Value> {
    let key = (vmx.vm.get(), name.to_owned());
    if let Some(s) = vmx.half.signals.borrow().get(&key) {
        return Ok(s.clone());
    }
    let s: Value = vmx.script_signal.call(())?;
    vmx.half.signals.borrow_mut().insert(key, s.clone());
    Ok(s)
}

/// Makes signal fields of `t` (`Changed`, ...) as they are first read.
pub(super) fn lazy_signals(lua: &Lua, vmx: &Rc<Vmx>, t: &Table, service: &str, fields: &'static [&'static str]) -> mlua::Result<()> {
    let (v, service) = (vmx.clone(), service.to_owned());
    let mt = lua.create_table()?;
    mt.raw_set(
        "__index",
        lua.create_function(move |lua, (t, key): (Table, String)| {
            if !fields.contains(&key.as_str()) {
                return Ok(Value::Nil);
            }
            let s = signal(lua, &v, &format!("{service}.{key}"))?;
            // What it follows now is its first value: a change from here on fires it.
            cx::with_app(|app| watch::baseline(app, &v.half, &format!("{service}.{key}")));
            t.raw_set(key, &s)?;
            Ok(s)
        })?,
    )?;
    t.set_metatable(Some(mt))?;
    Ok(())
}

/// Fires host signal `name` of every running half in `halves` with `args`, if it made it.
pub fn fire(app: &App, plugin: &super::Plugin, name: &str, args: impl Fn(&Lua) -> mlua::Result<MultiValue>) {
    let (Some(script), Some(vm)) = (plugin.script(), plugin.script().and_then(|s| s.vm_id())) else { return };
    let half = &plugin.half;
    let Some(signal) = half.signals.borrow().get(&(vm, name.to_owned())).cloned() else { return };
    let Some(fire) = half.fire.borrow().get(&vm).cloned() else { return };
    cx::enter(app, || {
        let Some(Ok(mut values)) = script.with_lua(|lua| args(lua)) else { return };
        values.push_front(signal);
        script.spawn_fn(&fire, values, None).ok();
    });
}

/// `ctx:CreateEditor(initial)`: the n-th editor this setup made is the window's n-th text field
/// of the plugin, its text and caret kept across reloads.
pub fn editor(lua: &Lua, vmx: &Rc<Vmx>, initial: Option<String>) -> mlua::Result<Table> {
    let index = vmx.half.editors_made.get();
    vmx.half.editors_made.set(index + 1);
    super::editors::ensure(&vmx.half.id, index, initial.as_deref().unwrap_or_default());
    let t = lua.create_table()?;
    t.raw_set("id", index)?;
    t.raw_set("plugin", &*vmx.half.id)?;
    let mt = lua.create_table()?;
    let methods = lua.create_table()?;
    let id = vmx.half.id.clone();
    let p = id.clone();
    method(lua, &methods, "Text", move |_, ()| {
        cx::read(Dep::Editor(p.clone(), index));
        Ok(super::editors::text(&p, index))
    })?;
    let p = id.clone();
    method(lua, &methods, "Version", move |_, ()| {
        cx::read(Dep::Editor(p.clone(), index));
        Ok(super::editors::version(&p, index))
    })?;
    let p = id.clone();
    method(lua, &methods, "SetText", move |_, text: String| {
        super::editors::set_text(&p, index, &text);
        Ok(())
    })?;
    let p = id.clone();
    method(lua, &methods, "Focus", move |_, ()| {
        super::editors::focus(&p, index);
        cx::ask(&p, cx::Request::Redraw);
        Ok(())
    })?;
    let p = id.clone();
    method(lua, &methods, "Blur", move |_, ()| {
        super::editors::blur(&p, index);
        cx::ask(&p, cx::Request::Redraw);
        Ok(())
    })?;
    let p = id.clone();
    method(lua, &methods, "SelectAll", move |_, ()| {
        super::editors::select_all(&p, index);
        cx::ask(&p, cx::Request::Redraw);
        Ok(())
    })?;
    let p = id.clone();
    method(lua, &methods, "Focused", move |_, ()| {
        cx::read(Dep::Editor(p.clone(), index));
        Ok(super::editors::focused(&p, index))
    })?;
    methods.raw_set("Changed", signal(lua, vmx, &format!("editor.{index}.Changed"))?)?;
    methods.raw_set("Submitted", signal(lua, vmx, &format!("editor.{index}.Submitted"))?)?;
    methods.raw_set("FocusChanged", signal(lua, vmx, &format!("editor.{index}.FocusChanged"))?)?;
    methods.raw_set("KeyPressed", signal(lua, vmx, &format!("editor.{index}.KeyPressed"))?)?;
    mt.raw_set("__index", methods)?;
    mt.raw_set("__type", "Editor")?;
    t.set_metatable(Some(mt))?;
    Ok(t)
}

/// What `dep` reads now, as a number that changes with it.
pub fn dep_version(app: &App, dep: &Dep) -> u64 {
    let mut h = std::hash::DefaultHasher::new();
    match dep {
        Dep::Session(id) => session_version(app, id).hash(&mut h),
        Dep::Routed => app.active_session_id().hash(&mut h),
        Dep::Sessions => {
            let running = watch::running_ancestors(app);
            app.sessions
                .iter()
                .map(|s| session::session_version_for(app, s, running.contains(s.id.as_str())))
                .for_each(|v| v.hash(&mut h));
        }
        Dep::Model(provider, model) => session::model_version(app, provider, model).hash(&mut h),
        Dep::Models => session::models_version(app).hash(&mut h),
        Dep::Resource(plugin, path) => app.plugins.half(plugin).and_then(|half| half.resources.borrow().get(path).map(|r| r.ticket.generation())).hash(&mut h),
        Dep::Store(plugin, key) => store_version(plugin, key).hash(&mut h),
        Dep::Editor(plugin, index) => super::editors::version_in(app, plugin, *index).hash(&mut h),
        Dep::Popover(plugin, id) => app.plugins.overlay.popover_open(plugin, id).hash(&mut h),
        Dep::Flyout(plugin) => app.plugins.overlay.flyout_row(plugin).hash(&mut h),
        Dep::Dialog(plugin, id) => app.plugins.dialog.as_ref().is_some_and(|o| *o.plugin == **plugin && o.id == *id).hash(&mut h),
        Dep::Panel(plugin, panel) => ui::panel_state(app, plugin, panel, app.active_session_id().unwrap_or_default()).hash(&mut h),
        Dep::Narrow => ui::narrow(app).hash(&mut h),
        Dep::Locale => opencode_i18n::locale().id().hash(&mut h),
        Dep::Now => (*app.now_ms).hash(&mut h),
        Dep::Server => (app.server_key(), app.can_write()).hash(&mut h),
        Dep::Servers => servers_version(app).hash(&mut h),
        Dep::Contract(id) => store::contract_generation(id).hash(&mut h),
        Dep::Dev(name) => format!("{:?}", diag::get(app, name)).hash(&mut h),
        Dep::Console => super::console::version().hash(&mut h),
        Dep::Diff(session) => super::native::diff::version(app, session).hash(&mut h),
        Dep::DiffMode => (app.kernel.slots.get(&crate::review::REVIEW).diff_mode == rune_kit::diff::DiffMode::Split).hash(&mut h),
        Dep::DiffTotals(session) => super::native::diff::totals_version(app, session).hash(&mut h),
        Dep::DiffTree(session) => super::native::diff::tree_version(app, session).hash(&mut h),
        Dep::DiffMeta(session) => super::native::diff::meta_version(app, session).hash(&mut h),
        Dep::NarrowReview(session) => super::native::diff::narrow_version(app, session).hash(&mut h),
        Dep::Files(session) => super::native::files::version(app, session).hash(&mut h),
        Dep::FilesMissing(session, path) => super::native::files::missing_version(app, session, path).hash(&mut h),
        Dep::FilesTree(session) => super::native::files::tree_version(app, session).hash(&mut h),
        Dep::FilesDoc(session, path) => super::native::files::doc_version(app, session, path).hash(&mut h),
        Dep::Terminals(session) => super::native::terminal::version(app, session).hash(&mut h),
        Dep::TerminalScreen(session) => super::native::terminal::screen_version(app, session).hash(&mut h),
        Dep::Sidebar => (app.workspace_panes.panel_sidebar.opened, app.workspace_panes.panel_sidebar.width.map(f32::to_bits), app.workspace_panes.panel_sidebar.side).hash(&mut h),
        Dep::Plugin(id) => app.plugins.list.iter().find(|p| p.half.id == *id).map(|p| p.half.surface_version(p.script())).hash(&mut h),
        Dep::Updater => crate::updater::revision(app).hash(&mut h),
        Dep::Image(plugin, id) => app.plugins.shared.images.borrow().get(plugin, *id).map(crate::plugin_host::images::load::Load::status).hash(&mut h),
        Dep::Video(plugin, url) => app.plugins.shared.images.borrow().video_failed(plugin, url).hash(&mut h),
        #[cfg(feature = "office")]
        Dep::Office(id) => crate::office::registry::version(*id).hash(&mut h),
        #[cfg(not(feature = "office"))]
        Dep::Office(_) => 0.hash(&mut h),
        Dep::Browser => super::native::browser::version(app).hash(&mut h),
        Dep::Composer => {
            use crate::pickers::PickerHost as _;
            let sel = app.selection();
            (
                app.has_composer(),
                app.composer_directory(),
                sel.model.provider,
                sel.model.id,
                app.composer_ext.pickers.models.len(),
                app.composer_ext.pickers.open().map(crate::pickers::Kind::key),
            )
                .hash(&mut h);
        }
    }
    h.finish()
}

/// A number that changes with the listed servers and their health.
pub fn servers_version(app: &App) -> u64 {
    let mut h = std::hash::DefaultHasher::new();
    super::server_sources::version().hash(&mut h);
    crate::servers::version().hash(&mut h);
    crate::servers::runtime::version().hash(&mut h);
    crate::servers::health::version().hash(&mut h);
    let mut health: Vec<_> = app.plugins.shared.facts.health.iter().collect();
    health.sort();
    health.hash(&mut h);
    h.finish()
}

/// Runs `f` with the active entry's `&App`, or raises the error a binding raises without one.
pub(super) fn with_app<R>(what: &str, f: impl FnOnce(&App) -> mlua::Result<R>) -> mlua::Result<R> {
    cx::with_app(f).unwrap_or_else(|| Err(mlua::Error::runtime(format!("{what} can only be read while the plugin runs"))))
}
