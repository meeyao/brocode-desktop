//! Desktop host adapter for `opencode-feature-connect` (`features/connect`).

pub use opencode_feature_connect::*;

use opencode_kernel::{CommandEntry, Feature, KernelBuilder, OverlayRegistration, Slot};
use opencode_kit::integration::Integration;
use rune::Tx;
use serde_json::Value;
use session_view::pickers::Model;

use crate::App;
use crate::jobs::Apply;
use crate::pickers::PickerHost as _;

pub(crate) static CONNECT: Slot<Connect> = Slot::new();
static CATALOGS: Slot<Catalogs> = Slot::new();

impl ConnectHost for App {
    fn connect(&self) -> &Connect {
        self.kernel.slots.get(&CONNECT)
    }

    fn connect_mut(&mut self) -> &mut Connect {
        self.kernel.slots.get_mut(&CONNECT)
    }

    fn catalogs(&self) -> &Catalogs {
        self.kernel.slots.get(&CATALOGS)
    }

    fn catalogs_mut(&mut self) -> &mut Catalogs {
        self.kernel.slots.get_mut(&CATALOGS)
    }

    fn active_directory(&self) -> String {
        match &*self.route {
            crate::Route::Session { id, .. } => self.session(id).map(|s| s.directory.clone()).unwrap_or_default(),
            _ => self.draft_dir().to_owned(),
        }
    }

    fn picker_scope(&self) -> String {
        crate::pickers::PickerHost::picker_scope(self)
    }

    fn auth_host(&self) -> AuthHost {
        use opencode_kit::servers::loopback_origin;
        use opencode_servers::ServerConn;
        let key = crate::servers::runtime::active(self);
        if key.is_sidecar() {
            return AuthHost::Local;
        }
        let Some(entry) = crate::servers::registry::list(self).into_iter().find(|e| &e.key() == key) else {
            return AuthHost::Remote(crate::servers::runtime::label(key));
        };
        match &entry.conn {
            ServerConn::Sidecar { .. } => AuthHost::Local,
            ServerConn::Http { http, .. } if loopback_origin(&http.url).is_some() => AuthHost::Local,
            conn => AuthHost::Remote(crate::servers::registry::label(conn)),
        }
    }

    fn window_id(&self) -> &str {
        &self.window.id
    }

    fn can_write(&self) -> bool {
        App::can_write(self)
    }

    fn chatgpt_welcome_seen(&self) -> bool {
        self.settings.chatgpt_welcome_seen
    }

    fn mark_chatgpt_welcome_seen(&mut self) {
        self.change_settings(|x| x.chatgpt_welcome_seen = true);
    }

    fn chatgpt_usage_limit_seen_at(&self) -> Option<i64> {
        crate::storage::json("opencode.global.dat", "chatgpt-usage-limit")
            .and_then(|v| v["lastSeenAt"].as_i64())
    }

    fn mark_chatgpt_usage_limit_seen(&mut self, at_ms: i64) {
        if self.persists() {
            crate::storage::set_json(
                "opencode.global.dat",
                "chatgpt-usage-limit",
                &serde_json::json!({ "lastSeenAt": at_ms }),
            );
        }
    }

    fn current_model_key(&self) -> Option<String> {
        let m = self.selection().model;
        (!m.id.is_empty()).then(|| format!("{}:{}", m.provider, m.id))
    }

    fn select_free_model(&mut self, provider: &str, id: &str, _tx: &mut Tx) {
        let key = format!("{provider}:{id}");
        let scope = crate::pickers::PickerHost::picker_scope(self);
        let mut selection = self.selection();
        selection.model = opencode_kit::model::ModelRef {
            id: id.to_owned(),
            provider: provider.to_owned(),
            variant: String::new(),
        };
        self.composer_ext.pickers.prefs.picked(&key);
        let draft = self.session(&scope).is_none();
        self.composer_ext.pickers.pick(scope.clone(), selection, draft);
        crate::pickers::saved::save(self, &scope);
    }

    fn models(&self) -> &[Model] {
        &self.composer_ext.pickers.models
    }

    fn load_providers(&mut self, list: &Value) {
        self.composer_ext.pickers.load_providers(list);
    }

    fn load_config(&mut self, list: &Value) {
        self.composer_ext.pickers.load_config(list);
        // A new window's pickers start with it, before its server answers (`saved_catalog`).
        crate::launch::saved_catalog::remember_config(list);
    }

    fn load_models(&mut self, list: &Value) {
        self.composer_ext.pickers.load_models(list);
    }

    fn load_agents(&mut self, list: &Value) {
        self.composer_ext.pickers.load_agents(list);
    }

    fn reveal_models(&mut self, keys: &[String]) {
        self.composer_ext.pickers.prefs.set_visibility(keys.iter().map(String::as_str), true);
    }

    fn apply_connected_model(&mut self, key: &str, scope: Option<&str>, tx: &mut Tx) {
        let Some(m) = self.composer_ext.pickers.models.iter().find(|m| m.key == key).cloned() else { return };
        if scope.is_none() {
            self.close_settings();
            self.new_session(tx);
        }
        let scope = scope.map(str::to_owned).unwrap_or_else(|| crate::pickers::PickerHost::picker_scope(self));
        let mut selection = self.selection();
        selection.model = opencode_kit::model::ModelRef { id: m.id, provider: m.provider, variant: String::new() };
        self.composer_ext.pickers.prefs.picked(key);
        let draft = self.session(&scope).is_none();
        self.composer_ext.pickers.pick(scope.clone(), selection, draft);
        crate::pickers::saved::save(self, &scope);
    }

    fn picker_reads(&self) -> &crate::connect::PickerReads {
        &self.jobs.picker_reads
    }

    fn picker_reads_mut(&mut self) -> &mut crate::connect::PickerReads {
        &mut self.jobs.picker_reads
    }

    fn before_dialog_open(&mut self) {
        self.close_picker(false);
        self.composer_ext.pickers.close_manage();
        self.composer.focused = false;
    }

    fn auto_focus_composer(&mut self, tx: &mut Tx) {
        if crate::auto_focus_composer(tx) {
            self.composer.focused = self.has_composer() && !self.settings_open();
        }
    }

    fn open_external(&mut self, url: &str, tx: &mut Tx) -> bool {
        crate::open_in::open_external(tx, url)
    }

    fn spawn_job(&mut self, job: ConnectJob) {
        match job {
            ConnectJob::Server { delay, request } => {
                self.job_after(delay, move |srv| {
                    let reply = execute_request(srv, request);
                    Box::new(move |s: &mut App, tx: &mut Tx| apply_reply(s, reply, tx)) as Apply
                });
            }
            ConnectJob::Timer { delay, reply } => {
                self.job_after(delay, move |_| {
                    Box::new(move |s: &mut App, tx: &mut Tx| apply_reply(s, reply, tx)) as Apply
                });
            }
            ConnectJob::Defer { reply } => {
                self.later(Box::new(move |s: &mut App, tx: &mut Tx| apply_reply(s, reply, tx)) as Apply);
            }
        }
    }
}

impl App {
    /// Opens the dialog on its list, or on `provider`'s sign-in. From Settings it signs in at the
    /// server's default location; from the composer, in the composer's folder.
    pub fn open_connect(&mut self, provider: Option<&str>, from_settings: bool, tx: &mut Tx) {
        open(self, provider, from_settings, tx);
    }

    pub fn close_connect(&mut self, tx: &mut Tx) {
        close(self, tx);
    }

    /// The integrations at `dir` (the server's default when none), once read.
    pub(crate) fn integrations(&self, dir: Option<&str>) -> Option<&[Integration]> {
        integrations(self, dir)
    }

    /// The providers the server runs at `dir`, once read.
    pub(crate) fn provider_infos(&self, dir: Option<&str>) -> Option<&[ProviderInfo]> {
        provider_infos(self, dir)
    }

    /// Puts a scene's integrations and providers in the server's default slot, as a sync would.
    pub(crate) fn seed_catalogs(&mut self, integrations: Vec<Integration>, providers: Vec<ProviderInfo>) {
        seed_catalogs(self, integrations, providers);
    }

    /// Reads `dir`'s integrations, providers, and model list again.
    pub(crate) fn sync_catalogs(&mut self, dir: Option<String>) {
        sync_catalogs(self, dir);
    }
}

/// Handles `session.step.failed` ChatGPT usage-limit errors (`usage-exceeded-dialogs.tsx:145-154`,
/// `#64926`): opens the personal dialog for `provider.chatgpt-limit` (including older servers) and
/// the Business workspace-admin variant for `provider.chatgpt-workspace-limit`.
pub(crate) fn on_step_failed(s: &mut App, data: &Value) -> bool {
    if s.settings_open() || !s.kernel.overlays.is_empty() {
        return false;
    }
    let workspace = match data["error"]["type"].as_str() {
        Some("provider.chatgpt-limit") => false,
        Some("provider.chatgpt-workspace-limit") => true,
        _ => return false,
    };
    usage_limit::open(s, workspace, &mut Tx::default())
}

/// Kernel feature registration for the Connect provider and ChatGPT plan welcome dialogs.
pub struct ConnectFeature;

impl Feature<App> for ConnectFeature {
    fn id(&self) -> &'static str {
        "feature.connect"
    }

    fn register(&self, builder: &mut KernelBuilder<App>) {
        builder
            .register_slot(&CONNECT)
            .register_slot(&CATALOGS);
        builder.add_command(
            CommandEntry::<App>::from_key(
                "provider.connect",
                opencode_i18n::key!("command.provider.connect"),
                crate::commands::order::PROVIDER_CONNECT,
                |s, tx| s.open_connect(None, false, tx),
            )
            .describe(opencode_i18n::key!("command.provider.connect.description"))
            .slash("connect")
            .when(App::has_composer),
        );
        builder.register_overlay(
            OverlayRegistration::<App>::new(OVERLAY_ID)
                .with_render(crate::overlays::layer::DIALOG, |s, cx| {
                    let p = crate::palette::pal(s.dark(cx));
                    view::layer(s, p, cx)
                })
                .above(crate::message_nav::JUMP_OVERLAY_ID)
                .with_key(|s, ev, tx| key(s, ev, tx))
                .with_dismiss(|s, tx| close(s, tx)),
        );
        builder.register_overlay(
            OverlayRegistration::<App>::new(WELCOME_OVERLAY_ID)
                .with_render(crate::overlays::layer::DIALOG, |s, cx| {
                    let p = crate::palette::pal(s.dark(cx));
                    welcome::layer(s, p, cx)
                })
                .above(OVERLAY_ID)
                .with_dismiss(|s, tx| welcome::close(s, tx)),
        );
        builder.register_overlay(
            OverlayRegistration::<App>::new(USAGE_LIMIT_OVERLAY_ID)
                .with_render(crate::overlays::layer::DIALOG, |s, cx| {
                    let p = crate::palette::pal(s.dark(cx));
                    usage_limit::layer(s, p, cx)
                })
                .above(WELCOME_OVERLAY_ID)
                .with_dismiss(|s, tx| usage_limit::close(s, tx)),
        );
        builder.register_overlay(
            OverlayRegistration::<App>::new(UNPAID_OVERLAY_ID)
                .with_render(crate::overlays::layer::DIALOG, |s, cx| {
                    let p = crate::palette::pal(s.dark(cx));
                    unpaid::layer(s, p, cx)
                })
                .above(USAGE_LIMIT_OVERLAY_ID)
                .with_dismiss(|s, tx| unpaid::close(s, tx)),
        );
    }
}

mod scenes {
    //! The connect dialog's steps that only a write reaches, as scenes (`--scene connect-models`):
    //! a run that can't write still shows them. Each sets the step as the simulated server leaves
    //! it: Simulated connected with its models, GitHub Copilot's device sign-in refused by the
    //! simulated network, and Poe's browser sign-in waiting. The `-long` ones fill a step past the
    //! dialog's height: 25 models, and an error many lines long.

    use std::rc::Rc;

    use opencode_kit::integration::{Authorization, Method, MethodKind};

    use super::dialog::{ReturnTo, Step};
    use super::machine::{Catalog, Paste, Retry, Sel, Signin, State};
    use crate::App;

    fn oauth(id: &str, label: &str) -> Method {
        Method { kind: MethodKind::Oauth, id: Some(id.into()), label: Some(label.into()), form: vec![] }
    }

    fn sel(method: Method) -> Sel {
        Sel { attempt: 2, catalog: Rc::new(Catalog { integration: None, methods: vec![method] }), method: 0, answer: None }
    }

    fn connect(state: State) -> Step {
        Step::Connect { back: ReturnTo::Picker, copied: None, connect: state }
    }

    /// 25 models from Drive, the first of them picked.
    fn long_models(s: &mut App) {
        s.composer_ext.pickers.models.retain(|m| m.provider != "drive");
        s.composer_ext.pickers.models.extend((1..=25).map(|i| crate::pickers::Model {
            key: format!("drive:model-{i}"),
            id: format!("model-{i}"),
            provider: "drive".into(),
            group: "Drive".into(),
            name: format!("Drive Model {i}"),
            variants: vec![],
            hidden: false,
            free: false,
            latest: false,
            inputs: vec![],
            tools: true,
            speed: None,
        }));
        super::preview_step(s, "drive", Step::Models);
    }

    /// Single-provider models scene: keeps live Drive models when present, or seeds sample ones for fixture runs.
    fn single_models(s: &mut App) {
        if !s.composer_ext.pickers.models.iter().any(|m| m.provider == "drive") {
            s.composer_ext.pickers.models.extend([
                crate::pickers::Model { key: "drive:simulated".into(), id: "simulated".into(), provider: "drive".into(), group: "Drive".into(), name: "Drive Simulated".into(), variants: vec![], hidden: false, free: false, latest: false, inputs: vec![], tools: true, speed: None },
                crate::pickers::Model { key: "drive:fast".into(), id: "fast".into(), provider: "drive".into(), group: "Drive".into(), name: "Drive Fast".into(), variants: vec![], hidden: false, free: false, latest: false, inputs: vec![], tools: true, speed: None },
            ]);
        }
        super::preview_step(s, "drive", Step::Models);
    }

    /// Multi-provider Console workspace models (tests sticky provider headers + prefix strip).
    fn multi_models(s: &mut App) {
        use opencode_feature_connect::{ProviderInfo, seed_catalogs};
        use opencode_kit::integration::Integration;
        seed_catalogs(
            s,
            vec![Integration { id: "opencode".into(), name: "OpenCode".into(), ..Default::default() }],
            vec![
                ProviderInfo { id: "ws-openai".into(), name: "Acme / OpenAI".into(), integration: Some("opencode".into()) },
                ProviderInfo { id: "ws-anthropic".into(), name: "Acme / Anthropic".into(), integration: Some("opencode".into()) },
            ],
        );
        s.composer_ext.pickers.models.extend([
            crate::pickers::Model { key: "ws-openai:gpt-5".into(), id: "gpt-5".into(), provider: "ws-openai".into(), group: "Acme / OpenAI".into(), name: "GPT-5".into(), variants: vec![], hidden: false, free: false, latest: false, inputs: vec![], tools: true, speed: None },
            crate::pickers::Model { key: "ws-anthropic:claude-sonnet".into(), id: "claude-sonnet".into(), provider: "ws-anthropic".into(), group: "Acme / Anthropic".into(), name: "Claude Sonnet".into(), variants: vec![], hidden: false, free: false, latest: false, inputs: vec![], tools: true, speed: None },
        ]);
        super::preview_step(s, "opencode", Step::Models);
    }

    rune_register::register!(crate::scenes::SCENE_LIST: crate::scenes::Scene = [
        ("connect-models", single_models),
        ("connect-models-multi", multi_models),
        ("connect-models-long", long_models),
        ("connect-failed-long", |s| {
            let error = (1..=30).map(|i| format!("at frame {i}: ECONNREFUSED 127.0.0.1:{}", 4000 + i)).collect::<Vec<_>>().join("\n");
            super::preview_step(s, "github-copilot", connect(State::Failed(sel(oauth("device", "Login with GitHub Copilot")), error, Retry::Start)));
        }),
        ("connect-failed", |s| {
            let error = "ECONNREFUSED: Unable to connect. Is the computer able to access the url?".to_owned();
            super::preview_step(s, "github-copilot", connect(State::Failed(sel(oauth("device", "Login with GitHub Copilot")), error, Retry::Start)));
        }),
        ("connect-browser", |s| {
            let method = oauth("poe", "Login with Poe (browser)");
            let auth = Authorization {
                attempt: "con_parity".into(),
                url: "https://poe.com/oauth/authorize?response_type=code&scope=apikey%3Acreate".into(),
                instructions: "Complete authorization in your browser. This window will close automatically.".into(),
                code: false,
                paste: false,
                created: 0,
                expires: 0,
            };
            super::preview_step(
                s,
                "poe",
                connect(State::Waiting(sel(method.clone()), Signin::Browser { method, auth, failed: false, paste: Paste::Unsupported })),
            );
        }),
        ("chatgpt-usage-limit", |s| {
            crate::scenes::open_scene(s);
            super::usage_limit::preview(s, false, super::CopyStatus::Idle);
        }),
        ("chatgpt-workspace-usage-limit", |s| {
            crate::scenes::open_scene(s);
            super::usage_limit::preview(s, true, super::CopyStatus::Idle);
        }),
        ("chatgpt-workspace-usage-limit-copied", |s| {
            crate::scenes::open_scene(s);
            super::usage_limit::preview(s, true, super::CopyStatus::Copied);
        }),
        ("chatgpt-workspace-usage-limit-failed", |s| {
            crate::scenes::open_scene(s);
            super::usage_limit::preview(s, true, super::CopyStatus::Failed);
        }),
        ("unpaid-models", |s| {
            crate::scenes::open_scene(s);
            s.composer_ext.pickers.provider_order = vec!["opencode".into()];
            s.composer_ext.pickers.models = vec![
                crate::pickers::Model {
                    key: "opencode:big-pickle".into(),
                    id: "big-pickle".into(),
                    provider: "opencode".into(),
                    group: "Brocode".into(),
                    name: "Big Pickle (free)".into(),
                    variants: Vec::new(),
                    hidden: false,
                    free: true,
                    latest: true,
                    inputs: Default::default(),
                    tools: true,
                    speed: None,
                },
                crate::pickers::Model {
                    key: "opencode:minimax-free".into(),
                    id: "minimax-free".into(),
                    provider: "opencode".into(),
                    group: "Brocode".into(),
                    name: "MiniMax M2.5 Free".into(),
                    variants: Vec::new(),
                    hidden: false,
                    free: true,
                    latest: false,
                    inputs: Default::default(),
                    tools: true,
                    speed: None,
                },
            ];
            super::unpaid::preview(s);
        }),
    ]);
}
