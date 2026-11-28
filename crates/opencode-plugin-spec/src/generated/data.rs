//! Data the host and plugins pass each other.
//! Generated from crates/opencode-plugin-spec/contract.json by `cargo run -p opencode-plugin-spec -- --write`. Do not edit.

#![allow(unused_imports)]

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::*;
use crate::{ColorSpec, impl_record};

/// What a handler receives.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct UiEvent {
    /// What happened.
    pub kind: UiEventKind,
    /// The `key` of the element the event came from.
    pub key: Option<String>,
    /// A `hover` event: true as the pointer enters, false as it leaves.
    pub entered: Option<bool>,
    /// A `submit` event: the field's text.
    pub text: Option<String>,
    /// A `change` event: the new state.
    pub checked: Option<bool>,
    /// A `select` event: the picked path.
    pub path: Option<String>,
    /// A `change` event of `ui.segmented`: the picked item's id.
    pub value: Option<String>,
    /// A `visible-rows` event: the first visible row, from 1.
    pub first: Option<i64>,
    /// A `visible-rows` event: the last visible row.
    pub last: Option<i64>,
    /// Shift was held.
    pub shift: Option<bool>,
    /// Alt (Option) was held.
    pub alt: Option<bool>,
    /// The platform's command key was held: Ctrl, or Cmd on macOS.
    pub r#mod: Option<bool>,
}
impl_record!(UiEvent, ["kind", "key", "entered", "text", "checked", "path", "value", "first", "last", "shift", "alt", "mod"], [], []);

/// Offsets from the parent's edges for an out-of-flow node. Give one or two per axis.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct UiPin {
    /// From the left edge.
    pub left: Option<f64>,
    /// From the top edge.
    pub top: Option<f64>,
    /// From the right edge.
    pub right: Option<f64>,
    /// From the bottom edge.
    pub bottom: Option<f64>,
}
impl_record!(UiPin, ["left", "top", "right", "bottom"], [], []);

/// One item of `ui.segmented`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct UiSegmentedItem {
    /// What `onChange` receives.
    pub id: String,
    /// Its icon.
    pub icon: String,
    /// Its accessible name and tooltip.
    pub label: String,
    /// Its element's key. Defaults to `id`.
    pub key: Option<String>,
}
impl_record!(UiSegmentedItem, ["id", "icon", "label", "key"], [], []);

/// One option of `ui.select`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct UiSelectOption {
    /// What `onChange` receives in `event.value`.
    pub value: String,
    /// The option's visible text.
    pub label: String,
}
impl_record!(UiSelectOption, ["value", "label"], [], []);

/// The panels' shared file sidebar.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct PanelSidebarInfo {
    /// It shows.
    pub opened: bool,
    /// Its width in points.
    pub width: f64,
    /// The edge of the panel it sits on. With `end`, the toggle is at the panel's top end corner: leave it room at the end of a header or toolbar under it, not at the start.
    pub side: PanelSidebarSide,
}
impl_record!(PanelSidebarInfo, ["opened", "width", "side"], [], []);

/// The interface language.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct LocaleInfo {
    /// The app's id for it, such as `"de"` or `"zht"`: the name of its catalog file, `locales/<id>.json`.
    pub id: String,
    /// Its BCP 47 tag, such as `"de-DE"` or `"zh-Hant"`.
    pub tag: String,
    /// Its name in itself, such as `"Deutsch"`.
    pub name: String,
    /// Which way it reads.
    pub direction: TextDirection,
}
impl_record!(LocaleInfo, ["id", "tag", "name", "direction"], [], []);

/// How `I18n:Number` writes a number.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct NumberFormat {
    /// Defaults to `decimal`.
    pub style: Option<NumberStyle>,
    /// Defaults to `standard`.
    pub notation: Option<NumberNotation>,
    /// An ISO 4217 code for `currency`. Defaults to `"USD"`.
    pub currency: Option<String>,
    /// The most fraction digits. Defaults to 3 for `decimal`, 2 for `currency`, 0 for `percent` and `compact`.
    pub max_fraction: Option<i64>,
    /// For `currency`, formats with the interface language's localized currency pattern (such as `1.234,50 $` or `1,84 USD`) instead of prefixing the narrow currency symbol. Defaults to false.
    pub locale_pattern: Option<bool>,
}
impl_record!(NumberFormat, ["style", "notation", "currency", "maxFraction", "localePattern"], [], []);

/// A session's token counts at its last step.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct SessionUsage {
    /// Input tokens.
    pub input: i64,
    /// Output tokens.
    pub output: i64,
    /// Reasoning tokens.
    pub reasoning: i64,
    /// Tokens read from the cache.
    pub cache_read: i64,
    /// Tokens written to the cache.
    pub cache_write: i64,
}
impl_record!(SessionUsage, ["input", "output", "reasoning", "cacheRead", "cacheWrite"], [], []);

/// A session as the app holds it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Session {
    /// The session id.
    pub id: String,
    /// The key of the server that owns it: a `ServerInfo.key`, `local` for the app's own server.
    pub server_id: String,
    /// Its workspace folder.
    pub directory: String,
    /// Its title.
    pub title: String,
    /// The parent session of a subagent's session.
    pub parent_id: Option<String>,
    /// The project it belongs to. Nil for a folder outside any project.
    pub project_id: Option<String>,
    /// The agent it runs with, such as `build`.
    pub agent: Option<String>,
    /// The model variant it runs with, such as `high`. Nil for the model's default.
    pub variant: Option<String>,
    /// A turn is running.
    pub busy: bool,
    /// Changes whenever anything here does: put it in a memo's version.
    pub version: i64,
    /// Changes when its newest messages or its inbox's waiting input do, not as older ones are read or the reader scrolls: the version for a read of its newest messages.
    pub messages_version: i64,
    /// What it cost in USD, its subagents' sessions included.
    pub cost: f64,
    /// Token counts at the last step with tokens.
    pub usage: Option<SessionUsage>,
    /// The provider of the step `usage` came from.
    pub usage_provider_id: Option<String>,
    /// The model of the step `usage` came from.
    pub usage_model_id: Option<String>,
    /// The id of the message `usage` came from: the last step with tokens after the last finished compaction and before a staged revert.
    pub usage_message_id: Option<String>,
    /// The oldest message Desktop's timeline would hold: its first read on opening the session, a page of 20 per approach of the top since, and every message after. `usage` and the Context tab's counts come from these messages, as Desktop's do. Empty while that is not known yet; nil before this window opened the session (every message counts).
    pub window_start: Option<String>,
    /// When the message `usage` came from was created, in milliseconds since the Unix epoch.
    pub usage_created: Option<f64>,
    /// The provider of the model the session runs with.
    pub provider_id: Option<String>,
    /// The model the session runs with.
    pub model_id: Option<String>,
    /// When it was created, in milliseconds since the Unix epoch.
    pub created: Option<f64>,
    /// When it last changed, in milliseconds since the Unix epoch.
    pub updated: Option<f64>,
    /// How many messages the app holds for it. Nil before it has read them.
    pub messages: Option<i64>,
    /// The HTTP status code of the session's last assistant message failure, when it failed with one (such as 402 or 429).
    pub last_error_status: Option<i64>,
}
impl_record!(Session, ["id", "serverId", "directory", "title", "parentId", "projectId", "agent", "variant", "busy", "version", "messagesVersion", "cost", "usage", "usageProviderId", "usageModelId", "usageMessageId", "windowStart", "usageCreated", "providerId", "modelId", "created", "updated", "messages", "lastErrorStatus"], [], []);

/// A model as the server's catalog names it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ModelInfo {
    /// The provider's name. Nil when the catalog does not list the provider.
    pub provider_name: Option<String>,
    /// The model's name. Nil when the catalog does not list it.
    pub model_name: Option<String>,
    /// Its context window in tokens.
    pub context_limit: Option<i64>,
    /// Its output limit in tokens.
    pub output_limit: Option<i64>,
    /// Any of its token prices is above zero.
    pub paid: Option<bool>,
}
impl_record!(ModelInfo, ["providerName", "modelName", "contextLimit", "outputLimit", "paid"], [], []);

/// An entry in the host's model catalog (`Models:List`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ModelEntry {
    /// The provider id.
    pub provider_id: String,
    /// The model id.
    pub model_id: String,
    /// Its display name.
    pub name: String,
    /// The provider's display name.
    pub provider_name: Option<String>,
    /// The provider's integration id, when connected through an integration.
    pub integration: Option<String>,
    /// Whether the model is marked free.
    pub free: bool,
    /// Whether any of its token prices is above zero.
    pub paid: bool,
}
impl_record!(ModelEntry, ["providerId", "modelId", "name", "providerName", "integration", "free", "paid"], [], []);

/// A short inline annotation a plugin attaches to a model row in the model picker (`Models:SetBadges`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ModelBadge {
    /// The provider id.
    pub provider_id: String,
    /// The model id.
    pub model_id: String,
    /// Short text shown beside the model name and in its effort flyout.
    pub label: String,
    /// Emphasizes the annotation in danger ink (for example, a reached limit or blocked model).
    pub critical: Option<bool>,
}
impl_record!(ModelBadge, ["providerId", "modelId", "label", "critical"], [], []);

/// The model selected in the composer.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ComposerModel {
    /// The provider id.
    pub provider_id: String,
    /// The model id.
    pub model_id: String,
    /// Its display name.
    pub name: String,
}
impl_record!(ComposerModel, ["providerId", "modelId", "name"], [], []);

/// A cached server read.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Resource {
    /// Its state.
    pub status: ResourceStatus,
    /// The answer as JSON, when it is JSON and the read didn't ask for `items`.
    pub json: Option<Value>,
    /// For a read that asked for them, the items its answer lists (`{ "data": [...] }` or `[...]`), split off the UI thread; `json` is the rest of the answer then.
    #[serde(default, deserialize_with = "crate::lenient::opt_seq")]
    pub items: Option<Vec<ResourceItem>>,
    /// The answer as text.
    pub text: Option<String>,
    /// Why it failed.
    pub error: Option<String>,
    /// Changes with every new answer: put it in a memo's version.
    pub generation: i64,
}
impl_record!(Resource, ["status", "json", "items", "text", "error", "generation"], [], []);

/// One item a read's answer lists.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ResourceItem {
    /// Its `id`.
    pub id: String,
    /// Its `type`.
    pub kind: String,
    /// Its `time.created`.
    pub created: Option<f64>,
    /// Its JSON.
    pub text: String,
}
impl_record!(ResourceItem, ["id", "kind", "created", "text"], [], []);

/// A server's answer.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ServerResponse {
    /// A 2xx status.
    pub ok: bool,
    /// The HTTP status. 0 when nothing answered.
    pub status: i64,
    /// The body as JSON, when it is JSON.
    pub json: Option<Value>,
    /// The body as text.
    pub text: String,
    /// Why it failed.
    pub error: Option<String>,
}
impl_record!(ServerResponse, ["ok", "status", "json", "text", "error"], [], []);

/// One event from the server's event stream.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ServerEvent {
    /// The event type, such as `"session.updated"`.
    pub r#type: String,
    /// The event's payload.
    pub data: Value,
    /// The key of the server that sent it: a `ServerInfo.key`, `local` for the app's own server.
    pub server_id: String,
    /// The workspace folder it concerns.
    pub directory: Option<String>,
}
impl_record!(ServerEvent, ["type", "data", "serverId", "directory"], [], []);

/// A server the app lists.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ServerInfo {
    /// Its key: `local` for the app's own server, `<plugin>:<id>` for a contributed server, or the address of a server added by address.
    pub key: String,
    /// Its display name.
    pub name: String,
    /// Its badge, such as `"SSH"`.
    pub label: Option<String>,
    /// Where it stands.
    pub state: ServerState,
    /// It runs on this machine.
    pub is_local: bool,
    /// This window works with it now: the server of the session or draft it shows, or of its Home.
    pub current: bool,
    /// The version it reported.
    pub version: Option<String>,
}
impl_record!(ServerInfo, ["key", "name", "label", "state", "isLocal", "current", "version"], [], []);

/// A server's latest health check.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ServerHealth {
    /// It answered.
    pub healthy: bool,
    /// The version it reported.
    pub version: Option<String>,
    /// This app does not support that version.
    pub incompatible: Option<bool>,
    /// A check is in flight.
    pub checking: Option<bool>,
}
impl_record!(ServerHealth, ["healthy", "version", "incompatible", "checking"], [], []);

/// How to reach a ready server.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ServerEndpoint {
    /// The base URL.
    pub url: String,
    /// The HTTP basic password. The user name is always `opencode`.
    pub password: Option<String>,
}
impl_record!(ServerEndpoint, ["url", "password"], [], []);

/// One server a server source lists. Its key is `<plugin>:<id>`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ServerEntry {
    /// The entry's id within the source.
    pub id: String,
    /// The display name.
    pub name: String,
    /// A short badge after the name, such as `"WSL"`.
    pub label: Option<String>,
    /// Where it stands.
    pub state: ServerState,
    /// False keeps it out of the app's server list. Settings still shows it. Defaults to true.
    pub listed: Option<bool>,
    /// The plugin keeps it reachable, as an SSH tunnel does: while the app checks a new endpoint, it shows a connecting spinner, not an unknown dot. Defaults to false.
    pub managed: Option<bool>,
    /// Its endpoint, once `ready`.
    pub http: Option<ServerEndpoint>,
}
impl_record!(ServerEntry, ["id", "name", "label", "state", "listed", "managed", "http"], [], []);

/// HTTP basic credentials.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct BasicAuth {
    /// The user name.
    pub user: String,
    /// The password.
    pub password: String,
}
impl_record!(BasicAuth, ["user", "password"], [], []);

/// An HTTP request.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct HttpRequest {
    /// The URL.
    pub url: String,
    /// Defaults to GET.
    pub method: Option<HttpMethod>,
    /// Request headers.
    pub headers: Option<BTreeMap<String, String>>,
    /// A text body.
    pub body: Option<String>,
    /// A JSON body, instead of `body`. Sets the content type.
    pub json: Option<Value>,
    /// Sends `Authorization: Basic`.
    pub basic_auth: Option<BasicAuth>,
    /// Seconds before it gives up. Defaults to 30.
    pub timeout: Option<f64>,
}
impl_record!(HttpRequest, ["url", "method", "headers", "body", "json", "basicAuth", "timeout"], [], []);

/// An HTTP answer.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct HttpResponse {
    /// A 2xx status.
    pub ok: bool,
    /// The HTTP status. 0 when nothing answered.
    pub status: i64,
    /// Response headers, with lowercase names.
    pub headers: BTreeMap<String, String>,
    /// The body as text.
    pub text: String,
    /// The body as JSON, when it is JSON.
    pub json: Option<Value>,
    /// Why it failed.
    pub error: Option<String>,
}
impl_record!(HttpResponse, ["ok", "status", "headers", "text", "json", "error"], [], []);

/// A process to start.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ProcessSpec {
    /// The program, as a path or a name on PATH.
    pub program: String,
    /// Its arguments, passed as they are, without a shell.
    #[serde(default, deserialize_with = "crate::lenient::opt_seq")]
    pub args: Option<Vec<String>>,
    /// Variables to add to the app's environment.
    pub env: Option<BTreeMap<String, String>>,
    /// Its working folder.
    pub cwd: Option<String>,
    /// Written to its stdin first. Stdin then closes unless `keepStdin` is set.
    pub stdin: Option<String>,
    /// Keeps stdin open for `Write`.
    pub keep_stdin: Option<bool>,
    /// Seconds before the host kills it.
    pub timeout: Option<f64>,
    /// How it encodes its output. Defaults to UTF-8, or UTF-16LE when its first output reads as that.
    pub encoding: Option<TextEncoding>,
    /// Sends stderr to a file rather than a pipe, read back as one `stderr` chunk as it exits: for programs that never exit after writing to a stderr pipe, as Windows' OpenSSH does after a failure.
    pub stderr_file: Option<bool>,
}
impl_record!(ProcessSpec, ["program", "args", "env", "cwd", "stdin", "keepStdin", "timeout", "encoding", "stderrFile"], [], []);

/// How a process ended.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ProcessExit {
    /// Its exit code, when it has one.
    pub code: Option<i64>,
    /// Why it could not start.
    pub error: Option<String>,
    /// Its timeout killed it.
    pub timed_out: bool,
    /// `Kill`, or the plugin going away, ended it.
    pub killed: bool,
}
impl_record!(ProcessExit, ["code", "error", "timedOut", "killed"], [], []);

/// A finished process and all its output.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ProcessOutput {
    /// It ran and exited with 0.
    pub ok: bool,
    /// Its exit code.
    pub code: Option<i64>,
    /// Everything it wrote to stdout.
    pub stdout: String,
    /// Everything it wrote to stderr.
    pub stderr: String,
    /// Why it could not start.
    pub error: Option<String>,
    /// Its timeout killed it.
    pub timed_out: bool,
}
impl_record!(ProcessOutput, ["ok", "code", "stdout", "stderr", "error", "timedOut"], [], []);

/// A button on a toast.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ToastAction {
    /// The button's label.
    pub label: String,
    /// Shows this path in the system's file manager.
    pub reveal_path: Option<String>,
    /// Opens this URL in the system's browser.
    pub open_url: Option<String>,
    /// Its look: `primary` (the default) or `secondary`.
    pub variant: Option<ToastActionVariant>,
}
impl_record!(ToastAction, ["label", "revealPath", "openUrl", "variant"], ["run"], []);

/// What a toast shows.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ToastSpec {
    /// The headline.
    pub title: String,
    /// More detail under the title.
    pub description: Option<String>,
    /// The tone. Defaults to `default`.
    pub variant: Option<ToastVariant>,
    /// An icon before the title.
    pub icon: Option<String>,
    /// Seconds before it closes. Ignored when `persistent`. Defaults to 5.
    pub duration: Option<f64>,
    /// Stays until the user or `Dismiss` closes it.
    pub persistent: Option<bool>,
    /// Buttons, in order.
    #[serde(default, deserialize_with = "crate::lenient::opt_seq")]
    pub actions: Option<Vec<ToastAction>>,
}
impl_record!(ToastSpec, ["title", "description", "variant", "icon", "duration", "persistent", "actions"], [], []);

/// Where to open one of this plugin's popovers.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct PopoverOpenSpec {
    /// The popover's id, from `ctx:AddPopover`.
    pub id: String,
    /// The `key` of the element it opens beside.
    pub anchor_key: String,
    /// The `key` of the button that opens it, which takes focus back when it closes (Kobalte's `onCloseAutoFocus`), however it was opened. Defaults to what had focus when it opened.
    pub trigger_key: Option<String>,
    /// The anchor's side. Defaults to below.
    pub side: Option<Side>,
    /// Alignment along that side. Defaults to start.
    pub align: Option<Align>,
    /// Points between the anchor and the popover.
    pub gap: Option<f64>,
    /// Passed to the popover's render as `props.data`.
    pub data: Option<BTreeMap<String, Value>>,
    /// How it appears.
    pub transition: Option<Transition>,
}
impl_record!(PopoverOpenSpec, ["id", "anchorKey", "triggerKey", "side", "align", "gap", "data", "transition"], [], []);

/// One entry of a menu.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct MenuEntry {
    /// Defaults to `item`.
    pub kind: Option<MenuEntryKind>,
    /// An item's id, returned when the user picks it.
    pub id: Option<String>,
    /// An item's label, or a group's title.
    pub title: Option<String>,
    /// An item's icon.
    pub icon: Option<String>,
    /// Muted text at an item's end.
    pub detail: Option<String>,
    /// A published command id whose shortcut the item shows.
    pub keybind: Option<String>,
    /// A literal shortcut the item shows, such as `"mod+plus"`, when no command has it.
    pub keys: Option<String>,
    /// False shows the item disabled. Defaults to true.
    pub enabled: Option<bool>,
    /// A destructive item.
    pub danger: Option<bool>,
    /// Shows a check mark.
    pub checked: Option<bool>,
}
impl_record!(MenuEntry, ["kind", "id", "title", "icon", "detail", "keybind", "keys", "enabled", "danger", "checked"], [], []);

/// A menu to open beside an element.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct MenuOpenSpec {
    /// The `key` of the element it opens beside.
    pub anchor_key: String,
    /// Its entries.
    #[serde(deserialize_with = "crate::lenient::seq")]
    pub items: Vec<MenuEntry>,
    /// Its accessible name.
    pub label: Option<String>,
    /// The anchor's side. Defaults to below.
    pub side: Option<Side>,
    /// Alignment along that side. Defaults to start.
    pub align: Option<Align>,
    /// Its minimum width in points.
    pub min_width: Option<f64>,
    /// Its width in points.
    pub width: Option<f64>,
}
impl_record!(MenuOpenSpec, ["anchorKey", "items", "label", "side", "align", "minWidth", "width"], [], []);

/// A new session's draft, opened with `Layout:OpenDraft`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct DraftSpec {
    /// The server it runs on: a `ServerInfo.key`. Defaults to the server this window works with.
    pub server_key: Option<String>,
    /// Its workspace folder on that server.
    pub directory: String,
    /// Text typed into its composer.
    pub prompt: Option<String>,
}
impl_record!(DraftSpec, ["serverKey", "directory", "prompt"], [], []);

/// How to open a panel tab.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct OpenPanelOptions {
    /// The session. Defaults to the routed session.
    pub session_id: Option<String>,
    /// The tab of an instance panel.
    pub instance: Option<String>,
    /// Opens the tab without selecting it or switching the narrow-screen view.
    pub background: Option<bool>,
    /// How the side region opens.
    pub transition: Option<Transition>,
}
impl_record!(OpenPanelOptions, ["sessionId", "instance", "background", "transition"], [], []);

/// A local link the app routes to a link handler.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Link {
    /// The target: a path, a URL or a `file://` link.
    pub href: String,
    /// The plugin that produced the linked item.
    pub origin: Option<String>,
    /// The path is a known workspace file, not a guess from text.
    pub exact: Option<bool>,
    /// Must not switch the narrow-screen view.
    pub background: Option<bool>,
    /// The workspace-relative folder the link was written in.
    pub base: Option<String>,
    /// The session the link belongs to.
    pub session_id: Option<String>,
}
impl_record!(Link, ["href", "origin", "exact", "background", "base", "sessionId"], [], []);

/// A system notification. It follows the user's notification settings.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct NotificationSpec {
    /// The title.
    pub title: String,
    /// The text under it.
    pub body: Option<String>,
    /// Clicking it opens this session.
    pub session_id: Option<String>,
    /// Plays the user's notification sound.
    pub sound: Option<bool>,
}
impl_record!(NotificationSpec, ["title", "body", "sessionId", "sound"], [], []);

/// The window's theme.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ThemeInfo {
    /// The theme's key.
    pub id: String,
    /// Its display name.
    pub name: String,
    /// The color scheme in use.
    pub scheme: ColorScheme,
}
impl_record!(ThemeInfo, ["id", "name", "scheme"], [], []);

/// Where a file was saved.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct SaveResult {
    /// Its path.
    pub path: Option<String>,
    /// Why it could not be saved.
    pub error: Option<String>,
}
impl_record!(SaveResult, ["path", "error"], [], []);

/// A file's text.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ReadResult {
    /// Its text, as UTF-8.
    pub text: Option<String>,
    /// Why it could not be read.
    pub error: Option<String>,
}
impl_record!(ReadResult, ["text", "error"], [], []);

/// An app that can open a path, such as an editor or a file manager.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct OpenInApp {
    /// Its id, for `Workspace:OpenIn`.
    pub id: String,
    /// Its display name.
    pub name: String,
}
impl_record!(OpenInApp, ["id", "name"], [], ["icon"]);

/// A diff's size.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct DiffTotals {
    /// Changed files.
    pub files: i64,
    /// Added lines.
    pub added: i64,
    /// Removed lines.
    pub removed: i64,
}
impl_record!(DiffTotals, ["files", "added", "removed"], [], []);

/// One changed file.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct DiffFile {
    /// Its path.
    pub path: String,
    /// `added`, `modified`, `deleted` or `renamed`.
    pub status: String,
    /// Added lines.
    pub added: i64,
    /// Removed lines.
    pub removed: i64,
}
impl_record!(DiffFile, ["path", "status", "added", "removed"], [], []);

/// Which changes to open.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct DiffOpenSpec {
    /// The session.
    pub session_id: String,
    /// What to compare.
    pub source: DiffSource,
    /// A `turn` source's user message.
    pub message_id: Option<String>,
}
impl_record!(DiffOpenSpec, ["sessionId", "source", "messageId"], [], []);

/// A terminal session to start on the server.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct TerminalSpec {
    /// The workspace folder.
    pub directory: String,
    /// The shell's working folder. Defaults to `directory`.
    pub cwd: Option<String>,
    /// Its initial title.
    pub title: Option<String>,
    /// A program to run instead of the user's shell.
    pub command: Option<String>,
    /// Its arguments.
    #[serde(default, deserialize_with = "crate::lenient::opt_seq")]
    pub args: Option<Vec<String>>,
}
impl_record!(TerminalSpec, ["directory", "cwd", "title", "command", "args"], [], []);

/// One terminal of a session, as its tab lists it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct TerminalInfo {
    /// The server's id for it.
    pub id: String,
    /// Its title, as the shell or a rename set it. Empty before the server names it.
    pub title: String,
    /// Its place among the session's terminals, from 1: what a default title says.
    pub number: i64,
    /// Listed only to hold a restored tab until the server lists the session's terminals.
    pub hidden: bool,
}
impl_record!(TerminalInfo, ["id", "title", "number", "hidden"], [], []);

/// How `Workspace:OpenFile` opens a file.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct OpenFileOptions {
    /// Opens it in the preview tab, which the next preview replaces, instead of a kept one.
    pub preview: Option<bool>,
    /// The first line to select and scroll into view, from 1.
    pub start_line: Option<i64>,
    /// The last line to select. Defaults to `startLine`.
    pub end_line: Option<i64>,
}
impl_record!(OpenFileOptions, ["preview", "startLine", "endLine"], [], []);

/// The terminal tab `Terminal:OpenAsync` showed.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct TerminalOpened {
    /// The terminal's id; nil when the session has none and can't start one.
    pub id: Option<String>,
    /// Where a session's first terminal opens (Settings > Terminal): `side` or `bottom`.
    pub placement: String,
}
impl_record!(TerminalOpened, ["id", "placement"], [], []);

/// A page to open.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct BrowserOpenSpec {
    /// The URL, or a local `:port`. Defaults to `about:blank`.
    pub url: String,
    /// The session whose side region gets the page's tab. Defaults to the routed session.
    pub session_id: Option<String>,
    /// Opens the tab without moving the keyboard: focus stays where it is.
    pub background: Option<bool>,
    /// The side-region pane (`side`, `split`, or `bottom`) to open the page's tab in.
    pub pane: Option<String>,
}
impl_record!(BrowserOpenSpec, ["url", "sessionId", "background", "pane"], [], []);

/// Where to show a page's tab.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct BrowserShowOptions {
    /// The session whose side region shows it. Defaults to the routed session.
    pub session_id: Option<String>,
    /// Selects the tab without moving the keyboard.
    pub background: Option<bool>,
}
impl_record!(BrowserShowOptions, ["sessionId", "background"], [], []);

/// One page of a session, as its tab lists it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct BrowserTabInfo {
    /// The host's id for it.
    pub id: String,
    /// Its address; empty for a blank page.
    pub url: String,
    /// Its title; empty before the page names itself.
    pub title: String,
    /// The page exists. A stored tab the engine never opened in this run is listed, not open.
    pub open: bool,
}
impl_record!(BrowserTabInfo, ["id", "url", "title", "open"], [], []);

/// Where the browser engine stands.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct BrowserEngine {
    /// Its state.
    pub state: BrowserEngineState,
    /// Why it failed, for `failed`.
    pub message: Option<String>,
    /// The engine that runs (or last ran) keeps a page's downloads for `BrowserTab:Files`; false before one has run, and for an engine from before downloads.
    pub downloads: bool,
    /// The engine that runs (or last ran) confines frame and sub-resource requests to the page's URL policy; false before one has run, and for an engine from before URL policy.
    pub policy: bool,
}
impl_record!(BrowserEngine, ["state", "message", "downloads", "policy"], [], []);

/// An element the user picked in a page with the element picker.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct BrowserPick {
    /// The page.
    pub tab_id: String,
    /// The element's `@eN` reference.
    pub r#ref: String,
    /// A CSS selector for it.
    pub selector: String,
}
impl_record!(BrowserPick, ["tabId", "ref", "selector"], [], []);

/// Where the app updater stands.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct UpdaterState {
    /// The phase.
    pub phase: UpdaterPhase,
    /// The available version.
    pub version: Option<String>,
    /// Download progress, 0 to 1.
    pub progress: Option<f64>,
    /// The release notes, as Markdown.
    pub notes: Option<String>,
    /// Why it failed.
    pub error: Option<String>,
}
impl_record!(UpdaterState, ["phase", "version", "progress", "notes", "error"], [], []);

/// A QR code's module matrix.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct QrCode {
    /// Modules per side.
    pub size: i64,
    /// `size * size` modules, row by row: true is dark.
    #[serde(deserialize_with = "crate::lenient::seq")]
    pub modules: Vec<bool>,
    /// Its dark modules inside a 4-module quiet zone, as an icon `ui.icon` draws crisp at any size in its color.
    pub icon: String,
}
impl_record!(QrCode, ["size", "modules", "icon"], [], []);

/// One line of the Studio console.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ConsoleLine {
    /// The plugin that wrote it.
    pub plugin: String,
    /// How serious it is.
    pub level: ConsoleLevel,
    /// What it says. An error's traceback follows its first line.
    pub message: String,
    /// The `.luau` file the message names, such as `init.luau`.
    pub file: Option<String>,
    /// The line in `file`.
    pub line: Option<i64>,
    /// A `file://` link to that line, when the plugin runs from a folder.
    pub href: Option<String>,
    /// How many times it was written in a row.
    pub count: i64,
}
impl_record!(ConsoleLine, ["plugin", "level", "message", "file", "line", "href", "count"], [], []);

/// What `Console:Eval` gave back.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ConsoleEvalResult {
    /// It ran without an error.
    pub ok: bool,
    /// Its value as text.
    pub value: Option<String>,
    /// The error it raised.
    pub error: Option<String>,
}
impl_record!(ConsoleEvalResult, ["ok", "value", "error"], [], []);

/// A plugin's live costs.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct PluginMetrics {
    /// Luau heap in KB.
    pub memory_kb: f64,
    /// The last render's cost in microseconds.
    pub render_us: f64,
    /// Share of memo lookups that reused their subtree, 0 to 1.
    pub memo_hit_rate: f64,
}
impl_record!(PluginMetrics, ["memoryKb", "renderUs", "memoHitRate"], [], []);

/// One tab an instance panel lists.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct PanelTab {
    /// The instance id.
    pub instance: String,
    /// The accessible name, and the label without a `label` render.
    pub title: String,
    /// Listed before every other tab, never closed or dragged.
    pub pinned: Option<bool>,
    /// Struck through, such as a deleted file.
    pub missing: Option<bool>,
    /// The workspace file the tab shows.
    pub file: Option<String>,
    /// Its icon, instead of the panel's. A tab showing `file` defaults to the file's type icon.
    pub icon: Option<String>,
    /// A launcher tab: the next preview replaces it, and closing it keeps nothing in Recently closed.
    pub transient: Option<bool>,
    /// Drags between panes. Defaults to true.
    pub draggable: Option<bool>,
    /// Selected when the selected tab closes.
    pub fallback: Option<bool>,
    /// Listed but not drawn in the strip: it holds a restored selection until the panel can show it.
    pub hidden: Option<bool>,
}
impl_record!(PanelTab, ["instance", "title", "pinned", "missing", "file", "icon", "transient", "draggable", "fallback", "hidden"], [], []);

/// What a titlebar item shows.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct TitlebarItemView {
    /// The visible label.
    pub label: String,
    /// The accessible name, when it differs from the label.
    pub title: Option<String>,
    /// Its icon. Defaults to a download arrow.
    pub icon: Option<String>,
    /// Shows a spinner and disables it.
    pub busy: Option<bool>,
    /// Shows it pressed.
    pub pressed: Option<bool>,
}
impl_record!(TitlebarItemView, ["label", "title", "icon", "busy", "pressed"], [], []);

/// One row of the menu a composer pill opens.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ComposerPillRow {
    /// Its id within the pill.
    pub id: String,
    /// Its text.
    pub label: String,
    /// Its icon.
    pub icon: Option<String>,
    /// Colors its icon.
    pub tone: Option<Tone>,
    /// Muted text at its end.
    pub detail: Option<String>,
}
impl_record!(ComposerPillRow, ["id", "label", "icon", "tone", "detail"], ["run"], []);

/// What a composer pill shows above one session's composer.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ComposerPillView {
    /// The capsule's text.
    pub label: String,
    /// The capsule's icon.
    pub icon: Option<String>,
    /// Colors the icon.
    pub tone: Option<Tone>,
    /// The accessible name, when it differs from `label`. It names the popover `renderMenu` fills, which has no heading of the host's.
    pub title: Option<String>,
    /// The menu it opens, when the pill has no `renderMenu`.
    #[serde(default, deserialize_with = "crate::lenient::opt_seq")]
    pub rows: Option<Vec<ComposerPillRow>>,
}
impl_record!(ComposerPillView, ["label", "icon", "tone", "title", "rows"], [], []);

/// An MCP tool result with an image: a text part and the image.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct McpImage {
    /// The text part, such as JSON metadata.
    pub text: String,
    /// The image, base64-encoded.
    pub image: String,
    /// Its type, such as `image/png`.
    pub mime_type: String,
}
impl_record!(McpImage, ["text", "image", "mimeType"], [], []);

/// Where an MCP tool call comes from.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct McpCallContext {
    /// The session whose agent called it.
    pub session_id: String,
    /// That session's workspace folder.
    pub directory: String,
}
impl_record!(McpCallContext, ["sessionId", "directory"], [], []);
