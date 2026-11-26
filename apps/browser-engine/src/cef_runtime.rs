//! CEF 156 runtime initialization, browser command-line sanitization, and global
//! `RequestContextHandler` attachment (`DefaultRequestContextHandler`).

use std::sync::mpsc::Sender;

use cef::rc::Rc as _;
use cef::{
    App, BrowserProcessHandler, CefString, CommandLine, ImplApp, ImplBrowserProcessHandler,
    ImplCommandLine, RequestContextHandler, SchemeRegistrar, Settings, WrapApp,
    WrapBrowserProcessHandler, sys, wrap_app, wrap_browser_process_handler,
};

use crate::profile::Profile;

pub struct CefRuntime {
    profile: Profile,
    stub_root: Option<std::path::PathBuf>,
    stub_lock: Option<std::fs::File>,
    #[cfg(target_os = "macos")]
    _loader: cef::library_loader::LibraryLoader,
}

impl CefRuntime {
    /// `adapter_luid`: the app's GPU (`HighPart`, `LowPart`), for Chromium to render on;
    /// `locale`: the app's language as Chromium names it; `pump`: where CEF's requests for its
    /// next work call go.
    pub fn initialize(
        profile: Profile,
        adapter_luid: Option<(i32, u32)>,
        locale: Option<&str>,
        ext_dirs: &[std::path::PathBuf],
        stub_root: Option<std::path::PathBuf>,
        stub_lock: Option<std::fs::File>,
        io_pipes: Option<String>,
        pump: Sender<crate::Event>,
        windows_sandbox_info: *mut std::ffi::c_void,
        main_args_override: Option<cef::MainArgs>,
    ) -> Self {
        #[cfg(windows)]
        assert!(
            !windows_sandbox_info.is_null(),
            "Windows browser engine requires sandbox_info from CEF bootstrap.exe (RunWinMain)"
        );
        #[cfg(not(windows))]
        let _ = (windows_sandbox_info, main_args_override);

        #[cfg(target_os = "macos")]
        crate::mac_app::ensure_ns_application();

        #[cfg(target_os = "macos")]
        let loader = {
            let exe = std::env::current_exe().expect("current_exe");
            let loader = cef::library_loader::LibraryLoader::new(&exe, false);
            assert!(loader.load(), "failed to load Chromium Embedded Framework");
            loader
        };

        let _ = cef::api_hash(sys::CEF_API_VERSION_LAST, 0);

        #[cfg(unix)]
        let exe_str = std::env::current_exe()
            .ok()
            .and_then(|p| p.into_os_string().into_string().ok())
            .unwrap_or_else(|| "opencode-browser-engine".into());
        #[cfg(unix)]
        let c_exe = std::ffi::CString::new(exe_str).unwrap();
        #[cfg(unix)]
        let argv = [c_exe.as_ptr()];
        #[cfg(unix)]
        let main_args = cef::MainArgs { argc: 1, argv: argv.as_ptr() as *mut *mut _ };
        #[cfg(windows)]
        let main_args = main_args_override.unwrap_or_else(|| cef::args::Args::new().as_main_args().clone());

        // Windows sandbox requires browser and subprocesses to share one executable (`bootstrap.exe`);
        // macOS finds its helper apps by bundle convention; Linux uses `opencode-browser-engine-helper`.
        #[cfg(target_os = "linux")]
        let browser_subprocess_path = std::env::current_exe()
            .ok()
            .map(|exe| exe.with_file_name(format!("opencode-browser-engine-helper{}", std::env::consts::EXE_SUFFIX)))
            .map(|helper| CefString::from(helper.to_string_lossy().as_ref()))
            .unwrap_or_default();
        #[cfg(not(target_os = "linux"))]
        let browser_subprocess_path = CefString::default();

        let ext_except = (!ext_dirs.is_empty()).then(|| {
            ext_dirs
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join(",")
        });
        let mut app = SidecarCefApp::new(
            adapter_luid.map(|(high, low)| format!("{high},{low}")),
            ext_except,
            io_pipes,
            pump,
        );
        let settings = Settings {
            browser_subprocess_path,
            windowless_rendering_enabled: 1,
            external_message_pump: 1,
            // Linux keeps revision 8's unsandboxed setup until a sandboxed Linux engine has run live.
            no_sandbox: if cfg!(target_os = "linux") { 1 } else { 0 },
            root_cache_path: CefString::from(profile.root.to_string_lossy().as_ref()),
            cache_path: CefString::from(profile.cache.to_string_lossy().as_ref()),
            persist_session_cookies: 1,
            log_severity: sys::cef_log_severity_t::LOGSEVERITY_WARNING.into(),
            // In the profile, not beside the executable (CEF's default), whose folder is the install.
            log_file: CefString::from(profile.root.join("debug.log").to_string_lossy().as_ref()),
            // Without one, Chromium follows the system's language. Pages hear the same language
            // first (Accept-Language, navigator.languages), then English.
            locale: locale.map(CefString::from).unwrap_or_default(),
            accept_language_list: locale
                .map(|l| if l.starts_with("en") { format!("{l},en") } else { format!("{l},en-US,en") })
                .map(|list| CefString::from(list.as_str()))
                .unwrap_or_default(),
            // Dedicated macOS Keychain item name (CEF PR #4247) so OSCrypt creates and owns its
            // own item under our Developer ID identity instead of colliding with `"Chromium Safe Storage"`.
            #[cfg(target_os = "macos")]
            keychain_service_name: CefString::from("Brocode Browser Safe Storage"),
            #[cfg(target_os = "macos")]
            keychain_account_name: CefString::from("Brocode Browser"),
            ..Default::default()
        };
        #[cfg(windows)]
        let ok = cef::initialize(Some(&main_args), Some(&settings), Some(&mut app), windows_sandbox_info.cast());
        #[cfg(not(windows))]
        let ok = cef::initialize(Some(&main_args), Some(&settings), Some(&mut app), std::ptr::null_mut());
        assert_eq!(ok, 1, "cef::initialize failed");
        // A stated platform state for this revision, written once where anyone looks.
        #[cfg(target_os = "linux")]
        {
            use std::io::Write as _;
            eprintln!("browser engine: sandbox: off on Linux in this revision");
            if let Ok(mut log) = std::fs::OpenOptions::new().create(true).append(true).open(profile.root.join("debug.log")) {
                let _ = writeln!(log, "browser engine: sandbox: off on Linux in this revision");
            }
        }
        if let Some(global) = cef::request_context_get_global_context() {
            crate::preview::handler::ban_file_scheme(&global);
        }

        Self {
            profile,
            stub_root,
            stub_lock,
            #[cfg(target_os = "macos")]
            _loader: loader,
        }
    }

    /// The profile and sibling isolated-downloads roots for `Downloads::open`.
    pub fn download_roots(&self) -> crate::downloads::DownloadRoots {
        crate::downloads::DownloadRoots {
            profile: self.profile.root.clone(),
            lasting_profile: self.profile.lasting_root.clone(),
            isolated_base: self.profile.isolated_downloads_base.clone(),
        }
    }
}

impl Drop for CefRuntime {
    fn drop(&mut self) {
        cef::shutdown();
        // Release the lock and remove `<stub_root>` (including `_rt/<ext_id>-<seq>` mirrors)
        // strictly AFTER `cef::shutdown()` so Windows has closed every Chromium handle inside it.
        drop(self.stub_lock.take());
        if let Some(root) = self.stub_root.take() {
            crate::extensions::integrity::cleanup_stub_root(&root);
        }
        // The pages' downloads and staged agent uploads end with the engine.
        crate::downloads::remove_owned(&self.profile.root);
        crate::downloads::remove_owned_isolated(&self.profile.isolated_downloads_base, std::process::id());
        crate::agent::remove_owned_uploads(&self.profile.root);
        if self.profile.temporary {
            let _ = std::fs::remove_dir_all(&self.profile.root);
        }
    }
}

wrap_app! {
    struct SidecarCefApp {
        // Chromium's `use-adapter-luid` (`HIGH,LOW`): the GPU the app's frames open on.
        adapter_luid: Option<String>,
        // Managed user-approved extension directories (`--disable-extensions-except`), or None to
        // pass `--disable-extensions`. Never allows host registry or external loaders.
        ext_except: Option<String>,
        // Windows only: anonymous non-inheritable pipe HANDLE pair for `--remote-debugging-pipe`.
        io_pipes: Option<String>,
        pump: Sender<crate::Event>,
    }
    impl App {
        fn on_before_command_line_processing(
            &self,
            process_type: Option<&cef::CefStringUtf16>,
            command_line: Option<&mut CommandLine>,
        ) {
            let Some(cmd) = command_line else { return };
            // Strip untrusted external switches on the browser process (`GetCommandLineW()` passes
            // through on Windows; `review-rd274.md` §7) before appending our controlled flags.
            let is_browser_process = process_type.is_none_or(|p| p.to_string().is_empty());
            if is_browser_process {
                for sw in [
                    "remote-debugging-port",
                    "remote-debugging-pipe",
                    "remote-debugging-io-pipes",
                    "load-extension",
                    "load-component-extension",
                    "disable-extensions-except",
                    "disable-extensions",
                    "disable-web-security",
                    "single-process",
                    "in-process-gpu",
                    "gpu-launcher",
                    "allow-file-access-from-files",
                    "user-data-dir",
                    "disable-site-isolation-trials",
                    "proxy-server",
                    "proxy-pac-url",
                    "proxy-bypass-list",
                    "host-resolver-rules",
                    "ignore-certificate-errors",
                    "use-mock-keychain",
                    "password-store",
                    "unsafely-treat-insecure-origin-as-secure",
                    "js-flags",
                    "renderer-cmd-prefix",
                    "utility-cmd-prefix",
                    "force-webrtc-ip-handling-policy",
                    "extensions-on-chrome-urls",
                    // Would turn back on what `DISABLED_NET_FEATURES` and the component-updater
                    // source below turn off (READER-7 #624-S1).
                    "component-updater",
                    "enable-features",
                    "force-fieldtrials",
                    "force-fieldtrial-params",
                    "variations-server-url",
                ] {
                    cmd.remove_switch(Some(&sw.into()));
                }
                // Linux leaves these alone, as revision 8 did: with `no_sandbox` set, CEF itself puts
                // `--no-sandbox` and `--browser-subprocess-path` (the helper) here before this runs.
                #[cfg(not(target_os = "linux"))]
                for sw in [
                    "no-sandbox",
                    "no-sandbox-and-elevated",
                    "disable-gpu-sandbox",
                    "disable-seccomp-filter-sandbox",
                    "disable-namespace-sandbox",
                    "disable-setuid-sandbox",
                    "no-zygote",
                    "browser-subprocess-path",
                    "allow-no-sandbox-job",
                    "disable-win32k-lockdown",
                    "disable-renderer-code-integrity",
                ] {
                    cmd.remove_switch(Some(&sw.into()));
                }
            }
            // Rune owns the window. CEF paints offscreen even when no display server is running.
            #[cfg(target_os = "linux")]
            crate::linux_gpu::configure(cmd);
            // Keep Chrome's host registry, component background extensions, & external loaders off:
            // with 0 user-approved extensions pass `--disable-extensions`; with >= 1 pass
            // `--disable-extensions-except=<inert_stub_dirs>` plus our private in-process CDP pipe.
            cmd.append_switch(Some(&"disable-component-extensions-with-background-pages".into()));
            if let Some(dirs) = &self.ext_except {
                cmd.append_switch_with_value(Some(&"disable-extensions-except".into()), Some(&dirs.as_str().into()));
                cmd.append_switch(Some(&"remote-debugging-pipe".into()));
                if let Some(pipes) = &self.io_pipes {
                    cmd.append_switch_with_value(Some(&"remote-debugging-io-pipes".into()), Some(&pipes.as_str().into()));
                }
            } else {
                cmd.append_switch(Some(&"disable-extensions".into()));
            }
            // Let `TabRequestHandler::auth_credentials` handle HTTP 401 challenges in OSR instead
            // of Chrome's native login dialog (which fails with `ERR_INVALID_AUTH_CREDENTIALS` on
            // windowless browsers; CEF issue #3603).
            cmd.append_switch(Some(&"disable-chrome-login-prompt".into()));
            // Route script-initiated pop-ups through `TabLifeSpanHandler::on_before_popup` (which
            // always returns `1` to cancel native windows and holds `user_gesture == 0` in the
            // host's "Pop-up blocked" bar) instead of silently swallowing them before CEF sees them.
            cmd.append_switch(Some(&"disable-popup-blocking".into()));
            // Windows keeps real DPAPI cookie encryption (silent, logon-bound).
            // macOS uses real Keychain encryption under `"Brocode Browser Safe Storage"` (CEF PR #4247)
            // on valid Developer ID builds from Anomaly's team whose keychain is unlocked
            // (`mac_app::decide_cookie_key`), while keeping `--use-mock-keychain` for every other
            // signature, `OPENCODE_BROWSER_MOCK_KEYCHAIN=1` runs and a locked or missing keychain,
            // so no run ever prompts; `Profile::open` keeps mock-key runs out of a Keychain profile.
            // Linux stays on `--password-store=basic` because Secret Service / KWallet can prompt
            // or stall when a keyring is locked or absent on desktop/headless Linux.
            #[cfg(target_os = "macos")]
            if crate::mac_app::cookie_key() == crate::mac_app::CookieKey::Mock {
                cmd.append_switch(Some(&"use-mock-keychain".into()));
            }
            #[cfg(target_os = "linux")]
            cmd.append_switch_with_value(Some(&"password-store".into()), Some(&"basic".into()));
            cmd.append_switch(Some(&"no-startup-window".into()));
            cmd.append_switch(Some(&"noerrdialogs".into()));
            cmd.append_switch(Some(&"hide-crash-restore-bubble".into()));
            // A side panel's pages, as Desktop's Electron shows them: no component downloads
            // (Widevine, Safe Browsing lists, on-device models), no field-trial or other
            // background fetches, which cost network, disk and CPU while nothing is open.
            cmd.append_switch(Some(&"disable-component-update".into()));
            cmd.append_switch(Some(&"disable-background-networking".into()));
            // Chrome services official's Electron does not contain (READER-7, `review-rd274.md`):
            // Safe Browsing (N1), sync (N3), the testing field-trial config and variations seed
            // that turn experimental features on at every boot of a non-official build (N6b).
            // The component updater still asked `update.googleapis.com` for the on-device model
            // manifest (`ceofaddefefcbblgcgnibnonglccbfja`) and fetched it from `edgedl.me.gvt1.com`
            // despite `--disable-component-update` (Playwright #42910): its update source is moved
            // to a closed loopback port, so it never leaves the machine.
            cmd.append_switch_with_value(
                Some(&"component-updater".into()),
                Some(&"url-source=http://127.0.0.1:9/".into()),
            );
            for sw in [
                "disable-client-side-phishing-detection",
                "safebrowsing-disable-auto-update",
                "safebrowsing-disable-download-protection",
                "disable-sync",
                "disable-field-trial-config",
                "disable-variations-seed-fetch",
            ] {
                cmd.append_switch(Some(&sw.into()));
            }
            // Pages play sound without a click first, as in official's Electron (its default
            // `autoplayPolicy`; official sets none): Chrome's own policy refused even muted play.
            cmd.append_switch_with_value(
                Some(&"autoplay-policy".into()),
                Some(&"no-user-gesture-required".into()),
            );
            // Background fetch and periodic background sync, which official's permission check
            // handler denies (N12); their content settings crash CEF 156 when set, so the APIs go.
            // Web Share too: official's Electron has no `navigator.share`.
            cmd.append_switch_with_value(
                Some(&"disable-blink-features".into()),
                Some(&"BackgroundFetch,PeriodicBackgroundSync,WebShare".into()),
            );
            // Disable Cast/DIAL discovery (UDP 5353/1900), mDNS local-IP responder (UDP 5353),
            // and `ConnectionAllowlists` (Spotify CEF 156.0.26 ships with `dcheck_always_on`,
            // where `services/network/network_context.cc:3616` hits `DCHECK(matcher.has_value())`
            // `0x80000003` in the NetworkService helper when a response's `Connection-Allowlist`
            // pattern fails `SimpleUrlPatternMatcher::Create`; previews stay confined by
            // `TabRequestHandler`, strict CSP, isolated contexts, and `disable_non_proxied_udp`).
            // Also enforce `disable_non_proxied_udp` globally so `NetworkService` never opens
            // non-loopback UDP listeners (avoiding Windows Defender Firewall and macOS incoming
            // connection prompts) and never exposes local LAN IPs to pages.
            // `AutofillServerCommunication`, `NetworkTimeServiceQuerying`, `AimEnabled`: the Chrome
            // runtime still sent each visited page's form signatures to
            // `content-autofill.googleapis.com`, polled `clients2.google.com/time`, and fetched
            // `www.google.com/async/folae` (NetLog, 21 Nov); Electron (official) makes none of these.
            // The rest: services the full Chrome runtime brings that official's Electron does not
            // contain at all (READER-7's non-extension audit, `notes/review-rd274.md`), so ours is
            // off as theirs is: Safe Browsing lookups (N1), FedCM's delegation and multi-IdP
            // extras (N2), password manager and account storage (N3), the Privacy Sandbox APIs and
            // fenced frames (N6a), payments (N7), Translate and on-device language/model services
            // (N11). FedCM itself stays: with its content setting blocked
            // (`permissions::CHROME_SERVICE_SETTINGS`) a page gets `NetworkError` before any
            // identity-provider fetch, as on official, where Electron has no FedCM delegate. The
            // built-in AI APIs (`LanguageModel`, `Summarizer`, `Writer`, `Rewriter`, `Proofreader`):
            // a page saw the first two; official has none.
            const DISABLED_NET_FEATURES: [&str; 35] = [
                "MediaRouter",
                "DialMediaRouteProvider",
                "CastMediaRouteProvider",
                "WebRtcHideLocalIpsWithMdns",
                "ConnectionAllowlists",
                "AutofillServerCommunication",
                "NetworkTimeServiceQuerying",
                "AimEnabled",
                "SafeBrowsingHashPrefixRealTimeLookups",
                "ExtensionTelemetry",
                "PasswordLeakDetection",
                "AutofillEnableAccountWalletStorage",
                "BrowsingTopics",
                "Fledge",
                "AdInterestGroupAPI",
                "ConversionMeasurement",
                "SharedStorageAPI",
                "PrivateStateTokens",
                "WebPayments",
                "ServiceWorkerPaymentApps",
                "TranslationAPI",
                "LanguageDetectionAPI",
                "OptimizationHints",
                "OptimizationHintsFetching",
                "OnDeviceModelService",
                "OptimizationGuideModelExecution",
                "Translate",
                "FencedFrames",
                "FedCmDelegation",
                "FedCmMultipleIdentityProviders",
                "AIPromptAPI",
                "AISummarizationAPI",
                "AIWriterAPI",
                "AIRewriterAPI",
                "AIProofreadingAPI",
            ];
            let existing = CefString::from(&cmd.switch_value(Some(&"disable-features".into()))).to_string();
            cmd.remove_switch(Some(&"disable-features".into()));
            let mut features: Vec<&str> = existing
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect();
            for feat in DISABLED_NET_FEATURES {
                if !features.contains(&feat) {
                    features.push(feat);
                }
            }
            let merged = features.join(",");
            cmd.append_switch_with_value(Some(&"disable-features".into()), Some(&merged.as_str().into()));
            cmd.append_switch_with_value(
                Some(&"force-webrtc-ip-handling-policy".into()),
                Some(&"disable_non_proxied_udp".into()),
            );
            if let Some(luid) = &self.adapter_luid {
                cmd.append_switch_with_value(Some(&"use-adapter-luid".into()), Some(&luid.as_str().into()));
            }
        }

        fn on_register_custom_schemes(&self, registrar: Option<&mut SchemeRegistrar>) {
            crate::preview::scheme::register(registrar);
        }

        fn browser_process_handler(&self) -> Option<BrowserProcessHandler> {
            Some(SidecarProcessHandler::new(self.pump.clone()))
        }
    }
}

wrap_browser_process_handler! {
    struct SidecarProcessHandler {
        pump: Sender<crate::Event>,
    }
    impl BrowserProcessHandler {
        // Any thread: the main loop wakes for it.
        fn on_schedule_message_pump_work(&self, delay_ms: i64) {
            let _ = self.pump.send(crate::Event::Pump(delay_ms));
        }

        // Attach our global RequestContextHandler so extension service workers, offscreen
        // documents, and extension-origin initiators can never fetch or redirect to `file://`.
        fn default_request_context_handler(&self) -> Option<RequestContextHandler> {
            Some(crate::context::DefaultRequestContextHandler::new())
        }
    }
}
