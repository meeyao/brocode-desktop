//! Where a new engine comes from and which engines this build accepts: the two inputs a release
//! sets, [`SOURCE`] and [`TRUST`], and the release this platform picks from the source's index.
//!
//! The index has the release service's manifest shape (`apps/browser-engine/scripts/
//! package-windows.ps1` writes one): `release.version`, and per platform
//! `release.targets.<os>-<arch>` with the archive's `url` (relative to the index, or absolute),
//! `size` and `sha256`.

use std::io::Read;
use std::path::PathBuf;
use std::time::Duration;

use serde_json::Value;

use crate::installer::{Distrust, Fault, Version};

/// The index's URL: `https://`, `http://` (loopback only, unless the trust rule is a publisher or
/// hashes), or `file:`. Set when the app is built (`OPENCODE_BROWSER_ENGINE_SOURCE`).
pub const SOURCE: &str = match option_env!("OPENCODE_BROWSER_ENGINE_SOURCE") {
    Some(s) => s,
    None => DEV_SOURCE,
};
/// Which archives install (`OPENCODE_BROWSER_ENGINE_TRUST`, set when the app is built):
/// - `publisher:<name>`: every program and library in it is signed by `<name>` (Windows: a
///   trusted Authenticode signature whose certificate names it; macOS: the Developer ID team).
/// - `sha256:<hex>[,<hex>...]`: the archive is one of these.
/// - `local`: whatever a loopback or `file:` source lists, checked against the index's size and
///   SHA-256 (a development build serving its own package).
pub const TRUST: &str = match option_env!("OPENCODE_BROWSER_ENGINE_TRUST") {
    Some(t) => t,
    None => DEV_TRUST,
};

#[cfg(any(all(windows, not(debug_assertions)), target_os = "macos", all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))))]
const RELEASE_SOURCE: &str = "https://anoma.ly/release/brocode/browser-engine/stable.json";

/// Windows, a debug build: `bun apps/browser-engine/scripts/serve.ts` serves a local package here.
/// Not 47963, which published builds up to `b22a2521` read: a development engine served for a test
/// must never be one an installed app on the same computer could take.
/// A release build never reads it: on a user's computer nothing serves it (every download failed
/// as "check your connection"), and the `local` rule would install whatever another program on
/// that computer served there.
#[cfg(all(windows, debug_assertions))]
const DEV_SOURCE: &str = "http://127.0.0.1:47981/index.json";
#[cfg(all(windows, debug_assertions))]
const DEV_TRUST: &str = "local";
/// Windows release builds verify every `.exe` and `.dll` in the engine archive against the app's
/// Authenticode signer (`features/updater/src/windows.rs` `EXPECTED_PUBLISHER`).
#[cfg(all(windows, not(debug_assertions)))]
const DEV_SOURCE: &str = RELEASE_SOURCE;
#[cfg(all(windows, not(debug_assertions)))]
const DEV_TRUST: &str = "publisher:Anomaly Innovations, Inc https://anoma.ly/";
/// macOS (arm64 and x64) verifies the engine bundle against the Developer ID team (`SEER_TEAM_ID`).
#[cfg(target_os = "macos")]
const DEV_SOURCE: &str = RELEASE_SOURCE;
#[cfg(target_os = "macos")]
const DEV_TRUST: &str = "publisher:5NZ4Q7NXJ4";
/// Linux x86_64 archives are unsigned; the release workflow pins `OPENCODE_BROWSER_ENGINE_TRUST`
/// to `sha256:<hex>` at build time, and a build without it fails closed as `Fault::NotPublished`.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const DEV_SOURCE: &str = RELEASE_SOURCE;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const DEV_TRUST: &str = "";
/// Linux aarch64 archives are unsigned; the release workflow pins `OPENCODE_BROWSER_ENGINE_TRUST`
/// to `sha256:<hex>` at build time, and a build without it fails closed as `Fault::NotPublished`.
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
const DEV_SOURCE: &str = RELEASE_SOURCE;
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
const DEV_TRUST: &str = "";
/// Targets without a published engine.
#[cfg(not(any(windows, target_os = "macos", all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64")))))]
const DEV_SOURCE: &str = "";
#[cfg(not(any(windows, target_os = "macos", all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64")))))]
const DEV_TRUST: &str = "";

/// This platform's target in the index.
const TARGET: &str = if cfg!(all(windows, target_arch = "aarch64")) {
    "windows-arm64"
} else if cfg!(windows) {
    "windows-x64"
} else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
    "macos-arm64"
} else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
    "macos-x64"
} else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
    "linux-x64"
} else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
    "linux-arm64"
} else {
    "unsupported"
};

/// How an archive earns its install.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Trust {
    Local,
    Sha256(Vec<String>),
    Publisher(String),
}

/// The release the index offers this platform.
#[derive(Clone, Debug)]
pub(crate) struct Release {
    pub version: Version,
    pub url: String,
    pub size: u64,
    pub sha256: String,
    pub trust: Trust,
}

/// The source and trust rule in force: the build's, or in a development build
/// `OPENCODE_BROWSER_ENGINE_SOURCE` and `OPENCODE_BROWSER_ENGINE_TRUST` from the environment (a
/// harness's stand-in on another port).
fn inputs() -> (String, String) {
    let runtime = |name: &str| cfg!(debug_assertions).then(|| std::env::var_os(name)).flatten().and_then(|v| v.into_string().ok());
    (
        runtime("OPENCODE_BROWSER_ENGINE_SOURCE").unwrap_or_else(|| SOURCE.to_owned()),
        runtime("OPENCODE_BROWSER_ENGINE_TRUST").unwrap_or_else(|| TRUST.to_owned()),
    )
}

/// Whether this build has both a source and a trust rule to download an engine (else the pane
/// says the browser isn't available). A platform no engine is built for (`TARGET`
/// "unsupported") has none.
pub(crate) fn configured() -> bool {
    let (source, trust) = inputs();
    TARGET != "unsupported" && !source.trim().is_empty() && !trust.trim().is_empty()
}

/// Whether the source is on this machine (a development build's stand-in, or a `file:` index).
pub(crate) fn local() -> bool {
    is_local(inputs().0.trim())
}

/// Whether this build has a valid trust rule for an installed engine.
pub(crate) fn has_trust() -> bool {
    TARGET != "unsupported" && parse_trust(&inputs().1).is_some()
}

/// Reads the index and picks this platform's release: one the build accepts (its engine
/// revision) from a source its trust rule allows.
pub(crate) fn latest() -> Result<Release, Fault> {
    let (source, trust) = inputs();
    let source = source.trim();
    if source.is_empty() || trust.trim().is_empty() {
        return Err(Fault::NotPublished);
    }
    let trust = parse_trust(&trust).ok_or(Fault::Untrusted(Distrust::Rule))?;
    if trust == Trust::Local && !is_local(source) {
        return Err(Fault::Untrusted(Distrust::Rule));
    }
    // Not JSON: something between here and the source answered in its place (a network's
    // sign-in page, a filter).
    let index: Value = serde_json::from_slice(&fetch_small(source)?).map_err(|_| Fault::Unexpected { host: Fault::host(source) })?;
    let release = &index["release"];
    let version = release["version"].as_str().and_then(Version::parse).ok_or(Fault::NotPublished)?;
    let target = &release["targets"][TARGET];
    let (Some(url), Some(size), Some(sha256)) = (target["url"].as_str(), target["size"].as_u64(), target["sha256"].as_str()) else {
        return Err(Fault::NotPublished);
    };
    if !version.accepted() {
        return Err(Fault::NotPublished);
    }
    let sha256 = sha256.to_ascii_lowercase();
    // Also used in the on-disk partial filename. Validate before treating it as a path component.
    if sha256.len() != 64 || !sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Fault::Untrusted(Distrust::Rule));
    }
    if let Trust::Sha256(allowed) = &trust
        && !allowed.contains(&sha256)
    {
        return Err(Fault::Untrusted(Distrust::Rule));
    }
    Ok(Release { version, url: resolve(source, url), size, sha256, trust })
}

/// What an install records of the rule that accepted it (`installer::TRUST_MARKER`): the
/// archive's hash, the publisher, or `local`.
pub(crate) fn marker(trust: &Trust, sha256: &str) -> String {
    match trust {
        Trust::Local => "local".to_owned(),
        Trust::Sha256(_) => format!("sha256:{sha256}"),
        Trust::Publisher(name) => format!("publisher:{name}"),
    }
}

/// Whether this build starts the engine installed in `version_dir`: one its own rule would have
/// installed. A development build (rule `local`) starts what it finds. An install without a
/// record came from a build before records: on macOS every such build pinned the published
/// archive's hash, elsewhere it was the loopback stand-in, which a release build never trusts.
pub(crate) fn accepts_install(version_dir: &std::path::Path) -> bool {
    let Some(trust) = parse_trust(&inputs().1) else { return false };
    let marker = std::fs::read_to_string(version_dir.join(crate::installer::TRUST_MARKER)).ok();
    match (marker.as_deref().map(str::trim), &trust) {
        (_, Trust::Local) => true,
        (None, Trust::Sha256(_)) => cfg!(target_os = "macos"),
        (None, Trust::Publisher(_)) => false,
        (Some(m), Trust::Sha256(allowed)) => m.strip_prefix("sha256:").is_some_and(|h| allowed.iter().any(|a| a == h)),
        (Some(m), Trust::Publisher(name)) => m.strip_prefix("publisher:") == Some(name.as_str()),
    }
}

fn parse_trust(rule: &str) -> Option<Trust> {
    let rule = rule.trim();
    if rule == "local" {
        return Some(Trust::Local);
    }
    if let Some(hashes) = rule.strip_prefix("sha256:") {
        let list: Vec<String> = hashes.split(',').map(|h| h.trim().to_ascii_lowercase()).filter(|h| h.len() == 64).collect();
        return (!list.is_empty()).then_some(Trust::Sha256(list));
    }
    rule.strip_prefix("publisher:").map(str::trim).filter(|p| !p.is_empty()).map(|p| Trust::Publisher(p.to_owned()))
}

/// A `file:` source, or `http://` on this machine.
fn is_local(url: &str) -> bool {
    if url.starts_with("file:") {
        return true;
    }
    let Some(rest) = url.strip_prefix("http://") else { return false };
    let host = rest.split(['/', '?']).next().unwrap_or_default();
    let host = host.rsplit_once(':').filter(|(h, port)| !h.is_empty() && port.bytes().all(|b| b.is_ascii_digit())).map_or(host, |(h, _)| h);
    matches!(host, "127.0.0.1" | "localhost" | "[::1]")
}

/// `url` as the index names it, against the index's own URL.
fn resolve(index: &str, url: &str) -> String {
    if url.contains("://") || url.starts_with("file:") {
        return url.to_owned();
    }
    if let Some(path) = url.strip_prefix('/') {
        let origin_end = index.find("://").map_or(0, |i| i + 3);
        let origin = index[origin_end..].find('/').map_or(index, |slash| &index[..origin_end + slash]);
        return format!("{origin}/{path}");
    }
    let dir = index.rfind('/').map_or(index, |slash| &index[..=slash]);
    format!("{dir}{url}")
}

/// A `file:` URL's path.
pub(crate) fn file_path(url: &str) -> Option<PathBuf> {
    let rest = url.strip_prefix("file:")?;
    let rest = rest.strip_prefix("//").unwrap_or(rest);
    // `file:///C:/x` on Windows.
    let rest = if cfg!(windows) { rest.strip_prefix('/').filter(|r| r.as_bytes().get(1) == Some(&b':')).unwrap_or(rest) } else { rest };
    Some(PathBuf::from(percent_decode(rest)))
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        if bytes[i] == b'%'
            && let (Some(h), Some(l)) = (bytes.get(i + 1).copied().and_then(hex), bytes.get(i + 2).copied().and_then(hex))
        {
            out.push((h * 16 + l) as u8);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The index, whole (it's a few hundred bytes; at most 1 MB is read).
fn fetch_small(url: &str) -> Result<Vec<u8>, Fault> {
    let mut body = Vec::new();
    if let Some(path) = file_path(url) {
        std::fs::File::open(path).and_then(|f| f.take(1 << 20).read_to_end(&mut body)).map_err(|_| Fault::NotPublished)?;
        return Ok(body);
    }
    let agent = crate::download::agent(Duration::from_secs(15));
    let mut resp = agent.get(url).call().map_err(|e| crate::download::request_fault(url, e))?;
    resp.body_mut().as_reader().take(1 << 20).read_to_end(&mut body).map_err(|e| crate::download::io_fault(url, &e))?;
    Ok(body)
}
