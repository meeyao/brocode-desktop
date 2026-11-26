//! Linux: checks the release feed and offers the direct package download link, never elevating
//! or replacing packages in-app.
//!
//! - **AppImage**, **`.deb`**, **`.rpm`**: checks `https://anoma.ly/release/opencode/desktop-rune/<channel>.json`
//!   for `linux-<arch>-<format>` and reports `Target::External` (`download-required`), opening the
//!   channel's HTTPS download URL for the matching format.
//! - **Snap, Flatpak, distro/bare binaries**: returns `None` (`Disabled`).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::engine::{Engine, Facts};
use super::feed;
use super::machine::Target;
use super::platform::Platform;
use super::protocol::{APP_NAME, ARTIFACT_NAME, DEFAULT_SERVICE_URL};

/// What the `.deb` and `.rpm` install as (`apps/desktop/scripts/package-linux.sh`).
const PACKAGE: &str = "brocode";
const PACKAGED_EXE: &str = "/usr/bin/brocode";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Format {
    AppImage,
    Deb,
    Rpm,
}

impl Format {
    fn suffix(self) -> &'static str {
        match self {
            Format::AppImage => "appimage",
            Format::Deb => "deb",
            Format::Rpm => "rpm",
        }
    }
}

pub struct Linux {
    engine: Engine,
    version: String,
    /// The release target this install follows (`linux-x64-appimage`, `linux-x64-deb`, ...).
    target: String,
}

/// The updater for an AppImage, `.deb` or `.rpm` install on a release channel (or with a debug
/// feed override). Snap, Flatpak, distro packages and bare binaries return `None` (`Disabled`).
pub fn platform(exe: &Path, version: &str, build_channel: &str, updates: PathBuf) -> Option<Linux> {
    if feed::env("SNAP").is_some() || feed::env("FLATPAK_ID").is_some() || Path::new("/.flatpak-info").exists() {
        eprintln!("updater: linux packaged under Snap/Flatpak; updater disabled");
        return None;
    }
    let service = match feed::debug_release_url() {
        Some(url) => url,
        None if build_channel != "local" => DEFAULT_SERVICE_URL.to_owned(),
        None => return None,
    };
    if !feed::allowed_update_url(&service) {
        eprintln!("updater: refusing insecure linux feed URL: {service}");
        return None;
    }
    let format = detect_format(exe)?;
    let target = format!("{}-{}", feed::target_name(), format.suffix());
    eprintln!("updater: linux {format:?} install, following {target} (download link)");
    sweep_old_stages(&updates);
    let facts = Facts { version: version.to_owned(), channel: feed::feed_channel(), service, updates };
    Some(Linux { engine: Engine::new(facts), version: version.to_owned(), target })
}

fn detect_format(exe: &Path) -> Option<Format> {
    if let Some(raw) = feed::env("APPIMAGE") {
        let path = PathBuf::from(&raw);
        if path.is_absolute() && !raw.contains('\0') && path.is_file() {
            return Some(Format::AppImage);
        }
        eprintln!("updater: ignoring invalid APPIMAGE path: {raw:?}");
        return None;
    }
    let canonical = exe.canonicalize().ok();
    let is_packaged = exe == Path::new(PACKAGED_EXE) || canonical.as_deref() == Some(Path::new(PACKAGED_EXE));
    if !is_packaged {
        return None;
    }
    if Path::new(&format!("/var/lib/dpkg/info/{PACKAGE}.list")).is_file() {
        return Some(Format::Deb);
    }
    let rpm_bin = Path::new("/usr/bin/rpm");
    if rpm_bin.is_file() {
        let rpm = Command::new(rpm_bin)
            .args(["-q", "--quiet", PACKAGE])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if rpm.is_ok_and(|s| s.success()) {
            return Some(Format::Rpm);
        }
    }
    None
}

impl Linux {
    /// The channel's direct download link for this install's target.
    fn latest(&self) -> String {
        let facts = &self.engine.facts;
        format!("{}/{APP_NAME}/{ARTIFACT_NAME}/{}/{}", facts.service.trim_end_matches('/'), facts.channel, self.target)
    }
}

impl Platform for Linux {
    fn version(&self) -> &str {
        &self.version
    }

    fn check(&self) -> Result<Option<Target>, String> {
        Ok(self.engine.check_target(&self.target)?.map(|found| Target::External { version: found.version, url: self.latest() }))
    }

    fn stage(&self, _progress: &dyn Fn(u64, u64)) -> Result<(), String> {
        Err("Linux builds update from a download".into())
    }

    fn install(&self) -> Result<(), String> {
        Err("Linux builds update from a download".into())
    }

    fn external(&self) -> bool {
        true
    }

    fn external_install(&self, url: &str) -> Result<(), String> {
        let trimmed = url.trim();
        if !feed::allowed_update_url(trimmed) || !browser_policy::is_safe_os_external_url(trimmed) {
            return Err("Refusing unsafe update URL".into());
        }
        if harness_log::record("openExternal", &[("url", trimmed)]) {
            return Ok(());
        }
        let opener = ["/usr/bin/xdg-open", "/bin/xdg-open"].into_iter().find(|p| Path::new(p).is_file()).unwrap_or("xdg-open");
        let status = Command::new(opener).arg(trimmed).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status();
        match status {
            Ok(s) if s.success() => Ok(()),
            Ok(s) => Err(format!("Could not open the download (xdg-open {s})")),
            Err(e) => Err(format!("Could not open the download: {e}")),
        }
    }
}

fn sweep_old_stages(root: &Path) {
    let Ok(entries) = fs::read_dir(root) else { return };
    for entry in entries.flatten() {
        if entry.file_name().to_string_lossy().starts_with("stage-") {
            fs::remove_dir_all(entry.path()).ok();
        }
    }
}
