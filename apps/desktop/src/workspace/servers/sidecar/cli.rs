//! Resolves, stages, and cleans up the brocode CLI executable used to run
//! brocode serve --service (packages/desktop/src/main/service/desktop-cli.ts,
//! desktop-cli-state.ts, cli-version.ts, cli-stages.ts).
//!
//! The packaged app ships brocode-cli beside itself; it is resolved, version-checked, and
//! atomically staged into <data>/cli/<version>-<arch>/ identically to official Desktop.
//! Without one (a source build), resolution falls back to an installed `brocode` (PATH /
//! login-shell PATH, ~/.brocode/bin, ~/.local/bin). Never `opencode`: this app only talks to
//! brocode.

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock, mpsc};
use std::time::Duration;

/// The first --version after an update can take several seconds while antivirus scans it
/// (desktop-cli.ts:17).
const RUN_TIMEOUT: Duration = Duration::from_secs(30);

/// Resolved (packages/desktop/src/main/service/desktop-cli.ts:19-23).
#[derive(Clone, Debug)]
pub(super) struct ResolvedCli {
    pub(super) version: Option<String>,
    pub(super) command: Vec<String>,
    /// Set only when we staged a bundled binary into <data>/cli/<stage>/, so clean_stages
    /// never touches external/official-Desktop directories.
    pub(super) staged_binary: Option<PathBuf>,
}

/// Explicit state machine mirroring packages/desktop/src/main/service/desktop-cli-state.ts:
/// concurrent callers share one lookup, successes are cached, and failures return to Idle.
#[derive(Default)]
enum ResolveState {
    #[default]
    Idle,
    Resolving(Vec<mpsc::Sender<Result<ResolvedCli, String>>>),
    Resolved(ResolvedCli),
}

static CLI_STATE: OnceLock<Mutex<ResolveState>> = OnceLock::new();

/// Resolves the CLI executable (caching successes, retrying failures on the next call).
pub(super) fn resolve(env: &BTreeMap<String, String>, isolated: bool) -> Result<ResolvedCli, String> {
    let lock = CLI_STATE.get_or_init(|| Mutex::new(ResolveState::Idle));
    let rx = {
        let mut state = lock.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        match &mut *state {
            ResolveState::Resolved(cli) => return Ok(cli.clone()),
            ResolveState::Resolving(waiters) => {
                let (tx, rx) = mpsc::channel();
                waiters.push(tx);
                Some(rx)
            }
            ResolveState::Idle => {
                *state = ResolveState::Resolving(Vec::new());
                None
            }
        }
    };
    if let Some(rx) = rx {
        return rx.recv().unwrap_or_else(|_| Err("CLI resolution cancelled".into()));
    }
    let result = resolve_uncached(env, isolated);
    let waiters = {
        let mut state = lock.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let prev = std::mem::replace(
            &mut *state,
            match &result {
                Ok(cli) => ResolveState::Resolved(cli.clone()),
                Err(_) => ResolveState::Idle,
            },
        );
        match prev {
            ResolveState::Resolving(w) => w,
            _ => Vec::new(),
        }
    };
    for tx in waiters {
        tx.send(result.clone()).ok();
    }
    result
}

fn packaged() -> bool {
    !cfg!(debug_assertions)
}

fn executable_name() -> &'static str {
    if cfg!(windows) { "brocode-cli.exe" } else { "brocode-cli" }
}

fn dev_executable_name() -> &'static str {
    if cfg!(windows) { "brocode-cli-dev.exe" } else { "brocode-cli-dev" }
}

fn cli_binary_name() -> &'static str {
    if cfg!(windows) { "brocode.exe" } else { "brocode" }
}

fn current_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        other => other,
    }
}

/// parseCliVersion (packages/desktop/src/main/service/cli-version.ts:1-8).
pub(super) fn parse_cli_version(output: &str) -> Result<String, String> {
    let trimmed = output.trim();
    let version = match trimmed.rfind(" v") {
        Some(idx) => &trimmed[idx + 2..],
        None => trimmed,
    };
    if version.is_empty() {
        return Err("V2 CLI did not provide a version".into());
    }
    Ok(version.to_owned())
}

/// stageName (packages/desktop/src/main/service/cli-version.ts:34-36).
pub(super) fn stage_name(version: &str) -> String {
    format!("{version}-{}", current_arch())
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') { c } else { '-' })
        .collect()
}

/// keptStages (packages/desktop/src/main/service/cli-version.ts:38-44).
pub(super) fn kept_stages(bundled_version: &str, running_version: Option<&str>) -> Vec<String> {
    let mut stages = vec![stage_name(bundled_version)];
    if let Some(running) = running_version
        && running != bundled_version
    {
        stages.push(stage_name(running));
    }
    stages
}

/// cleanStages (packages/desktop/src/main/service/cli-stages.ts:6-31): removes old staged
/// directories beside staged_binary except the current and running stages.
pub(super) fn clean_stages(staged_binary: &Path, keep: &[String]) {
    let Some(current) = staged_binary.parent() else { return };
    let Some(root) = current.parent() else { return };
    let mut keep_set: HashSet<&str> = keep.iter().map(String::as_str).collect();
    if let Some(name) = current.file_name().and_then(|n| n.to_str()) {
        keep_set.insert(name);
    }
    let Ok(entries) = std::fs::read_dir(root) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else { continue };
        if keep_set.contains(name_str) {
            continue;
        }
        let target = entry.path();
        if target.is_dir() {
            let _ = std::fs::remove_dir_all(&target);
        }
    }
}

fn resolve_uncached(env: &BTreeMap<String, String>, isolated: bool) -> Result<ResolvedCli, String> {
    // 1. Development CLI override (desktop-cli.ts:55-68).
    if !packaged()
        && let Some(dev_dir) = env.get("OPENCODE_DESKTOP_CLI_DEV").map(String::as_str).filter(|s| !s.is_empty())
    {
        let version = env
            .get("OPENCODE_VERSION")
            .cloned()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "local".into());
        return Ok(ResolvedCli {
            version: Some(version),
            command: vec![
                "bun".into(),
                "run".into(),
                "--cwd".into(),
                dev_dir.into(),
                "--define=OPENCODE_VERSION=process.env.OPENCODE_VERSION".into(),
                "src/index.ts".into(),
            ],
            staged_binary: None,
        });
    }

    // 2. Bundled brocode-cli shipped alongside this app (desktop-cli.ts:74-86).
    if let Some(bundled) = find_bundled_cli(isolated) {
        let version = bundled_version(&bundled)?;
        let (binary, staged) = if packaged() || isolated {
            let dest = stage_cli(&bundled, &version)?;
            (dest.clone(), Some(dest))
        } else {
            (bundled, None)
        };
        return Ok(ResolvedCli {
            version: Some(version),
            command: vec![binary.to_string_lossy().into_owned()],
            staged_binary: staged,
        });
    }

    // 3. An installed brocode CLI (source builds; packaged apps bundle theirs).
    if let Some(command) = find_system_cli(env) {
        return Ok(ResolvedCli { version: None, command, staged_binary: None });
    }

    Err("brocode CLI executable not found. Install brocode or run brocode serve --service.".into())
}

fn find_bundled_cli(isolated: bool) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?;
    let name = if !packaged() && isolated { dev_executable_name() } else { executable_name() };
    let mut candidates = Vec::new();
    #[cfg(target_os = "macos")]
    if let Some(contents) = exe_dir.parent() {
        candidates.push(contents.join("Resources").join(name));
    }
    candidates.push(exe_dir.join(name));
    #[cfg(target_os = "linux")]
    if let Some(prefix) = exe_dir.parent() {
        candidates.push(prefix.join("share").join("brocode").join(name));
    }
    candidates.into_iter().find(|p| p.is_file())
}

fn read_bundled_version_file(bundled: &Path) -> Option<String> {
    let text = std::fs::read_to_string(bundled.parent()?.join("brocode-cli.version")).ok()?;
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

fn bundled_version(bundled: &Path) -> Result<String, String> {
    if let Some(shipped) = read_bundled_version_file(bundled) {
        return Ok(shipped);
    }
    let output = run_version(bundled)?;
    parse_cli_version(&output)
}

fn run_version(binary: &Path) -> Result<String, String> {
    let mut cmd = Command::new(binary);
    cmd.arg("--version").stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd.spawn().map_err(|e| format!("Failed to run {}: {e}", binary.display()))?;
    let mut stdout = child.stdout.take();
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("brocode-cli-ver".into())
        .spawn(move || {
            let mut buf = Vec::new();
            if let Some(ref mut out) = stdout {
                let _ = out.take(64 << 10).read_to_end(&mut buf);
            }
            tx.send(buf).ok();
        })
        .ok();
    let deadline = rune::time::Instant::now() + RUN_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if rune::time::Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Timed out reading CLI version".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("Failed waiting for CLI version: {e}"));
            }
        }
    };
    if !status.success() {
        return Err(format!("CLI --version exited with status {status}"));
    }
    let bytes = rx.recv_timeout(Duration::from_millis(250)).unwrap_or_default();
    Ok(String::from_utf8_lossy(&bytes).trim().to_owned())
}

/// installCli (packages/desktop/src/main/service/desktop-cli.ts:132-155): atomically copies
/// the bundled CLI to <data>/cli/<version>-<arch>/brocode-cli[.exe] so running servers never
/// lock the app bundle during updates.
fn stage_cli(source: &Path, version: &str) -> Result<PathBuf, String> {
    let data = crate::dirs::data().ok_or_else(|| "App data directory unavailable".to_owned())?;
    let directory = data.join("cli").join(stage_name(version));
    let destination = directory.join(executable_name());
    if destination.is_file() {
        return Ok(destination);
    }
    std::fs::create_dir_all(&directory).map_err(|e| format!("Failed creating CLI stage dir: {e}"))?;
    let temp = directory.join(format!("{}.{}.tmp", executable_name(), std::process::id()));
    std::fs::copy(source, &temp).map_err(|e| format!("Failed copying CLI to stage: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o755));
    }
    if let Err(e) = std::fs::rename(&temp, &destination) {
        let _ = std::fs::remove_file(&temp);
        // Another instance/window may have won the rename race.
        if destination.is_file() {
            return Ok(destination);
        }
        return Err(format!("Failed staging CLI executable: {e}"));
    }
    Ok(destination)
}

/// Looks up the user's installed brocode CLI: resolved PATH, then its standard install dirs.
fn find_system_cli(env: &BTreeMap<String, String>) -> Option<Vec<String>> {
    if let Some(path_var) = env.get("PATH").or_else(|| env.get("Path"))
        && let Some(cmd) = search_path(path_var)
    {
        return Some(cmd);
    }
    let name = cli_binary_name();
    let mut candidates = Vec::new();
    if let Some(home) = rune::dirs::home_from(|k| std::env::var_os(k)) {
        candidates.push(home.join(".brocode").join("bin").join(name));
        candidates.push(home.join(".local").join("bin").join(name));
    }
    // install.ps1's default folder.
    #[cfg(windows)]
    if let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
        candidates.push(local.join("Programs").join("brocode").join(name));
    }
    #[cfg(target_os = "macos")]
    for dir in ["/opt/homebrew/bin", "/usr/local/bin"] {
        candidates.push(PathBuf::from(dir).join(name));
    }
    #[cfg(target_os = "linux")]
    for dir in ["/usr/local/bin", "/usr/bin"] {
        candidates.push(PathBuf::from(dir).join(name));
    }
    candidates.into_iter().find(|p| p.is_file()).map(|p| command_for_path(&p))
}

fn search_path(path_var: &str) -> Option<Vec<String>> {
    #[cfg(windows)]
    let mut fallback_cmd: Option<PathBuf> = None;
    for dir in std::env::split_paths(path_var) {
        let exe = dir.join(cli_binary_name());
        if exe.is_file() {
            return Some(command_for_path(&exe));
        }
        #[cfg(windows)]
        if fallback_cmd.is_none() {
            for ext in ["brocode.cmd", "brocode.bat"] {
                let candidate = dir.join(ext);
                if candidate.is_file() {
                    fallback_cmd = Some(candidate);
                    break;
                }
            }
        }
    }
    #[cfg(windows)]
    return fallback_cmd.map(|p| command_for_path(&p));
    #[cfg(not(windows))]
    None
}

fn command_for_path(path: &Path) -> Vec<String> {
    let s = path.to_string_lossy().into_owned();
    #[cfg(windows)]
    {
        let lower = s.to_ascii_lowercase();
        if lower.ends_with(".cmd") || lower.ends_with(".bat") {
            return vec!["cmd.exe".into(), "/c".into(), s];
        }
    }
    vec![s]
}
