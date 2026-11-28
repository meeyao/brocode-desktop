//! Connect an integration (Desktop's `DialogConnectProvider`, `providers/connect/*`): the
//! searchable list of providers and services, then the picked one's sign-in (an API key, a
//! browser sign-in, a device code, a command or an external login, with the method's form), its
//! accounts once connected, and the models to start with. `+` beside the model search, `/connect`
//! and Settings › Providers open it. The steps are [`dialog`] and [`machine`]; [`run`] executes
//! their effects against the server through [`ConnectHost`].

#![forbid(unsafe_code)]

pub mod catalog;
pub mod dialog;
pub mod form;
pub mod machine;
pub mod picker_reads;
pub mod run;
pub mod steps;
mod ui;
pub mod unpaid;
pub mod usage_limit;
pub mod view;
pub mod welcome;

use rune::machine::Effects;
use rune::{Key, KeyEvent, TextEdit, Tx};
use serde_json::Value;
use opencode_kit::model::Model;

pub use catalog::{
    CatalogRead, Catalogs, ProviderInfo, SyncAfter, integration_of, integrations, provider_infos, seed_catalogs, sync_catalogs,
};
pub use picker_reads::{PickerList, PickerRead, PickerReads};
pub use run::{ConnectJob, ConnectReply, ConnectRequest, apply_reply, execute_request};

use dialog::{Accounts, Dialog, Event, Provider, Step};

/// Overlay stack identifier for the Connect provider dialog (`opencode_kernel::OverlayStack`).
pub const OVERLAY_ID: &str = "dialog.connect";
/// Overlay stack identifier for the ChatGPT plan welcome dialog (`opencode_kernel::OverlayStack`).
pub const WELCOME_OVERLAY_ID: &str = "dialog.connect.welcome";
/// Overlay stack identifier for the ChatGPT usage limit dialog (`opencode_kernel::OverlayStack`).
pub const USAGE_LIMIT_OVERLAY_ID: &str = "dialog.connect.usage-limit";
/// Overlay stack identifier for the unpaid free-model picker (`opencode_kernel::OverlayStack`).
pub const UNPAID_OVERLAY_ID: &str = "dialog.connect.unpaid";

/// `providers/connect/remote.tsx::authServerName`. An HTTP connection on *any* loopback
/// port counts as local here. A plugin's SSH tunnel remains remote even when it listens on
/// loopback. Do not use this UI distinction to grant access to local files.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum AuthHost {
    #[default]
    Local,
    Remote(String),
}

/// The open dialog and what its inputs hold.
pub struct Open {
    pub dialog: Dialog,
    /// Where the credential is stored, not whether this endpoint shares a filesystem.
    pub auth_host: AuthHost,
    /// The folder the sign-in belongs to; none for the server's default location.
    pub dir: Option<String>,
    /// The composer whose model Continue sets; none starts a new session with it.
    pub scope: Option<String>,
    pub search: TextEdit,
    /// The list's lit row, by integration id.
    pub active: Option<String>,
    /// The API key or the pasted code; a new step starts it empty.
    pub entry: TextEdit,
    /// A form step's text, or its own answer; a new step starts it on its default.
    pub field: TextEdit,
    /// The account whose options are open.
    pub menu: Option<String>,
    pub rename: TextEdit,
}

/// Copy-request status inside the ChatGPT Business workspace usage limit dialog (`#64926`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CopyStatus {
    #[default]
    Idle,
    Copied,
    Failed,
}

/// Active modal owned by [`Connect`]: mutually exclusive across Connect, ChatGPT welcome,
/// ChatGPT usage limit, and unpaid free-model picker.
#[derive(Default)]
pub enum Modal {
    #[default]
    Closed,
    Dialog(Box<Open>),
    Welcome,
    UsageLimit { workspace: bool, copy: CopyStatus },
    Unpaid { active: Option<usize> },
}

pub enum ModalEvent {
    /// Open (or reopen) the Connect provider dialog, replacing any active modal.
    OpenDialog(Box<Open>),
    /// Close the Connect provider dialog; shows the ChatGPT plan welcome when `show_welcome` is true.
    CloseDialog { show_welcome: bool },
    /// Open the ChatGPT plan welcome modal directly when no modal is open.
    OpenWelcome,
    /// Close the ChatGPT plan welcome modal.
    CloseWelcome,
    /// Open the ChatGPT usage limit modal (personal or Business workspace) when closed.
    OpenUsageLimit { workspace: bool, seen_at: i64 },
    /// Record copy-request feedback inside an open workspace usage limit modal.
    UsageLimitCopy(CopyStatus),
    /// Close the ChatGPT usage limit modal.
    CloseUsageLimit,
    /// Open the unpaid free-model picker modal, replacing any active modal.
    OpenUnpaid,
    /// Close the unpaid free-model picker modal.
    CloseUnpaid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModalEffect {
    OpenOverlay(&'static str),
    CloseOverlay(&'static str),
    MarkWelcomeSeen,
    MarkUsageLimitSeen(i64),
    FocusComposer,
}

impl Modal {
    fn overlay_id(&self) -> Option<&'static str> {
        match self {
            Self::Closed => None,
            Self::Dialog(_) => Some(OVERLAY_ID),
            Self::Welcome => Some(WELCOME_OVERLAY_ID),
            Self::UsageLimit { .. } => Some(USAGE_LIMIT_OVERLAY_ID),
            Self::Unpaid { .. } => Some(UNPAID_OVERLAY_ID),
        }
    }

    pub fn step(&mut self, event: ModalEvent) -> Effects<ModalEffect> {
        let prev = self.overlay_id();
        match (self, event) {
            (m, ModalEvent::OpenDialog(open)) => {
                *m = Self::Dialog(open);
                match prev {
                    Some(id) if id != OVERLAY_ID => Effects::two(ModalEffect::CloseOverlay(id), ModalEffect::OpenOverlay(OVERLAY_ID)),
                    _ => Effects::one(ModalEffect::OpenOverlay(OVERLAY_ID)),
                }
            }
            (m @ Self::Dialog(_), ModalEvent::CloseDialog { show_welcome: true }) => {
                *m = Self::Welcome;
                let mut effects = Effects::two(ModalEffect::CloseOverlay(OVERLAY_ID), ModalEffect::MarkWelcomeSeen);
                effects.push(ModalEffect::OpenOverlay(WELCOME_OVERLAY_ID));
                effects
            }
            (m @ Self::Dialog(_), ModalEvent::CloseDialog { show_welcome: false }) => {
                *m = Self::Closed;
                Effects::two(ModalEffect::CloseOverlay(OVERLAY_ID), ModalEffect::FocusComposer)
            }
            (m @ Self::Closed, ModalEvent::OpenWelcome) => {
                *m = Self::Welcome;
                Effects::two(ModalEffect::MarkWelcomeSeen, ModalEffect::OpenOverlay(WELCOME_OVERLAY_ID))
            }
            (m @ Self::Welcome, ModalEvent::CloseWelcome) => {
                *m = Self::Closed;
                Effects::two(ModalEffect::CloseOverlay(WELCOME_OVERLAY_ID), ModalEffect::FocusComposer)
            }
            (m @ Self::Closed, ModalEvent::OpenUsageLimit { workspace, seen_at }) => {
                *m = Self::UsageLimit { workspace, copy: CopyStatus::Idle };
                Effects::two(ModalEffect::MarkUsageLimitSeen(seen_at), ModalEffect::OpenOverlay(USAGE_LIMIT_OVERLAY_ID))
            }
            (Self::UsageLimit { copy, .. }, ModalEvent::UsageLimitCopy(next)) => {
                *copy = next;
                Effects::none()
            }
            (m @ Self::UsageLimit { .. }, ModalEvent::CloseUsageLimit) => {
                *m = Self::Closed;
                Effects::two(ModalEffect::CloseOverlay(USAGE_LIMIT_OVERLAY_ID), ModalEffect::FocusComposer)
            }
            (m, ModalEvent::OpenUnpaid) => {
                *m = Self::Unpaid { active: None };
                match prev {
                    Some(id) if id != UNPAID_OVERLAY_ID => Effects::two(ModalEffect::CloseOverlay(id), ModalEffect::OpenOverlay(UNPAID_OVERLAY_ID)),
                    _ => Effects::one(ModalEffect::OpenOverlay(UNPAID_OVERLAY_ID)),
                }
            }
            (m @ Self::Unpaid { .. }, ModalEvent::CloseUnpaid) => {
                *m = Self::Closed;
                Effects::two(ModalEffect::CloseOverlay(UNPAID_OVERLAY_ID), ModalEffect::FocusComposer)
            }
            _ => Effects::none(),
        }
    }
}

#[derive(Default)]
pub struct Connect {
    pub modal: Modal,
    /// brocode: the API Brother key prompt has run its once-per-launch check.
    pub brand_prompted: bool,
    /// brocode: the startup catalog sync for the prompt has been asked for.
    pub brand_synced: bool,
    /// Fallback editor targets so `on_edit` callbacks never panic if fired after close.
    scratch_search: TextEdit,
    scratch_entry: TextEdit,
    scratch_field: TextEdit,
    scratch_rename: TextEdit,
}

impl Connect {
    /// Whether the Connect provider dialog is open.
    pub fn is_open(&self) -> bool {
        matches!(self.modal, Modal::Dialog(_))
    }

    /// Whether the ChatGPT plan welcome modal is open.
    pub fn is_welcome_open(&self) -> bool {
        matches!(self.modal, Modal::Welcome)
    }

    /// Whether the ChatGPT usage limit modal is open.
    pub fn is_usage_limit_open(&self) -> bool {
        matches!(self.modal, Modal::UsageLimit { .. })
    }

    /// Current `(workspace, copy_status)` when the ChatGPT usage limit modal is open.
    pub fn usage_limit(&self) -> Option<(bool, CopyStatus)> {
        match self.modal {
            Modal::UsageLimit { workspace, copy } => Some((workspace, copy)),
            _ => None,
        }
    }

    /// Whether the unpaid free-model picker modal is open.
    pub fn is_unpaid_open(&self) -> bool {
        matches!(self.modal, Modal::Unpaid { .. })
    }

    /// Active keyboard index when the unpaid free-model picker is open.
    pub fn unpaid_active(&self) -> Option<Option<usize>> {
        match self.modal {
            Modal::Unpaid { active } => Some(active),
            _ => None,
        }
    }

    /// Whether any Connect modal (provider dialog, ChatGPT welcome/usage limit, or unpaid picker)
    /// is open.
    pub fn is_modal_open(&self) -> bool {
        !matches!(self.modal, Modal::Closed)
    }

    pub fn dialog(&self) -> Option<&Open> {
        match &self.modal {
            Modal::Dialog(open) => Some(open),
            _ => None,
        }
    }

    pub fn dialog_mut(&mut self) -> Option<&mut Open> {
        match &mut self.modal {
            Modal::Dialog(open) => Some(open),
            _ => None,
        }
    }

    pub(crate) fn search_mut(&mut self) -> &mut TextEdit {
        match &mut self.modal {
            Modal::Dialog(o) => &mut o.search,
            _ => &mut self.scratch_search,
        }
    }

    pub(crate) fn entry_mut(&mut self) -> &mut TextEdit {
        match &mut self.modal {
            Modal::Dialog(o) => &mut o.entry,
            _ => &mut self.scratch_entry,
        }
    }

    pub(crate) fn field_mut(&mut self) -> &mut TextEdit {
        match &mut self.modal {
            Modal::Dialog(o) => &mut o.field,
            _ => &mut self.scratch_field,
        }
    }

    pub(crate) fn rename_mut(&mut self) -> &mut TextEdit {
        match &mut self.modal {
            Modal::Dialog(o) => &mut o.rename,
            _ => &mut self.scratch_rename,
        }
    }
}

/// Object-safe host boundary for the Connect provider dialog, ChatGPT plan welcome, integration
/// catalogs, and server-wide picker catalog reads.
pub trait ConnectHost: opencode_kernel::AppEnv + opencode_kernel::AppFeedback + opencode_kernel::AppOverlays {
    fn connect(&self) -> &Connect;
    fn connect_mut(&mut self) -> &mut Connect;
    fn catalogs(&self) -> &Catalogs;
    fn catalogs_mut(&mut self) -> &mut Catalogs;

    /// Folder for a composer-launched sign-in (the active session's or draft's directory).
    fn active_directory(&self) -> String;
    /// Picker scope key for the composer that opened the dialog (e.g. session ID or `"draft"`).
    fn picker_scope(&self) -> String {
        "draft".into()
    }
    /// Whether the active server stores credentials locally or on a named remote server.
    fn auth_host(&self) -> AuthHost {
        AuthHost::Local
    }
    /// Stable window identifier passed as Console OAuth `return_window`, or empty if none.
    fn window_id(&self) -> &str {
        ""
    }

    /// Whether this window may mutate server state (defaults to [`opencode_kernel::AppEnv::is_live`]).
    fn can_write(&self) -> bool {
        self.is_live()
    }

    /// Whether the ChatGPT plan welcome card has already been shown.
    fn chatgpt_welcome_seen(&self) -> bool {
        false
    }
    /// Persists that the ChatGPT plan welcome card has been shown.
    fn mark_chatgpt_welcome_seen(&mut self) {}
    /// Epoch ms when the ChatGPT usage limit dialog was last shown (24 h throttle).
    fn chatgpt_usage_limit_seen_at(&self) -> Option<i64> {
        None
    }
    /// Persists when the ChatGPT usage limit dialog was last shown.
    fn mark_chatgpt_usage_limit_seen(&mut self, _at_ms: i64) {}
    /// Currently selected `provider:id` key for marking the active row in the unpaid picker.
    fn current_model_key(&self) -> Option<String> {
        None
    }
    /// Selects a free model from the unpaid free-model picker.
    fn select_free_model(&mut self, _provider: &str, _id: &str, _tx: &mut Tx) {}

    /// Models currently known to the composer's pickers.
    fn models(&self) -> &[Model];
    /// Updates the host's picker providers list from `/api/provider`.
    fn load_providers(&mut self, list: &Value);
    /// Updates the host's picker config from `/api/config`.
    fn load_config(&mut self, list: &Value);
    /// Updates the host's picker models list from `/api/model`.
    fn load_models(&mut self, list: &Value);
    /// Updates the host's picker agents list from `/api/agent`.
    fn load_agents(&mut self, list: &Value);
    /// Marks newly unlocked model keys visible in model preferences.
    fn reveal_models(&mut self, keys: &[String]);
    /// Applies the model picked on the final Models step (into `scope`, or starts a new session
    /// when `scope` is `None`).
    fn apply_connected_model(&mut self, key: &str, scope: Option<&str>, tx: &mut Tx);
    /// Notified after catalog/model data changes (optional hook for host picker/UI refreshes).
    fn on_models_changed(&mut self, tx: &mut Tx) {
        let _ = tx;
    }

    /// Server-scoped generation & debounce state for [`picker_reads`].
    fn picker_reads(&self) -> &PickerReads;
    /// Mutable server-scoped generation & debounce state for [`picker_reads`].
    fn picker_reads_mut(&mut self) -> &mut PickerReads;

    /// Prepares the shell before opening the Connect dialog (e.g. closes pickers/manage models
    /// and blurs composer).
    fn before_dialog_open(&mut self) {}
    /// Restores composer focus after closing the Connect or Welcome dialog when appropriate.
    fn auto_focus_composer(&mut self, tx: &mut Tx) {
        let _ = tx;
    }
    /// Opens `url` in the system browser; returns `true` when launch succeeded.
    fn open_external(&mut self, url: &str, tx: &mut Tx) -> bool {
        harness_log::open_url(tx, url)
    }
    /// Schedules an off-thread server request, timer, or next-poll deferred reply.
    fn spawn_job(&mut self, job: ConnectJob);
}

fn focused(text: &str) -> TextEdit {
    let mut t = TextEdit::new(text);
    t.focused = true;
    t
}

pub(crate) fn apply_modal(host: &mut dyn ConnectHost, event: ModalEvent, tx: &mut Tx) {
    let effects = host.connect_mut().modal.step(event);
    for effect in effects {
        match effect {
            ModalEffect::OpenOverlay(id) => {
                let return_focus = tx.focused().map(str::to_owned);
                host.on_overlay_opened(id, return_focus);
            }
            ModalEffect::CloseOverlay(id) => host.on_overlay_closed(id),
            ModalEffect::MarkWelcomeSeen => host.mark_chatgpt_welcome_seen(),
            ModalEffect::MarkUsageLimitSeen(at_ms) => host.mark_chatgpt_usage_limit_seen(at_ms),
            ModalEffect::FocusComposer => host.auto_focus_composer(tx),
        }
    }
}

/// brocode: ask for the API Brother key once per run when the server has none, the way the
/// CLI's startup prompt does. Runs after the integration catalog lands; leaves the prompt for a
/// later frame while another modal is open.
pub fn maybe_brand_prompt(host: &mut dyn ConnectHost, tx: &mut Tx) {
    if host.connect().brand_prompted {
        return;
    }
    let Some(list) = host.catalogs().integrations(None) else {
        // The catalog is otherwise read only when Connect or Settings opens.
        if !host.connect().brand_synced {
            host.connect_mut().brand_synced = true;
            crate::catalog::sync_catalogs(host, None);
        }
        return;
    };
    let Some(brand) = list.iter().find(|i| i.id == "brocode") else {
        host.connect_mut().brand_prompted = true;
        return;
    };
    let can_key = brand
        .methods
        .iter()
        .any(|m| matches!(m.kind, opencode_kit::integration::MethodKind::Key | opencode_kit::integration::MethodKind::Env));
    if !brand.connections.is_empty() || !can_key {
        host.connect_mut().brand_prompted = true;
        return;
    }
    if host.connect().is_modal_open() {
        return;
    }
    host.connect_mut().brand_prompted = true;
    eprintln!("brocode: no API Brother key on the server; opening the connect prompt");
    open(host, Some("brocode"), false, tx);
}

/// Opens the dialog on its list, or on `provider`'s sign-in. From Settings it signs in at the
/// server's default location; from the composer, in the composer's folder.
pub fn open(host: &mut dyn ConnectHost, provider: Option<&str>, from_settings: bool, tx: &mut Tx) {    tx.instant();
    host.before_dialog_open();
    let dir = (!from_settings).then(|| host.active_directory()).filter(|d| !d.is_empty());
    let scope = (!from_settings).then(|| host.picker_scope());
    let auth_host = host.auth_host();
    apply_modal(
        host,
        ModalEvent::OpenDialog(Box::new(Open {
            dialog: Dialog::default(),
            auth_host,
            dir: dir.clone(),
            scope,
            search: focused(""),
            active: None,
            entry: TextEdit::new(""),
            field: TextEdit::new(""),
            menu: None,
            rename: TextEdit::new(""),
        })),
        tx,
    );
    fresh_inputs(host);
    catalog::sync_catalogs(host, dir);
    if let Some(id) = provider {
        let accounts = accounts_of(host, id);
        connect_event(host, Event::Selected { provider: id.to_owned(), from_picker: false, accounts }, tx);
    }
}

/// Closes the Connect dialog, notifies `OverlayStack`, and opens the ChatGPT plan welcome when
/// earned or restores composer focus.
pub fn close(host: &mut dyn ConnectHost, tx: &mut Tx) {
    if !host.connect().is_open() {
        return;
    }
    tx.instant();
    let show_welcome = host.connect().dialog().is_some_and(|o| o.dialog.chatgpt_welcome) && !host.chatgpt_welcome_seen();
    connect_event(host, Event::Disposed, tx);
    apply_modal(host, ModalEvent::CloseDialog { show_welcome }, tx);
}

/// Opens the Connect dialog directly onto a synthetic `provider` + `step` (for scene previews).
pub fn preview_step(host: &mut dyn ConnectHost, provider: &str, step: Step) {
    open(host, None, false, &mut Tx::default());
    if let Some(open) = host.connect_mut().dialog_mut() {
        open.dialog = Dialog {
            session: 1,
            provider: Some(Provider { id: provider.into(), accounts: Accounts::default(), step, closed: false }),
            ..Dialog::default()
        };
        let in_models = matches!(open.dialog.provider.as_ref().map(|p| &p.step), Some(Step::Models));
        open.search = if in_models { focused("") } else { TextEdit::new("") };
        open.active = None;
    }
}

/// The folder the currently open Connect dialog targets, if any.
pub fn connect_dir(host: &dyn ConnectHost) -> Option<&str> {
    host.connect().dialog().and_then(|o| o.dir.as_deref())
}

/// The integration's accounts as the server lists them, the active one first.
pub fn accounts_of(host: &dyn ConnectHost, id: &str) -> Accounts {
    let Some(i) = catalog::integration_of(host, connect_dir(host), id) else { return Accounts::default() };
    let list = i.accounts();
    Accounts {
        count: list.len(),
        active: list.first().map(|a| a.id.clone()),
        problem: list.first().is_some_and(|a| a.status.is_some()),
    }
}

/// Runs one event through the dialog and its effects.
pub fn connect_event(host: &mut dyn ConnectHost, event: Event, tx: &mut Tx) {
    let (effects, moved, scope) = {
        let Some(open) = host.connect_mut().dialog_mut() else { return };
        let scope = open.scope.clone();
        let (next, effects) = dialog::transition(&open.dialog, event);
        // A transition to another step (or method, or form) starts its inputs afresh.
        let moved = steps::inputs_id(&open.dialog) != steps::inputs_id(&next);
        open.dialog = next;
        (effects, moved, scope)
    };
    if moved {
        fresh_inputs(host);
        let in_models = host.connect().dialog().and_then(|o| o.dialog.provider.as_ref()).is_some_and(|p| matches!(p.step, Step::Models));
        if in_models {
            tx.focus("connect-models-search", false);
        } else if host.connect().dialog().is_some_and(|o| o.dialog.provider.is_some()) {
            tx.focus("connect-focus-host", false);
        }
    }
    for effect in effects {
        run::run_effect(host, effect, scope.as_deref(), tx);
    }
}

/// A new step's inputs start empty, its first one taking the keys.
fn fresh_inputs(host: &mut dyn ConnectHost) {
    let Some(open) = host.connect_mut().dialog_mut() else { return };
    let value = steps::field_default(&open.dialog);
    let in_models = open.dialog.provider.as_ref().is_some_and(|p| matches!(p.step, Step::Models));
    open.field = focused(&value);
    open.entry = focused("");
    if in_models {
        open.search = focused("");
        open.active = None;
    } else {
        open.search.focused = open.dialog.provider.is_none();
    }
    open.menu = None;
}

/// The catalogs changed: the dialog follows the accounts and Claude Agent's login.
pub fn catalogs_changed(host: &mut dyn ConnectHost, tx: &mut Tx) {
    let Some((id, claude)) = host
        .connect()
        .dialog()
        .and_then(|o| o.dialog.provider.as_ref())
        .map(|p| (p.id.clone(), matches!(p.step, Step::Claude { .. })))
    else {
        return;
    };
    let accounts = accounts_of(host, &id);
    connect_event(host, Event::AccountsSynced(accounts.clone()), tx);
    if claude {
        let models = !connection_models(host, &id).is_empty();
        connect_event(host, Event::ClaudeStatus { problem: accounts.problem, models }, tx);
    }
}

/// The models a connection to `id` offers: its providers' (`id` or integrating as `id`).
pub fn connection_models(host: &dyn ConnectHost, id: &str) -> Vec<Model> {
    let infos = catalog::provider_infos(host, connect_dir(host)).unwrap_or_default();
    let of = |provider: &str| provider == id || infos.iter().any(|p| p.id == provider && p.integration.as_deref() == Some(id));
    host.models().iter().filter(|m| of(&m.provider)).cloned().collect()
}

/// Whether the connection offers any models to pick from (`controller.ts:hasModels`).
pub fn has_models(host: &dyn ConnectHost, provider: &str) -> bool {
    !steps::connection_groups(host, provider).is_empty()
}

/// Display name for integration/provider `id` in the open dialog's location.
pub fn provider_name(host: &dyn ConnectHost, id: &str) -> String {
    let dir = connect_dir(host);
    catalog::integration_of(host, dir, id)
        .map(|i| i.name.clone())
        .or_else(|| catalog::provider_infos(host, dir).and_then(|ps| ps.iter().find(|p| p.id == id).map(|p| p.name.clone())))
        .unwrap_or_else(|| id.to_owned())
}

/// The dialog's keys: Escape closes it (or its open account menu), and the list's arrows and
/// Enter pick a provider.
pub fn key(host: &mut dyn ConnectHost, ev: &KeyEvent, tx: &mut Tx) -> bool {
    if welcome::key(host, ev, tx) || usage_limit::key(host, ev, tx) || unpaid::key(host, ev, tx) {
        return true;
    }
    let (menu_open, in_picker, in_models) = match host.connect().dialog() {
        Some(open) => (
            open.menu.is_some(),
            open.dialog.provider.is_none(),
            open.dialog.provider.as_ref().is_some_and(|p| matches!(p.step, Step::Models)),
        ),
        None => return false,
    };
    if ev.key == Key::Escape {
        if menu_open {
            tx.instant();
            if let Some(open) = host.connect_mut().dialog_mut() {
                open.menu = None;
            }
            return true;
        }
        close(host, tx);
        return true;
    }
    if in_picker {
        return view::picker_key(host, ev, tx);
    }
    if in_models {
        return steps::models::models_key(host, ev, tx);
    }
    false
}
