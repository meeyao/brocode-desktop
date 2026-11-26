//! Embeds the app (`OPENCODE_RUNE_SETUP_PAYLOAD`: the release zip of the install folder, the
//! same archive the updater downloads) and, on Windows, the setup's icon, version and manifest.
//! Without a payload the setup builds, and says it has nothing to install.

fn main() {
    for name in ["OPENCODE_RUNE_SETUP_PAYLOAD", "OPENCODE_RUNE_WINDOW_TITLE", "OPENCODE_RUNE_VERSION", "OPENCODE_RUNE_BUNDLE_ID", "OPENCODE_RUNE_BUILD_CHANNEL", "OPENCODE_RUNE_URL_SCHEME"] {
        println!("cargo:rerun-if-env-changed={name}");
    }
    #[allow(clippy::disallowed_methods)]
    let env = |name: &str| std::env::var(name).ok().filter(|v| !v.is_empty());
    let out = std::path::PathBuf::from(env("OUT_DIR").unwrap_or_default()).join("payload.zip");
    let payload = env("OPENCODE_RUNE_SETUP_PAYLOAD");
    if let Some(payload) = &payload {
        println!("cargo:rerun-if-changed={payload}");
    }
    let bytes = match &payload {
        Some(path) => std::fs::read(path).unwrap_or_else(|e| panic!("OPENCODE_RUNE_SETUP_PAYLOAD {path}: {e}")),
        None => Vec::new(),
    };
    if std::fs::read(&out).ok().as_deref() != Some(bytes.as_slice()) {
        std::fs::write(&out, &bytes).unwrap_or_else(|e| panic!("{}: {e}", out.display()));
    }
    if env("CARGO_CFG_TARGET_OS").as_deref() != Some("windows") {
        return;
    }
    let title = env("OPENCODE_RUNE_WINDOW_TITLE").unwrap_or_else(|| win_install::default_name(env("OPENCODE_RUNE_BUILD_CHANNEL").as_deref()).into());
    let version = env("OPENCODE_RUNE_VERSION").or_else(|| env("CARGO_PKG_VERSION")).unwrap_or_default();
    let dir = std::path::PathBuf::from(env("CARGO_MANIFEST_DIR").unwrap_or_default());
    let (icon, manifest) = (dir.join("../desktop/assets/icon.ico"), dir.join("setup.manifest"));
    let description = format!("{title} Setup");
    let resources = win_install::resources::Resources::new(&icon, &manifest, "brocode-setup.exe", &version).names(&description, &title);
    if let Err(e) = win_install::resources::embed(&resources) {
        panic!("{e}");
    }
}
