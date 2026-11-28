//! Loads the user's login shell environment on macOS/Linux and applies OpenCode Desktop's service
//! environment variables (`packages/desktop/src/main/service/shell-env.ts`,
//! `packages/desktop/src/main/lifecycle/environment.ts:49-60,127-147`).

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, HashSet};
#[cfg(unix)]
use std::path::Path;
#[cfg(unix)]
use std::sync::OnceLock;
#[cfg(unix)]
use std::time::Duration;

#[cfg(unix)]
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// Cached login-shell environment on Unix (probed at most once per process, off the UI thread).
#[cfg(unix)]
static SHELL_ENV: OnceLock<Option<BTreeMap<String, String>>> = OnceLock::new();

/// Builds the full environment map for spawning `opencode serve --service`, matching official
/// Desktop's `preferApplicationEnvironment` + `ensureLoopbackNoProxy`
/// (`packages/desktop/src/main/lifecycle/environment.ts:49-60,136-147`).
pub(super) fn service_env(isolated: bool) -> BTreeMap<String, String> {
    let mut env: BTreeMap<String, String> = std::env::vars().collect();

    #[cfg(unix)]
    if let Some(shell_env) = SHELL_ENV.get_or_init(load_unix_shell_env) {
        // Official deletes an inherited XDG_STATE_HOME when the user's shell didn't set one
        // (`environment.ts:53`), unless this run explicitly rooted dirs via RUNE_DIRS_ROOT.
        if !shell_env.contains_key("XDG_STATE_HOME") && !crate::dirs::rooted() {
            env.remove("XDG_STATE_HOME");
        }
        for (k, v) in shell_env {
            env.insert(k.clone(), v.clone());
        }
    }

    env.insert("OPENCODE_EXPERIMENTAL_ICON_DISCOVERY".into(), "true".into());
    env.insert("OPENCODE_EXPERIMENTAL_FILEWATCHER".into(), "true".into());
    env.insert("OPENCODE_CLIENT".into(), "desktop".into());

    if isolated && let Some(data) = crate::dirs::data() {
        env.insert("XDG_STATE_HOME".into(), data.to_string_lossy().into_owned());
    }

    apply_loopback_no_proxy(&mut env);
    env
}

/// Full user environment merged with the cached Unix login-shell environment, for plugin child
/// processes that need the user's shell `PATH` on GUI-launched macOS/Linux apps without sidecar-
/// specific overrides. Returns `None` on Windows or when the login shell probe is unavailable.
pub(super) fn login_env() -> Option<BTreeMap<String, String>> {
    #[cfg(unix)]
    {
        let shell_env = SHELL_ENV.get_or_init(load_unix_shell_env).as_ref()?;
        let mut env: BTreeMap<String, String> = std::env::vars().collect();
        for (k, v) in shell_env {
            env.insert(k.clone(), v.clone());
        }
        Some(env)
    }
    #[cfg(not(unix))]
    {
        None
    }
}

/// Merges `127.0.0.1,localhost,::1` into both `NO_PROXY` and `no_proxy`
/// (`packages/desktop/src/main/lifecycle/environment.ts:136-147`).
fn apply_loopback_no_proxy(env: &mut BTreeMap<String, String>) {
    let mut seen = HashSet::new();
    let mut merged = Vec::new();
    for source in [
        env.get("NO_PROXY").map(String::as_str).unwrap_or(""),
        env.get("no_proxy").map(String::as_str).unwrap_or(""),
        "127.0.0.1,localhost,::1",
    ] {
        for entry in source.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            if seen.insert(entry.to_owned()) {
                merged.push(entry.to_owned());
            }
        }
    }
    let joined = merged.join(",");
    env.insert("NO_PROXY".into(), joined.clone());
    env.insert("no_proxy".into(), joined);
}

#[cfg(unix)]
fn resolve_user_shell(env_shell: Option<&str>, login_shell: Option<&str>) -> String {
    let env_shell = env_shell.map(str::trim).filter(|s| !s.is_empty());
    let login = login_shell.map(str::trim).filter(|s| !s.is_empty() && *s != "unknown");
    env_shell.or(login).unwrap_or("/bin/sh").to_owned()
}

#[cfg(unix)]
fn is_nushell(shell: &str) -> bool {
    let name = Path::new(shell).file_name().and_then(|n| n.to_str()).unwrap_or("").to_ascii_lowercase();
    let raw = shell.to_ascii_lowercase();
    name == "nu" || name == "nu.exe" || raw.ends_with("\\nu.exe")
}

#[cfg(unix)]
fn parse_shell_env(out: &[u8]) -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    for entry in String::from_utf8_lossy(out).split('\0') {
        if entry.is_empty() {
            continue;
        }
        if let Some((k, v)) = entry.split_once('=')
            && !k.is_empty()
        {
            env.insert(k.to_owned(), v.to_owned());
        }
    }
    env
}

#[cfg(unix)]
enum Probe {
    Loaded(BTreeMap<String, String>),
    Timeout,
    Unavailable,
}

#[cfg(unix)]
fn probe_shell(shell: &str, mode: &str) -> Probe {
    use std::io::Read;
    use std::process::{Command, Stdio};
    use std::sync::mpsc;

    let mut child = match Command::new(shell)
        .args([mode, "-c", "env -0"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return Probe::Unavailable,
    };
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Probe::Unavailable;
    };
    let (tx, rx) = mpsc::channel();
    let reader = std::thread::Builder::new().name("opencode-shell-env".into()).spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.take(1 << 20).read_to_end(&mut buf);
        tx.send(buf).ok();
    });
    if reader.is_err() {
        let _ = child.kill();
        let _ = child.wait();
        return Probe::Unavailable;
    }

    let deadline = rune::time::Instant::now() + PROBE_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if rune::time::Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Probe::Timeout;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Probe::Unavailable;
            }
        }
    };
    if !status.is_some_and(|s| s.success()) {
        return Probe::Unavailable;
    }
    let out = rx.recv_timeout(Duration::from_millis(250)).unwrap_or_default();
    let env = parse_shell_env(&out);
    if env.is_empty() { Probe::Unavailable } else { Probe::Loaded(env) }
}

/// `loadShellEnv` (`packages/desktop/src/main/service/shell-env.ts:99-129`): tries `-il` first,
/// returns `None` on timeout without falling back, falls back to `-l` on non-zero/error.
#[cfg(unix)]
fn load_unix_shell_env() -> Option<BTreeMap<String, String>> {
    let env_shell = std::env::var_os("SHELL").and_then(|s| s.into_string().ok());
    let login = super::process::login_shell();
    let shell = resolve_user_shell(env_shell.as_deref(), login.as_deref());
    if is_nushell(&shell) {
        return None;
    }
    match probe_shell(&shell, "-il") {
        Probe::Loaded(env) => Some(env),
        Probe::Timeout => None,
        Probe::Unavailable => match probe_shell(&shell, "-l") {
            Probe::Loaded(env) => Some(env),
            Probe::Timeout | Probe::Unavailable => None,
        },
    }
}
