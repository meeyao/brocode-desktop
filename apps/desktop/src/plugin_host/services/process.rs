//! Desktop `Process` service adapter over `opencode_plugin_runtime::process::service`.

use std::rc::Rc;

use mlua::{Lua, Table, Value};
use rune_luau::{Data, Host};

use crate::plugin_host::cx;
use crate::plugin_host::plugin::Half;
use crate::plugin_host::vm::Vmx;

/// Installs the desktop process resolver once so plugin child processes resolve bare `"opencode"`
/// through `servers::sidecar` (bundled/staged CLI + service env) and inherit the user's Unix
/// login-shell environment on GUI launches.
#[cfg(not(target_arch = "wasm32"))]
pub fn init_resolver() {
    opencode_plugin_runtime::process::set_resolver(resolve_program);
}

#[cfg(target_arch = "wasm32")]
pub fn init_resolver() {}

#[cfg(not(target_arch = "wasm32"))]
fn resolve_program(program: &str) -> Option<opencode_plugin_runtime::process::Resolved> {
    if is_bare_opencode(program) {
        let (command, env) = crate::servers::sidecar::resolve_cli_command();
        let (resolved_program, prefix_args) = match command {
            Some(mut argv) if !argv.is_empty() => {
                let first = argv.remove(0);
                (first, argv)
            }
            _ => (program.to_owned(), Vec::new()),
        };
        return Some(opencode_plugin_runtime::process::Resolved {
            program: resolved_program,
            prefix_args,
            base_env: Some(env.into_iter().collect()),
        });
    }
    let env = crate::servers::sidecar::login_env()?;
    Some(opencode_plugin_runtime::process::Resolved {
        program: program.to_owned(),
        prefix_args: Vec::new(),
        base_env: Some(env.into_iter().collect()),
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn is_bare_opencode(program: &str) -> bool {
    #[cfg(windows)]
    {
        program.eq_ignore_ascii_case("opencode") || program.eq_ignore_ascii_case("opencode.exe")
    }
    #[cfg(not(windows))]
    {
        program == "opencode"
    }
}

pub fn build(lua: &Lua, vmx: &Rc<Vmx>, t: &Table) -> mlua::Result<()> {
    init_resolver();
    let v_vm = vmx.clone();
    opencode_plugin_runtime::process::service::build(
        lua,
        t,
        vmx.half.procs.clone(),
        move || v_vm.vm.get(),
        vmx.script_signal.clone(),
        vmx.asyncs.clone(),
        || cx::with_app(|app| app.plugins.waker.clone()).flatten(),
    )
}

pub fn asyncs(host: &Host<'_>, t: &Table, half: &Rc<Half>) -> mlua::Result<()> {
    opencode_plugin_runtime::process::service::asyncs(host, t, &half.procs, |lua| {
        lua.app_data_ref::<Rc<Vmx>>().map(|v| v.vm.get())
    })
}

pub fn drain(half: &Half, vm: u64) -> Vec<(Value, Data)> {
    half.procs.drain(vm)
}

pub fn reap(half: &Half, keep_vm: Option<u64>) {
    half.procs.reap(keep_vm);
}
