//! macOS: team builds (`com.apibrother.brocode`) update from Anomaly's release feed. A download is
//! verified (size, SHA-256, code signature, signing team, Gatekeeper) and unpacked into a private
//! folder under `~/Library/Caches/com.apibrother.brocode/Updates`; Restart to update or quitting
//! with a ready update checks the staged bundle again and swaps it in atomically (the old bundle
//! comes back when the move fails), and Restart to update hands off to a detached non-GUI helper
//! that waits for this process to exit before reopening the swapped bundle via Launch Services
//! without `-n`, so macOS reuses any pinned ("Keep in Dock") Dock tile instead of spawning a
//! second icon. Local dev bundles (`com.apibrother.brocode.dev`) and bare `cargo` binaries never
//! update. A bundle this user can't replace (translocated, read-only) keeps checking and links to
//! the DMG; `DisableUpdates` in the app's defaults domain turns updates off.

use std::ffi::OsString;
use std::io;
use std::os::unix::fs::MetadataExt as _;
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use super::engine::{self, Engine, Facts};
use super::feed;
use super::platform::Platform;

/// Team release bundle ID installed from the release feed (the dev channel's, the only one).
pub const TEAM_BUNDLE_ID: &str = "com.apibrother.brocode";

/// Only the bundle the team release feed publishes updates (never local dev builds,
/// `com.apibrother.brocode.dev`, or a pull request's). A staged update must carry the running
/// bundle's own id (`verify_bundle`).
pub fn is_updatable_bundle_id(bundle_id: &str) -> bool {
    bundle_id == TEAM_BUNDLE_ID
}
const EXPECTED_SIGNING_TEAM: &str = "5NZ4Q7NXJ4";

/// The code requirement every update must satisfy (`codesign --verify -R`): Apple's anchor, the
/// bundle id, a Developer ID certificate of Anomaly's team, and Apple's notarization, as Hex's
/// `app/update-requirement.txt`.
fn update_requirement(bundle_id: &str, team: &str, notarized: bool) -> String {
    let base = format!("anchor apple generic and identifier \"{bundle_id}\" and certificate 1[field.1.2.840.113635.100.6.2.6] exists and certificate leaf[field.1.2.840.113635.100.6.1.13] exists and certificate leaf[subject.OU] = \"{team}\"");
    if notarized { format!("{base} and notarized") } else { base }
}

/// Whether an administrator turned updates off: `DisableUpdates` in the app's defaults domain,
/// set by a configuration profile (managed preferences) or `defaults write`.
fn updates_disabled_by_policy(bundle_id: &str) -> bool {
    let user = feed::env("USER").unwrap_or_default();
    let domains = [
        bundle_id.to_owned(),
        format!("/Library/Preferences/{bundle_id}"),
        format!("/Library/Managed Preferences/{bundle_id}"),
        format!("/Library/Managed Preferences/{user}/{bundle_id}"),
    ];
    domains.iter().any(|domain| {
        Command::new("/usr/bin/defaults")
            .args(["read", domain, "DisableUpdates"])
            .output()
            .is_ok_and(|out| out.status.success() && matches!(String::from_utf8_lossy(&out.stdout).trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
    })
}

static STAGE_SEQ: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BundleInfo {
    pub path: PathBuf,
    pub id: String,
    pub version: String,
    pub team: Option<String>,
}

struct Staged {
    version: String,
    dir: PathBuf,
    app: PathBuf,
    sha256: String,
}

pub struct Mac {
    engine: Engine,
    bundle: BundleInfo,
    exe_rel: PathBuf,
    check_team: Option<String>,
    check_gatekeeper: bool,
    staged: Mutex<Option<Staged>>,
}

/// The updater for the running bundle, when it's a team build; `None` keeps it off.
pub fn platform(exe: &Path, updates: PathBuf) -> Option<Mac> {
    let exe = exe.canonicalize().ok()?;
    let app = bundle_root_from_exe(&exe)?;
    let exe_rel = exe.strip_prefix(&app).ok()?.to_path_buf();
    let bundle = bundle_info(&app).ok()?;
    if !is_updatable_bundle_id(&bundle.id) {
        return None;
    }
    if updates_disabled_by_policy(&bundle.id) {
        eprintln!("updater: off (DisableUpdates is set for {})", bundle.id);
        return None;
    }
    let service = feed::debug_release_url().unwrap_or_else(|| feed::DEFAULT_SERVICE_URL.to_owned());
    if !feed::allowed_update_url(&service) {
        return None;
    }
    let local_loopback_test = feed::debug_skip_gatekeeper() && feed::loopback(&service) && bundle.team.is_none();
    // Always Anomaly's team, never whichever team signed the running bundle.
    let (check_team, check_gatekeeper) = if local_loopback_test { (None, false) } else { (Some(EXPECTED_SIGNING_TEAM.to_owned()), true) };
    sweep_old_stages(&updates);
    sweep_swap_leftovers(&app);
    sync_installed_receipt(&updates, &exe, &app.join(&exe_rel));
    let facts = Facts { version: bundle.version.clone(), channel: feed::feed_channel(), service, updates };
    Some(Mac::new(facts, bundle, exe_rel, check_team, check_gatekeeper))
}

impl Mac {
    pub fn new(facts: Facts, bundle: BundleInfo, exe_rel: PathBuf, check_team: Option<String>, check_gatekeeper: bool) -> Self {
        Mac { engine: Engine::new(facts), bundle, exe_rel, check_team, check_gatekeeper, staged: Mutex::new(None) }
    }

    /// Swaps the staged bundle in, without opening it (`install` spawns the post-exit relaunch
    /// helper). Writes a receipt keyed by the placed executable's `(dev, ino)` into `Updates/`
    /// after `swap` succeeds so the record survives even when `apply` runs after `storage::close()`.
    pub fn apply(&self) -> Result<(), String> {
        self.apply_inner(false)
    }

    fn apply_inner(&self, on_quit: bool) -> Result<(), String> {
        let staged = self.staged.lock().ok().and_then(|mut s| s.take()).ok_or_else(|| "No staged update ready to install.".to_owned())?;
        // On quit, `spctl --assess` already passed during `stage()` and can stall online past
        // `exit_deadline`, so verify bundle ID, `codesign --verify --strict --deep`, and signing
        // team locally; also skip `engine::record_installed` because `storage::close()` has run
        // (`sync_installed_receipt` promotes `installed-bundle.json` on next boot).
        let check_gatekeeper = if on_quit { false } else { self.check_gatekeeper };
        if let Err(e) = verify_bundle(&staged.app, &self.bundle.id, self.check_team.as_deref(), check_gatekeeper) {
            std::fs::remove_dir_all(&staged.dir).ok();
            return Err(e);
        }
        swap(&staged.app, &self.bundle.path)?;
        write_installed_receipt(&self.engine.facts.updates, &self.bundle.path.join(&self.exe_rel), &staged.version, &staged.sha256);
        if !on_quit {
            engine::record_installed(&staged.version, &staged.sha256);
        }
        std::fs::remove_dir_all(&staged.dir).ok();
        Ok(())
    }
}

impl Platform for Mac {
    fn version(&self) -> &str {
        &self.bundle.version
    }

    fn check(&self) -> Result<Option<super::machine::Target>, String> {
        if let Some(r) = read_installed_receipt(&self.engine.facts.updates)
            && file_id(&self.bundle.path.join(&self.exe_rel)) == Some((r.dev, r.ino))
            && r.version != self.bundle.version
        {
            return Ok(Some(super::machine::Target::Installed(r.version)));
        }
        let Some(version) = self.engine.check()? else { return Ok(None) };
        // A bundle this user can't replace (translocated, read-only, another user's) is never
        // overwritten: the update links to the DMG instead.
        if let Err(why) = check_replaceable(&self.bundle.path) {
            eprintln!("updater: {why}; offering the download instead");
            return Ok(Some(super::machine::Target::External { version, url: self.download_url() }));
        }
        Ok(Some(super::machine::Target::Restart(version)))
    }

    fn stage(&self, progress: &dyn Fn(u64, u64)) -> Result<(), String> {
        let found = self.engine.found().ok_or_else(|| "No update to download".to_owned())?;
        if let Ok(staged) = self.staged.lock()
            && let Some(st) = staged.as_ref()
            && st.sha256 == found.target.sha256
            && st.app.exists()
        {
            return Ok(());
        }
        check_replaceable(&self.bundle.path)?;
        let root = &self.engine.facts.updates;
        std::fs::create_dir_all(root).map_err(|e| format!("could not create update dir: {e}"))?;
        let dir = root.join(format!("stage-{}-{}", std::process::id(), STAGE_SEQ.fetch_add(1, Ordering::Relaxed)));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).map_err(|e| format!("could not create stage dir: {e}"))?;
        let result = (|| {
            let zip = dir.join("update.zip");
            let found = self.engine.download(&zip, progress)?;
            let unpacked = dir.join("unpacked");
            std::fs::create_dir_all(&unpacked).map_err(|e| format!("could not create unpack dir: {e}"))?;
            let app = unpack_zip(&zip, &unpacked)?;
            std::fs::remove_file(&zip).ok();
            verify_bundle(&app, &self.bundle.id, self.check_team.as_deref(), self.check_gatekeeper)?;
            first_run(&app.join(&self.exe_rel));
            Ok(Staged { version: found.version, dir: dir.clone(), app, sha256: found.target.sha256 })
        })();
        match result {
            Ok(staged) => {
                if let Ok(mut slot) = self.staged.lock()
                    && let Some(prev) = slot.replace(staged)
                {
                    std::fs::remove_dir_all(prev.dir).ok();
                }
                Ok(())
            }
            Err(e) => {
                std::fs::remove_dir_all(&dir).ok();
                Err(e)
            }
        }
    }

    fn install(&self) -> Result<(), String> {
        self.apply()?;
        self.spawn_relaunch_helper()
    }

    /// Handled by `spawn_relaunch_helper` after this process exits (not `Tx::relaunch`, which
    /// runs `open -n` while the old process is still alive and causes macOS Dock to spawn a
    /// second icon when the app is pinned with "Keep in Dock").
    fn relaunches(&self) -> bool {
        false
    }

    /// Opt-in install-on-quit (`packages/gui-plugins/src/updater/platform.ts`, `opencode#66501`).
    /// Because Rune swaps the signed bundle directly at quit (no Squirrel.Mac), turning the
    /// setting off mid-session always disarms quit-install cleanly.
    fn installs_on_quit(&self) -> bool {
        super::install_on_quit_enabled()
    }

    fn install_on_quit(&self) {
        if let Err(e) = self.apply_inner(true) {
            eprintln!("updater: couldn't install on quit: {e}");
        }
    }

    fn auto_install(&self) -> bool {
        feed::env("OPENCODE_RUNE_AUTO_INSTALL_ON_READY").as_deref() == Some("1")
    }

    fn external(&self) -> bool {
        true
    }

    fn external_install(&self, url: &str) -> Result<(), String> {
        let url = url.trim();
        if !feed::allowed_update_url(url) || !browser_policy::is_safe_os_external_url(url) {
            return Err("Refusing unsafe update URL".into());
        }
        if harness_log::record("openExternal", &[("url", url)]) {
            return Ok(());
        }
        match Command::new("/usr/bin/open").arg(url).status() {
            Ok(s) if s.success() => Ok(()),
            Ok(s) => Err(format!("Could not open the download (open {s})")),
            Err(e) => Err(format!("Could not open the download: {e}")),
        }
    }
}

/// Detached POSIX shell helper spawned by `Mac::spawn_relaunch_helper`.
///
/// Running `/usr/bin/open -n` while the old app is still alive starts a second concurrent
/// instance while the exiting process still owns any pinned ("Keep in Dock") tile, leaving the
/// updated app stuck on a separate transient Dock icon. Waiting for the exiting PID to terminate
/// and clear Launch Services first, then opening the swapped bundle without `-n`, reuses the
/// existing Dock tile.
const RELAUNCH_HELPER_SCRIPT: &str = r#"pid="$1"
bundle="$2"
shift 2
i=0
while /bin/kill -0 "$pid" 2>/dev/null; do
    i=$((i + 1))
    if [ "$i" -ge 600 ]; then
        exit 0
    fi
    /bin/sleep 0.05
done
if [ -x /usr/bin/lsappinfo ]; then
    j=0
    while /usr/bin/lsappinfo find "pid=$pid" 2>/dev/null | /usr/bin/grep -q "ASN:"; do
        j=$((j + 1))
        if [ "$j" -ge 40 ]; then
            break
        fi
        /bin/sleep 0.05
    done
fi
/bin/sleep 0.1
attempt=0
while [ "$attempt" -lt 10 ]; do
    if [ "$#" -gt 0 ]; then
        /usr/bin/open --env "RUNE_RELAUNCH_AFTER=$pid" "$bundle" --args "$@" && exit 0
    else
        /usr/bin/open --env "RUNE_RELAUNCH_AFTER=$pid" "$bundle" && exit 0
    fi
    attempt=$((attempt + 1))
    /bin/sleep 0.15
done
"#;

impl Mac {
    /// The channel's DMG for this Mac (`…/opencode/desktop-rune/dev/macos-arm64-dmg`).
    fn download_url(&self) -> String {
        let facts = &self.engine.facts;
        format!("{}/{}/{}/{}/{}-dmg", facts.service.trim_end_matches('/'), super::protocol::APP_NAME, super::protocol::ARTIFACT_NAME, facts.channel, feed::target_name())
    }

    /// Spawns a detached non-GUI `/bin/sh` helper that waits for this process to exit and leave
    /// Launch Services, then reopens `self.bundle.path` via `/usr/bin/open` without `-n`.
    fn spawn_relaunch_helper(&self) -> Result<(), String> {
        let pid = std::process::id().to_string();
        let args: Vec<OsString> = rune::tooling::without_relaunch_drops(std::env::args_os().skip(1).collect())
            .into_iter()
            .filter(|a| !a.to_str().is_some_and(|s| s.starts_with("-psn_")))
            .collect();
        let mut cmd = Command::new("/bin/sh");
        cmd.args(["-c", RELAUNCH_HELPER_SCRIPT, "opencode-updater-relaunch"])
            .arg(&pid)
            .arg(&self.bundle.path)
            .args(&args)
            .current_dir("/")
            .env_remove("RUNE_RELAUNCH_AFTER")
            .env_remove("RUNE_INSTANCE_HANDED")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0);
        cmd.spawn()
            .map(drop)
            .map_err(|e| format!("could not start the relaunch helper: {e}"))
    }
}

pub fn bundle_root_from_exe(exe: &Path) -> Option<PathBuf> {
    let macos = exe.parent()?;
    if macos.file_name().and_then(|n| n.to_str()) != Some("MacOS") {
        return None;
    }
    let contents = macos.parent()?;
    if contents.file_name().and_then(|n| n.to_str()) != Some("Contents") {
        return None;
    }
    let app = contents.parent()?;
    (app.extension().and_then(|e| e.to_str()) == Some("app")).then(|| app.to_path_buf())
}

fn plist_field(plist: &Path, key: &str) -> Result<String, String> {
    let out = Command::new("/usr/libexec/PlistBuddy").args(["-c", &format!("Print :{key}")]).arg(plist).output().map_err(|e| format!("failed to run PlistBuddy: {e}"))?;
    if !out.status.success() {
        return Err(format!("Info.plist has no {key}"));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

fn team_of(app: &Path) -> Option<String> {
    let out = Command::new("/usr/bin/codesign").args(["--display", "--verbose=4"]).arg(app).output().ok()?;
    let combined = format!("{}\n{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    combined.lines().find_map(|line| {
        let team = line.strip_prefix("TeamIdentifier=")?.trim();
        (team != "not set" && !team.is_empty()).then(|| team.to_owned())
    })
}

pub fn bundle_info(app: &Path) -> Result<BundleInfo, String> {
    let plist = app.join("Contents/Info.plist");
    let id = plist_field(&plist, "CFBundleIdentifier")?;
    let version = plist_field(&plist, "CFBundleShortVersionString")?;
    Ok(BundleInfo { path: app.to_path_buf(), id, version, team: team_of(app) })
}

fn check_replaceable(app: &Path) -> Result<(), String> {
    if app.components().any(|c| c.as_os_str() == "AppTranslocation") {
        return Err(format!("Move {} to Applications so it can update itself.", super::build_info().product_name));
    }
    let parent = app.parent().ok_or_else(|| "App bundle has no parent folder.".to_owned())?;
    let probe = parent.join(format!(".brocode-update-probe-{}", std::process::id()));
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .map(drop)
        .and_then(|()| std::fs::remove_file(&probe))
        .map_err(|_| format!("{} is not writable.", parent.display()))?;
    // Moving the bundle aside renames it, which needs write access to the bundle folder itself
    // (an administrator's copy in /Applications is often root's).
    if !Command::new("/bin/test").arg("-w").arg(app).status().is_ok_and(|s| s.success()) {
        return Err(format!("{} is not writable.", app.display()));
    }
    Ok(())
}

/// macOS checks a new executable file the first time a process is made from it, before that
/// process's `main`: 1,074,000 to 1,097,000 µs for the 179 MB bundles of dev-473 and dev-474 on
/// an M4, and more for a larger bundle. It is once per file, not per signature (a second unpack
/// of the same build pays it again), and `codesign --verify` and `spctl --assess` don't take it.
/// The swap renames the staged files into place, so the staged executable runs once here
/// (`--version` prints and exits before anything else starts), and the version the app
/// restarts into is one macOS has checked: its `main` came 27,273 µs after its start instead of
/// 1,095,137. A run that fails, or is ended after 20 s, changes nothing: the update is staged
/// either way.
fn first_run(exe: &Path) {
    use std::time::{Duration, Instant};
    let Ok(mut child) = Command::new(exe).arg("--version").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() else { return };
    let deadline = Instant::now() + Duration::from_secs(20);
    while matches!(child.try_wait(), Ok(None)) {
        if Instant::now() >= deadline {
            child.kill().ok();
            child.wait().ok();
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn unpack_zip(zip: &Path, into: &Path) -> Result<PathBuf, String> {
    let out = Command::new("/usr/bin/ditto").args(["-x", "-k"]).arg(zip).arg(into).output().map_err(|e| format!("failed to run ditto: {e}"))?;
    if !out.status.success() {
        return Err(format!("could not unpack update archive: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    let entries = std::fs::read_dir(into).map_err(|e| format!("could not read unpacked dir: {e}"))?;
    let mut apps: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.is_dir() && p.extension().and_then(|e| e.to_str()) == Some("app")).collect();
    if apps.len() != 1 {
        return Err(format!("expected 1 .app in update archive, found {}", apps.len()));
    }
    Ok(apps.remove(0))
}

pub fn verify_bundle(app: &Path, expected_id: &str, expected_team: Option<&str>, check_gatekeeper: bool) -> Result<(), String> {
    let info = bundle_info(app)?;
    if info.id != expected_id {
        return Err(format!("update bundle ID {} did not match {expected_id}", info.id));
    }
    let mut codesign = Command::new("/usr/bin/codesign");
    codesign.args(["--verify", "--strict", "--deep"]);
    // On quit (`check_gatekeeper` false) the notarization was already checked at staging, and an
    // online notarization lookup could stall the exit.
    if let Some(team) = expected_team {
        codesign.arg(format!("-R={}", update_requirement(expected_id, team, check_gatekeeper)));
    }
    let cs = codesign.arg(app).output().map_err(|e| format!("failed to run codesign: {e}"))?;
    if !cs.status.success() {
        return Err(format!("the update's code signature is not Anomaly's notarized Developer ID: {}", String::from_utf8_lossy(&cs.stderr).trim()));
    }
    let Some(expected_team) = expected_team else { return Ok(()) };
    if info.team.as_deref() != Some(expected_team) {
        return Err(format!("update signing team {:?} did not match {expected_team}", info.team));
    }
    if check_gatekeeper {
        let gk = Command::new("/usr/sbin/spctl").args(["--assess", "--type", "execute"]).arg(app).output().map_err(|e| format!("failed to run spctl: {e}"))?;
        if !gk.status.success() {
            return Err(format!("Gatekeeper rejected update: {}", String::from_utf8_lossy(&gk.stderr).trim()));
        }
    }
    Ok(())
}

/// Atomically swaps `staged` into `dest`, restoring `dest` if the final move fails.
/// Uses per-process unique sibling names so a leftover file, dangling symlink, or locked directory
/// at legacy `.{name}.update` / `.{name}.previous` never permanently blocks updates (`ENOTDIR`),
/// and never follows or removes symlinks.
fn swap(staged: &Path, dest: &Path) -> Result<(), String> {
    let parent = dest.parent().ok_or_else(|| "app has no parent directory".to_owned())?;
    let name = dest.file_name().ok_or_else(|| "app has no file name".to_owned())?.to_string_lossy();
    let seq = STAGE_SEQ.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let incoming = parent.join(format!(".{name}.update-{pid}-{seq}"));
    let outgoing = parent.join(format!(".{name}.previous-{pid}-{seq}"));
    for path in [&incoming, &outgoing] {
        if std::fs::symlink_metadata(path).is_ok() {
            return Err(format!("unexpected path already exists at {}", path.display()));
        }
    }
    move_bundle(staged, &incoming).map_err(|e| format!("could not stage update in {}: {e}", parent.display()))?;
    if let Err(e) = move_bundle(dest, &outgoing) {
        std::fs::remove_dir_all(&incoming).ok();
        return Err(format!("could not move current app aside: {e}"));
    }
    if let Err(e) = move_bundle(&incoming, dest) {
        move_bundle(&outgoing, dest).ok();
        std::fs::remove_dir_all(&incoming).ok();
        return Err(format!("could not move update into place: {e}"));
    }
    std::fs::remove_dir_all(&outgoing).ok();
    Ok(())
}

fn move_bundle(from: &Path, to: &Path) -> io::Result<()> {
    match std::fs::rename(from, to) {
        Err(e) if e.kind() == io::ErrorKind::CrossesDevices => {
            let st = Command::new("/usr/bin/ditto").arg(from).arg(to).status()?;
            if !st.success() {
                std::fs::remove_dir_all(to).ok();
                return Err(io::Error::other("ditto failed across volumes"));
            }
            std::fs::remove_dir_all(from)
        }
        other => other,
    }
}

fn pid_alive(pid: u32) -> bool {
    if pid == std::process::id() {
        return true;
    }
    match Command::new("/bin/kill").args(["-0", &pid.to_string()]).output() {
        Ok(out) if out.status.success() => true,
        Ok(out) => String::from_utf8_lossy(&out.stderr).contains("Operation not permitted"),
        Err(_) => true,
    }
}

fn is_real_dir(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| !m.file_type().is_symlink() && m.is_dir())
}

fn sweep_old_stages(root: &Path) {
    if !is_real_dir(root) {
        return;
    }
    let Ok(entries) = std::fs::read_dir(root) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some(rest) = name.strip_prefix("stage-") else { continue };
        let live = rest
            .split_once('-')
            .and_then(|(pid, _)| pid.parse::<u32>().ok())
            .is_some_and(pid_alive);
        if !live && is_real_dir(&entry.path()) {
            std::fs::remove_dir_all(entry.path()).ok();
        }
    }
}

/// Removes dead-PID swap leftovers beside `app` (and non-symlink legacy `.update`/`.previous` dirs),
/// never following symlinks and never touching leftovers of a still-running process.
fn sweep_swap_leftovers(app: &Path) {
    let Some(parent) = app.parent() else { return };
    if !is_real_dir(parent) {
        return;
    }
    let Some(name) = app.file_name().and_then(|n| n.to_str()) else { return };
    let Ok(entries) = std::fs::read_dir(parent) else { return };
    let legacy_u = format!(".{name}.update");
    let legacy_p = format!(".{name}.previous");
    let prefix_u = format!(".{name}.update-");
    let prefix_p = format!(".{name}.previous-");
    for entry in entries.flatten() {
        let n = entry.file_name();
        let Some(n) = n.to_str() else { continue };
        let dead = if n == legacy_u || n == legacy_p {
            true
        } else if let Some(rest) = n.strip_prefix(&prefix_u).or_else(|| n.strip_prefix(&prefix_p)) {
            rest.split_once('-').and_then(|(pid, _)| pid.parse::<u32>().ok()).is_some_and(|pid| !pid_alive(pid))
        } else {
            false
        };
        if dead && is_real_dir(&entry.path()) {
            std::fs::remove_dir_all(entry.path()).ok();
        }
    }
}

const INSTALLED_RECEIPT: &str = "installed-bundle.json";

#[derive(Deserialize, Serialize)]
struct InstalledBundle {
    version: String,
    sha256: String,
    dev: u64,
    ino: u64,
}

fn file_id(path: &Path) -> Option<(u64, u64)> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.dev(), meta.ino()))
}

fn read_installed_receipt(updates: &Path) -> Option<InstalledBundle> {
    let text = std::fs::read_to_string(updates.join(INSTALLED_RECEIPT)).ok()?;
    let r: InstalledBundle = serde_json::from_str(&text).ok()?;
    (r.sha256.len() == 64).then_some(r)
}

/// Promotes `Updates/installed-bundle.json` into `rune.installedRelease` when the running
/// executable's `(dev, ino)` matches the receipt written by a post-close `apply`. If neither the
/// running executable nor the current bundle at the install path matches (for example, the user
/// replaced the `.app` from a DMG), removes the stale receipt.
fn sync_installed_receipt(updates: &Path, running_exe: &Path, installed_exe: &Path) {
    let Some(r) = read_installed_receipt(updates) else { return };
    if file_id(running_exe) == Some((r.dev, r.ino)) {
        engine::record_installed(&r.version, &r.sha256);
    } else if file_id(installed_exe) != Some((r.dev, r.ino)) {
        std::fs::remove_file(updates.join(INSTALLED_RECEIPT)).ok();
        if engine::read_installed_sha(&r.version).as_deref() == Some(r.sha256.as_str()) {
            opencode_platform::storage::write_json(engine::NAMESPACE, "rune.installedRelease", &serde_json::Value::Null);
        }
    }
}

fn write_installed_receipt(updates: &Path, placed_exe: &Path, version: &str, sha256: &str) {
    let Some((dev, ino)) = file_id(placed_exe) else { return };
    if std::fs::create_dir_all(updates).is_err() {
        return;
    }
    let tmp = updates.join(format!("installed-bundle.tmp-{}", std::process::id()));
    let Ok(json) = serde_json::to_vec(&InstalledBundle { version: version.to_owned(), sha256: sha256.to_owned(), dev, ino }) else { return };
    if std::fs::write(&tmp, json).is_ok() {
        std::fs::rename(&tmp, updates.join(INSTALLED_RECEIPT)).ok();
    } else {
        std::fs::remove_file(&tmp).ok();
    }
}
