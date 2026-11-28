# Brocode plugin API reference (contract 1.13)

> Generated from crates/opencode-plugin-spec/contract.json by `cargo run -p opencode-plugin-spec -- --write`. Do not edit. For a walkthrough, read [Writing a plugin](writing-a-plugin.md).

Contents: [Versioning](#versioning) · [Modules and globals](#modules-and-globals) · [Contexts](#contexts) · [Contributions](#contributions) · [Services](#services) · [UI nodes](#ui-nodes) · [Semantic tokens](#semantic-tokens) · [Enums](#enums) · [Types](#types) · [plugin.toml](#plugintoml)

## Versioning

Contract versions are `MAJOR.MINOR`. This is contract `1.13`.

- A plugin names the contract it targets in plugin.toml: `contract = "1.0"`.
- Minor versions (1.1, 1.2, …) only add: new optional properties on `ui.*` nodes and specs, new semantic tokens (each with a 1.0 fallback token), new services, methods, signals, contribution kinds and enum values. A plugin that targets 1.0 runs unchanged on every 1.x host.
- A host runs a plugin whose contract has the same major version. When the plugin targets a newer minor version than the host, the host still loads it and logs one warning in the console.
- Unknown optional keys in a `ui.*` props table or a spec table log one warning per key in the console and fall back to the default. They never raise an error.
- A deprecated property, method or token carries `@deprecated` in opencode.d.luau and in the docs, and keeps working for the whole 1.x line. Only contract 2.0 can remove it.
- Names never change meaning within a major version: a token, enum value or method keeps its behavior and its type.

## Modules and globals

### `require("@opencode/plugin")`

| Member | Type | Description |
|---|---|---|
| `.contract` | `string` | The contract version the host implements, such as `"1.0"`. |
| `.defineWindow` | `(setup: (ctx: WindowContext) -> ()) -> PluginHalf` | Defines the window half: `setup` runs once per window. Return the result from `init.luau`. |
| `.defineApp` | `(setup: (ctx: AppContext) -> ()) -> PluginHalf` | Defines the app half: `setup` runs once per process, before any window half. Return the result from `app.luau`. |

### `require("@opencode/ui")`

Every [UI node](#ui-nodes) constructor, and:

| Member | Type | Description |
|---|---|---|
| `.hex` | `(color: string) -> HexColor` | A literal color from `#RRGGBB` or `#RRGGBBAA`. Prefer a semantic token, which follows the theme. |
| `.each` | `<T>(items: { T }, key: string \| ((item: T, index: number) -> string \| number), version: string \| ((item: T, index: number) -> MemoVersion), build: (item: T, index: number) -> UiNode) -> { UiNode }` | A node for each item, as children to pass on: `build(item, index)`, memoized per item as `view:Memo` is, by its key and version. A render builds again only the items whose version changed. `key` and `version` are field names (`"id"`) or functions of the item. An item's key becomes its node's key, so keep keys unique in the render. Put everything the row draws with, the theme among it, in its version. |

The module also exports the `Ui*` types without their prefix: `ui.Node`, `ui.BoxProps`, `ui.Event`. Every other contract type is a global type.

### `task`

Cooperative scheduling on the host's frame clock. Waiting costs no CPU.

| Member | Type | Description |
|---|---|---|
| `.spawn` | `<A..., R...>(callback: ((A...) -> R...) \| thread, A...) -> thread` | Runs a function or thread now, until it first yields. |
| `.defer` | `<A..., R...>(callback: ((A...) -> R...) \| thread, A...) -> thread` | Runs a function or thread at the end of the current handler or event batch, before the frame commits. |
| `.delay` | `<A..., R...>(seconds: number, callback: ((A...) -> R...) \| thread, A...) -> thread` | Runs a function or thread after `seconds`. |
| `.wait` | `(seconds: number?) -> number` | Yields for `seconds`, or until the next frame without it. Returns the seconds that passed. Not allowed in a render. Yields. |
| `.cancel` | `(thread: thread) -> ()` | Cancels a scheduled or waiting thread and its pending request. |

### `warn`

Writes a warning to the console.

`warn(...any) -> ()`

## Contexts

What a plugin half's setup function receives.

### `WindowContext`

| Member | Type | Description |
|---|---|---|
| `id` | `string` | The plugin's id. |
| `half` | `Half` | Which half this is. |
| `contract` | `string` | The contract version the host implements, such as `"1.0"`. |
| `CreateSignal` | `<T...>() -> ScriptSignal<T...>` | A signal this plugin fires itself. |
| `CreateEditor` | `(initial: string?) -> Editor` | A host-owned text field's content. The n-th editor a setup creates is the same field after a hot reload. |
| `BindHotState` | `(save: () -> any, load: (state: any) -> ()) -> ()` | Carries plain data across a hot reload: the host calls `save` on the old code and `load` on the new. |
| `GetService` | `(name) -> <Name>Service` | One of: [`Sessions`](#sessions), [`Models`](#models), [`Server`](#server), [`Servers`](#servers), [`Http`](#http), [`Process`](#process), [`Store`](#store), [`Contracts`](#contracts), [`Console`](#console), [`Toast`](#toast), [`Popover`](#popover), [`Dialog`](#dialog), [`Menu`](#menu), [`Layout`](#layout), [`Links`](#links), [`Commands`](#commands), [`Theme`](#theme), [`App`](#app), [`System`](#system), [`Notifications`](#notifications), [`Workspace`](#workspace), [`Diff`](#diff), [`Terminal`](#terminal), [`Browser`](#browser), [`Composer`](#composer), [`Office`](#office), [`Updater`](#updater), [`Diagnostics`](#diagnostics), [`Power`](#power), [`Qr`](#qr), [`Images`](#images), [`I18n`](#i18n). |
| `AddCommand` | `(spec: CommandSpec) -> Contribution` | Adds a command to the palette, its keybind and its slash command. |
| `AddPanel` | `(spec: PanelSpec) -> Contribution` | Adds a panel: tabs in the session's side region. |
| `AddSetting` | `(spec: SettingSpec) -> Contribution` | Adds a row to Settings › Plugins. |
| `AddSettingsPage` | `(spec: SettingsPageSpec) -> Contribution` | Adds a settings page, or a section on a host page. |
| `AddMenuItem` | `(spec: MenuItemSpec) -> Contribution` | Adds an item to a host menu. |
| `AddLinkHandler` | `(spec: LinkHandlerSpec) -> Contribution` | Opens local links it matches. |
| `AddTitlebarItem` | `(spec: TitlebarItemSpec) -> Contribution` | Adds a pill at the titlebar's end. |
| `AddComposerPill` | `(spec: ComposerPillSpec) -> Contribution` | Adds a pill above the session composer. |
| `AddCodeBlock` | `(spec: CodeBlockSpec) -> Contribution` | Renders fenced code blocks of some languages. |
| `AddCodeBlockAction` | `(spec: CodeBlockActionSpec) -> Contribution` | Adds a button beside code blocks' Copy button. |
| `AddSlot` | `(spec: SlotSpec) -> Contribution` | Fills one of the shell's slots. |
| `AddTheme` | `(spec: ThemeSpec) -> Contribution` | Adds a color theme. |
| `AddFileViewer` | `(spec: FileViewerSpec) -> Contribution` | Shows files with some extensions. |
| `AddServerSource` | `(spec: ServerSourceSpec) -> ServerSource` | Adds a source of servers. Add it in the app half so every window shares one list. |
| `AddDialog` | `(spec: DialogSpec) -> Contribution` | Adds a modal dialog, opened with `Dialog:Open`. |
| `AddPopover` | `(spec: PopoverSpec) -> Contribution` | Adds popover content, opened with `Popover:Open` or `ui.flyoutRow`. |
| `AddMcpTool` | `(spec: McpToolSpec) -> Contribution` | Adds a tool to the app's client MCP server. |
| `Disposing` | `Signal<()>` | Fires as the half goes away: the plugin turns off, reloads, or its window closes. The host disconnects every connection afterwards. |
| `FocusChanged` | `Signal<boolean>` | Fires as the window gains or loses focus. |

### `AppContext`

| Member | Type | Description |
|---|---|---|
| `id` | `string` | The plugin's id. |
| `half` | `Half` | Which half this is. |
| `contract` | `string` | The contract version the host implements, such as `"1.0"`. |
| `CreateSignal` | `<T...>() -> ScriptSignal<T...>` | A signal this plugin fires itself. |
| `BindHotState` | `(save: () -> any, load: (state: any) -> ()) -> ()` | Carries plain data across a hot reload: the host calls `save` on the old code and `load` on the new. |
| `GetService` | `(name) -> <Name>Service` | One of: [`Sessions`](#sessions), [`Models`](#models), [`Server`](#server), [`Servers`](#servers), [`Http`](#http), [`Process`](#process), [`Store`](#store), [`Contracts`](#contracts), [`Console`](#console), [`Toast`](#toast), [`Theme`](#theme), [`App`](#app), [`System`](#system), [`Notifications`](#notifications), [`Workspace`](#workspace), [`Terminal`](#terminal), [`Updater`](#updater), [`Diagnostics`](#diagnostics), [`Power`](#power), [`Qr`](#qr), [`Askpass`](#askpass), [`I18n`](#i18n). |
| `AddMenubarItem` | `(spec: MenubarItemSpec) -> Contribution` | Adds an item to the app menu bar. |
| `AddServerSource` | `(spec: ServerSourceSpec) -> ServerSource` | Adds a source of servers. Add it in the app half so every window shares one list. |
| `AddMcpTool` | `(spec: McpToolSpec) -> Contribution` | Adds a tool to the app's client MCP server. |
| `Disposing` | `Signal<()>` | Fires as the half goes away: the plugin turns off, reloads, or its window closes. The host disconnects every connection afterwards. |

## Contributions

Every `Add*` method returns a handle the host withdraws with the plugin half. Callbacks run on the UI thread; render callbacks (`render`, `label`, `show`, `match`) must not yield, and the rest run as coroutines that may.

| Kind | Method | Halves | Spec | Desktop's name |
|---|---|---|---|---|
| `command` | `AddCommand` | window | [`CommandSpec`](#commandspec) | Command |
| `panel` | `AddPanel` | window | [`PanelSpec`](#panelspec) | Panel |
| `setting` | `AddSetting` | window | [`SettingSpec`](#settingspec) | — |
| `settingsPage` | `AddSettingsPage` | window | [`SettingsPageSpec`](#settingspagespec) | SettingsPage |
| `menuItem` | `AddMenuItem` | window | [`MenuItemSpec`](#menuitemspec) | MenuItem |
| `menubarItem` | `AddMenubarItem` | app | [`MenubarItemSpec`](#menubaritemspec) | MenubarItem |
| `linkHandler` | `AddLinkHandler` | window | [`LinkHandlerSpec`](#linkhandlerspec) | LinkHandler |
| `titlebarItem` | `AddTitlebarItem` | window | [`TitlebarItemSpec`](#titlebaritemspec) | TitlebarItem |
| `composerPill` | `AddComposerPill` | window | [`ComposerPillSpec`](#composerpillspec) | ComposerPill |
| `codeBlock` | `AddCodeBlock` | window | [`CodeBlockSpec`](#codeblockspec) | CodeBlock |
| `codeBlockAction` | `AddCodeBlockAction` | window | [`CodeBlockActionSpec`](#codeblockactionspec) | CodeBlockAction |
| `slot` | `AddSlot` | window | [`SlotSpec`](#slotspec) | Slot |
| `theme` | `AddTheme` | window | [`ThemeSpec`](#themespec) | Theme |
| `fileViewer` | `AddFileViewer` | window | [`FileViewerSpec`](#fileviewerspec) | FileViewer |
| `serverSource` | `AddServerSource` | window, app | [`ServerSourceSpec`](#serversourcespec) | Server |
| `dialog` | `AddDialog` | window | [`DialogSpec`](#dialogspec) | — |
| `popover` | `AddPopover` | window | [`PopoverSpec`](#popoverspec) | — |
| `mcpTool` | `AddMcpTool` | window, app | [`McpToolSpec`](#mcptoolspec) | McpTool |

## Services

Get one with `ctx:GetService("Name")`. Methods named `…Async` yield: call them from a handler, a task or a signal callback, never from a render.

### Sessions

The sessions the app holds. Halves: window, app.

| Member | Type | Description |
|---|---|---|
| `:Get` | `(sessionId: string) -> Session?` | A session by id. |
| `:Routed` | `() -> Session?` | The session this window shows. Always nil in the app half. |
| `:Open` | `() -> { Session }` | The sessions of open tabs. |
| `.Changed` | `Signal<Session>` | Fires with a session after it changes. |
| `.RoutedChanged` | `Signal<Session?>` | Fires as this window routes another session, or none. |

### Models

The server's model catalog. Halves: window, app.

| Member | Type | Description |
|---|---|---|
| `:Get` | `(providerId: string, modelId: string) -> ModelInfo` | How the catalog names a model. |
| `:List` | `() -> { ModelEntry }` | Every model currently in the host's catalog. Since 1.5. |
| `:SetBadges` | `(badges: { ModelBadge }) -> ()` | Replaces this plugin's inline model-picker badges. Pass an empty list to clear them. Window halves only. Since 1.5. |
| `.Changed` | `Signal<()>` | Fires when the catalog changes. |

### Server

The OpenCode server this window works with: the server of the session or draft it shows, so a panel in a tab from another server talks to that server. A path that starts with `local:` goes to the app's own server. In the app half, the app's own server. Halves: window, app.

| Member | Type | Description |
|---|---|---|
| `:RequestAsync` | `(method: HttpMethod, path: string, body: any?) -> ServerResponse` | Sends a request with a JSON body, off the UI thread. Yields. |
| `:Resource` | `(path: string, version: MemoVersion, items: boolean?) -> Resource` | A GET of `path`, cached until `version` changes. Safe in a render: it never yields, and the window redraws when the answer lands. With `items`, a list's answer comes split into `items` off the UI thread instead of decoded into `json`. |
| `:Invalidate` | `(prefix: string) -> ()` | Drops this plugin's cached reads whose path starts with `prefix`. |
| `:OnEvent` | `(prefix: string) -> Signal<ServerEvent>` | A signal for the server's events whose type starts with `prefix`. |
| `:EncodeUri` | `(text: string) -> string` | Percent-encodes text for a path or query. |
| `:Version` | `() -> string?` | The version it reported. |
| `:IsLocal` | `() -> boolean` | It runs on this machine. |
| `:IsLive` | `() -> boolean` | The window works with a live server rather than an offline fixture. Since 1.5. |
| `:CanWrite` | `() -> boolean` | The app may change things on it. |
| `.Connected` | `Signal<boolean>` | Fires as its event connection goes up or down. |

### Servers

The servers the app lists. Halves: window, app.

| Member | Type | Description |
|---|---|---|
| `:List` | `() -> { ServerInfo }` | Every listed server. |
| `:Current` | `() -> string?` | The key of the server this window works with now. |
| `:Health` | `(key: string) -> ServerHealth?` | A server's latest health check. |
| `:Connect` | `(key: string) -> ()` | Shows a listed server on Home. Every listed server stays connected, so nothing switches and no tab closes. |
| `.Changed` | `Signal<()>` | Fires when the list or a server's state changes. |

### Http

HTTP requests off the UI thread. Halves: window, app.

| Member | Type | Description |
|---|---|---|
| `:FetchAsync` | `(request: HttpRequest) -> HttpResponse` | Sends a request. Yields. |

### Process

Child processes the host supervises. Halves: window, app.

| Member | Type | Description |
|---|---|---|
| `:Spawn` | `(spec: ProcessSpec) -> ProcessHandle` | Starts a process. A program that cannot start still returns a handle, whose `Exited` fires with the error. |
| `:RunAsync` | `(spec: ProcessSpec) -> ProcessOutput` | Starts a process and yields until it exits, with all its output. Yields. |

### Store

Values the host saves for this plugin in the app's database. Both halves and every window share one copy. Every value is in memory before the plugin's setup runs, so a read never waits and has the saved value. A `Set` changes the value at once; it reaches the disk within 250 ms, and always when a window hides or closes and when the app quits. Names starting `rune.` are the host's own. Halves: window, app.

| Member | Type | Description |
|---|---|---|
| `:Value` | `<T>(name: string, default: T) -> StoreValue<T>` | A global value. |
| `:SessionValue` | `<T>(name: string, default: T) -> SessionStoreValue<T>` | A value per session, kept apart for each server. |

### Contracts

Typed values plugins share. A plugin may provide only ids under its own id. Halves: window, app.

| Member | Type | Description |
|---|---|---|
| `:Provide` | `(id: string, value: any) -> Contribution` | Provides `value` under `id` (`<plugin>` or `<plugin>.<name>`). From the app half, for the whole process; from a window half, for that window. |
| `:Use` | `(id: string) -> Live<any>` | Follows a contract another plugin provides. Keep working while it is not active. |
| `:Summary` | `() -> { [string]: LiveState }` | Every contract id this half sees, with its state. |

### Console

The Studio console. `print` and `warn` write here too. Halves: window, app.

| Member | Type | Description |
|---|---|---|
| `:Log` | `(...any) -> ()` | Writes an information line. |
| `:Warn` | `(...any) -> ()` | Writes a warning. |
| `:Error` | `(...any) -> ()` | Writes an error, without raising one. |
| `:Lines` | `() -> { ConsoleLine }` | Every plugin's lines, oldest first; a repeat of the last line counts up. Read in a render, it redraws as lines come. |
| `:Clear` | `() -> ()` | Drops every line. |
| `:Eval` | `(pluginId: string, source: string) -> ConsoleEvalResult` | Runs Luau `source` in the window half of plugin `pluginId`, as Studio's command bar does: an expression's value, or a chunk's return value, as text. Local and beta builds only; elsewhere it fails. |

### Toast

Short messages in a corner of the window. From the app half, in the focused window. Halves: window, app.

| Member | Type | Description |
|---|---|---|
| `:Show` | `(spec: ToastSpec) -> ToastHandle` | Shows a toast. |
| `:Info` | `(title: string, description: string?) -> ToastHandle` | Shows a `default` toast. |
| `:Success` | `(title: string, description: string?) -> ToastHandle` | Shows a `success` toast. |
| `:Error` | `(title: string, description: string?) -> ToastHandle` | Shows an `error` toast that stays until dismissed. |

### Popover

This plugin's popovers. The host places them, moves focus and closes them. Halves: window.

| Member | Type | Description |
|---|---|---|
| `:Open` | `(spec: PopoverOpenSpec) -> ()` | Opens a popover beside an element, closing any other of this plugin's. |
| `:Toggle` | `(spec: PopoverOpenSpec) -> ()` | Opens it, or closes it when it is open. |
| `:Close` | `() -> ()` | Closes this plugin's open popover. |
| `:IsOpen` | `(id: string) -> boolean` | Whether a popover is open. |

### Dialog

This plugin's modal dialogs. Halves: window.

| Member | Type | Description |
|---|---|---|
| `:Open` | `(id: string, data: { [string]: any }?) -> ()` | Opens a dialog, closing any other of this plugin's. |
| `:Close` | `() -> ()` | Closes this plugin's open dialog. |
| `:IsOpen` | `(id: string) -> boolean` | Whether a dialog is open. |

### Menu

The app's own menus. Halves: window.

| Member | Type | Description |
|---|---|---|
| `:OpenAsync` | `(spec: MenuOpenSpec) -> string?` | Opens a menu beside an element and yields until it closes. Returns the picked item's id, or nil. Yields. |

### Layout

Panels, settings and drafts in this window. Halves: window.

| Member | Type | Description |
|---|---|---|
| `:OpenPanel` | `(panelId: string, options: OpenPanelOptions?) -> ()` | Opens or selects one of this plugin's panels. |
| `:TogglePanel` | `(panelId: string, options: OpenPanelOptions?) -> ()` | Opens a panel, or closes it when it shows. |
| `:ClosePanel` | `(panelId: string, options: OpenPanelOptions?) -> ()` | Closes a panel's tab. |
| `:PanelState` | `(panelId: string, sessionId: string?) -> PanelState` | Where a panel is in a session. Defaults to the routed session. |
| `:OpenSettings` | `(pageId: string?) -> ()` | Opens Settings at one of this plugin's pages, or at the top. |
| `:NewDraft` | `(directory: string, prompt: string?) -> ()` | Opens a new session's draft in a folder, with a prompt typed in. |
| `:OpenDraft` | `(spec: DraftSpec) -> ()` | Opens a new session's draft in a folder on a listed server, with a prompt typed in. Does nothing while the server is not listed. |
| `:OpenProject` | `(serverKey: string, title: string?) -> ()` | Asks for a folder on a server and opens a draft there, as Add project does: the system's folder picker for this machine's server, the server's own folder browser for another. A server that is not listed yet (one the plugin just started) gets 30 seconds to be listed; a newer request replaces a waiting one. |
| `:Narrow` | `() -> boolean` | The window is narrow-screen sized. |
| `:PanelSidebar` | `() -> PanelSidebarInfo` | The file sidebar `ui.panelSidebar` draws. |
| `:TogglePanelSidebar` | `() -> ()` | Shows or hides it. |
| `:MovePanelSidebar` | `(side: PanelSidebarSide) -> ()` | Puts it on an edge of the panel, for every side panel. Since 1.9. |
| `:OpenDrawerView` | `(title: string, data: { [string]: any }?) -> ()` | From a `drawer` kind panel's view in a narrow window's More drawer: shows a nested view in the drawer in its place (Desktop's `useDrawer().open`), with a back button named after the panel's view. The panel's render draws it, with `props.drawer` set to `data`. Does nothing elsewhere. |
| `:RegionOpen` | `(sessionId: string?) -> boolean` | A session's side region is open. Defaults to the routed session. |
| `:RegionFocused` | `() -> boolean` | The side region has the keyboard: the last focus, click or key was in it, as Desktop's side focus reads (`#review-panel[data-focused]`). Read it when a command runs: Desktop's Open browser shortcut focuses the new tab's address only from inside the region. |
| `:Focus` | `(key: string) -> ()` | Moves the keyboard to the element with `key`, or to its first focusable control, and draws its focus ring: where a shortcut leaves the keyboard. Official's Toggle browser toolbar shortcut leaves it on the Options button. Since 1.7. |

### Links

Routes local links to the plugins that handle them. Halves: window.

| Member | Type | Description |
|---|---|---|
| `:Open` | `(link: Link) -> boolean` | Opens a link with the best handler. False when no handler matches. |
| `:Exists` | `(link: Link) -> boolean` | Whether a handler matches and says the target exists. |

### Commands

Published commands, as the palette runs them. Halves: window.

| Member | Type | Description |
|---|---|---|
| `:Run` | `(commandId: string, input: string?) -> boolean` | Runs a command by id: `<plugin>.<id>` or one of the app's. False when it is unknown or disabled. |
| `:Keybind` | `(commandId: string) -> string?` | A command's effective keybind. |

### Theme

The theme and the user's appearance settings. Halves: window, app.

| Member | Type | Description |
|---|---|---|
| `:Current` | `() -> ThemeInfo` | The theme in use. |
| `:IsDark` | `() -> boolean` | The dark scheme is in use. |
| `:Resolve` | `(color: Color) -> string` | A color as `#RRGGBBAA` for the current theme. |
| `:Font` | `(family: FontFamily) -> string` | The user's font family name. |
| `.Changed` | `Signal<ThemeInfo>` | Fires when the theme or scheme changes. |

### App

The running app. Halves: window, app.

| Member | Type | Description |
|---|---|---|
| `.Version` | `string` | The app's version. |
| `.Channel` | `Channel` | Its release channel. |
| `.Platform` | `Platform` | The operating system. |
| `.Commit` | `string?` | The commit this build was made from, when the build knows it. |
| `.BetaNotes` | `any?` | The changes a local or beta build carries (Desktop's `OPENCODE_BETA_NOTES`): `{ sha, commits, notes }`, each note `{ type, summary, sha, pr?, author? }`, newest first; nil in other builds. |
| `:Locale` | `() -> string` | The interface language, such as `"en"`. |
| `:Arg` | `(name: string) -> string?` | The value of command-line option `--name`. |
| `:LogsDir` | `() -> string?` | The folder the app writes its logs to. |
| `:DownloadsDir` | `() -> string?` | The folder the app saves downloads to. |
| `.LocaleChanged` | `Signal<string>` | Fires with the new interface language. |

### System

The clipboard, the system browser and file manager, and the clock. Halves: window, app.

| Member | Type | Description |
|---|---|---|
| `:CopyText` | `(text: string) -> ()` | Puts text on the clipboard. |
| `:OpenExternal` | `(url: string) -> ()` | Opens a URL in the system's browser. |
| `:Reveal` | `(path: string) -> ()` | Shows a path in the system's file manager. |
| `:Now` | `() -> number` | The wall clock in milliseconds since the Unix epoch. |
| `:UtcOffset` | `(ms: number) -> number` | The local time zone's offset from UTC in seconds, at that time. |
| `:FreePort` | `() -> number?` | A TCP port nothing listens on now on the loopback address, for a server or tunnel a plugin starts. Nil when the system has none. |
| `:RandomId` | `() -> string` | A random version 4 UUID from the system's secure source, for passwords and tokens. |

### Notifications

System notifications that follow the user's notification settings. Halves: window, app.

| Member | Type | Description |
|---|---|---|
| `:Notify` | `(spec: NotificationSpec) -> ()` | Shows a notification when the window is not in front. |

### Workspace

Files in the workspace and the apps that open them. Halves: window, app.

| Member | Type | Description |
|---|---|---|
| `:ReadFileAsync` | `(path: string) -> ReadResult` | Reads a text file through the server. Yields. |
| `:SaveDownloadAsync` | `(name: string, content: string) -> SaveResult` | Saves content as a file in the downloads folder. Yields. |
| `:OpenDocument` | `(path: string) -> CodeDocument` | Opens a file for `ui.codeView`. |
| `:OpenTree` | `(directory: string) -> FileTreeModel` | Opens a folder for `ui.fileTree`. |
| `:OpenInApps` | `() -> { OpenInApp }` | The installed apps that can open a path. |
| `:OpenIn` | `(appId: string, path: string) -> ()` | Opens a path in one of those apps. |
| `:Relative` | `(path: string, directory: string) -> string?` | A path inside a workspace folder, relative to it with `/` between parts; nil for a path outside it. A relative path stays as it is. |
| `:OpenFile` | `(sessionId: string, path: string, options: OpenFileOptions?) -> ()` | Opens workspace file `path` in a tab of the session's side region, leaving the keyboard where it was. |
| `.FileChanged` | `Signal<string>` | Fires with a workspace path after it changes on disk. |

### Diff

Session changes for `ui.diffView`. Halves: window.

| Member | Type | Description |
|---|---|---|
| `:Open` | `(spec: DiffOpenSpec) -> DiffDocument` | Opens a set of changes. |
| `:Mode` | `() -> DiffMode` | The mode diffs draw in now. A `SetMode` or `ui.diffView` `mode` change lands with a later frame, so what sizes around the diff reads this to change with it. |
| `:SetMode` | `(mode: DiffMode) -> ()` | Draws every diff in `mode`. |

### Terminal

Terminal sessions on the server, for `ui.terminalSurface`. Halves: window, app.

| Member | Type | Description |
|---|---|---|
| `:Create` | `(spec: TerminalSpec) -> PtySession` | Starts a terminal session. |
| `:Attach` | `(id: string) -> PtySession?` | Reattaches to a running session by id. |
| `:List` | `(sessionId: string, open: { string }) -> { TerminalInfo }` | Session `sessionId`'s terminals in strip order. Until the server lists them, the stored tabs `open` names, hidden; after a list that failed, those tabs shown. |
| `:Available` | `() -> boolean` | This window can start terminals: its server has them and the window isn't read-only. |
| `:New` | `(sessionId: string?) -> ()` | Starts a terminal in the session (the routed one by default) and opens its tab beside its others, or where Settings place a first one; it takes the keyboard. |
| `:Toggle` | `() -> ()` | `terminal.toggle`: a new terminal when the routed session has none; the active terminal's pane hides while its tab shows; otherwise its tab opens and takes the keyboard. |
| `:Select` | `(sessionId: string, id: string, focus: boolean) -> ()` | Makes terminal `id` the session's active one, and with `focus` gives it the keyboard. |
| `:OpenAsync` | `(sessionId: string, newTerminal: boolean) -> TerminalOpened` | Shows session `sessionId`'s terminal tab, starting a terminal when `newTerminal` or it has none, and leaves the keyboard where it was. Yields. |

### Browser

Pages in this window's embedded browser, each a tab of a session's side region. Halves: window.

| Member | Type | Description |
|---|---|---|
| `:OpenAsync` | `(spec: BrowserOpenSpec) -> BrowserTab` | Opens a page in a new tab of a session's side region, and returns it once its tab is there. Yields. |
| `:Tabs` | `(sessionId: string?) -> { BrowserTab }` | Session `sessionId`'s open pages, oldest first; without a session, every open page. |
| `:Get` | `(id: string) -> BrowserTab?` | An open page by id. |
| `:List` | `(sessionId: string, open: { string }) -> { BrowserTabInfo }` | The pages of the stored tabs `open` names, in that order, for a panel's `list`. |
| `:Active` | `(sessionId: string) -> BrowserTab?` | The page a session shows: its selected browser tab, else its newest open page, else the page the engine shows. |
| `:Selected` | `(sessionId: string?) -> BrowserTab?` | The page whose tab is selected in a session's open side region (the routed session by default). |
| `:Focused` | `() -> BrowserTab?` | The routed session's selected page while the keyboard is in its pane: its page, or a field of this plugin's. |
| `:Available` | `() -> boolean` | Pages can open here: this build has a browser engine for its platform. |
| `:Engine` | `() -> BrowserEngine` | Where the browser engine stands. |
| `:NormalizeUrl` | `(url: string, sessionId: string?) -> string` | Normalizes a URL for `sessionId`'s URL policy (adding `http://` or `https://` when omitted), or raises the agent refusal message when the URL is not supported. |
| `:Prepare` | `(seconds: number?) -> ()` | A page is likely to open within `seconds` (5 by default, at most 60): the pointer is on a launcher for one, or an agent that uses the browser starts a turn. The engine's process starts now, so the page doesn't wait for it, and stops again once `seconds` pass with no page opened; a later call's `seconds` replace the earlier's. Does nothing while a page is open or the engine runs, and never downloads it: call it on a sign of intent, not at launch or on a timer. |
| `.Changed` | `Signal<string>` | Fires with a page's id when its address, title or loading changes, or it closes. |
| `.Picked` | `Signal<BrowserPick>` | Fires when the element picker picks an element. |

### Composer

The active session's or draft's composer. Halves: window.

| Member | Type | Description |
|---|---|---|
| `:InsertPill` | `(id: string, label: string) -> ()` | Puts a pill at the caret, with a space on each side, and gives the composer the keyboard. |
| `:Active` | `() -> boolean` | Whether the current screen has a composer (a draft or a root session). Since 1.5. |
| `:Directory` | `() -> string` | The composer's workspace folder (the active session's, or the draft's). Since 1.5. |
| `:Model` | `() -> ComposerModel?` | The model currently selected in the composer, when listed in the model catalog. Since 1.5. |
| `:Picker` | `() -> ComposerPicker?` | Which composer picker overlay is currently open, or nil when none is open. Since 1.5. |
| `.PickerChanged` | `Signal<ComposerPicker?>` | Fires when a composer picker opens or closes, with the open picker kind or nil. Since 1.5. |

### Office

Office documents laid out and drawn by the host, each on a thread of its own. Halves: window.

| Member | Type | Description |
|---|---|---|
| `:OpenAsync` | `(path: string, directory: string, kind: OfficeKind) -> OfficeFile` | Reads a file of the session's folder `directory` through the server and, when its bytes are a zip archive, opens and lays it out as `kind`. Yields. |
| `:Version` | `(document: OfficeDocument) -> number` | Grows with every change its views draw: pages laid out, zoom, a sheet's frame. Read in a render, it redraws as it grows. |
| `:SetSheet` | `(document: OfficeDocument, index: number) -> ()` | Shows a workbook's sheet `index`, from 1. |
| `:Sheet` | `(document: OfficeDocument) -> number` | The workbook's sheet on show, from 1: the one `Office:SetSheet` chose, or the one a link in the sheet went to. Reading it redraws the caller when it changes. |
| `:Close` | `(document: OfficeDocument) -> ()` | Releases it and ends its thread. |

### Updater

The app's own updates (Desktop's `Updater` IPC). The host checks, downloads and verifies on its own schedule; a plugin draws where it stands and asks for a check or the install. Halves: window, app.

| Member | Type | Description |
|---|---|---|
| `:State` | `() -> UpdaterState?` | Where it stands; nil when no updater runs in this build. Read in a render, it redraws as it moves. |
| `:CheckAsync` | `() -> UpdaterState` | Checks for a newer version now, joining a check already running, and resumes with the state the check settled on: `up-to-date`, `ready`, `download-required`, `error`, or the work under way. Yields. |
| `:InstallAsync` | `() -> string?` | Restarts into the ready version, or opens a required download's page: resumes with nil once it handed off (a restart then quits the app on its own), else why it could not. Yields. |
| `.StateChanged` | `Signal<UpdaterState?>` | Fires with each new state. |

### Diagnostics

Developer diagnostics. Local and beta builds only, except `Get` of an experiment and `HeapSnapshotAsync`; elsewhere the rest return nil. Halves: window, app.

| Member | Type | Description |
|---|---|---|
| `:Memory` | `() -> { [string]: any }?` | The process's memory counters. |
| `:Metrics` | `(pluginId: string?) -> PluginMetrics?` | A plugin's live costs. Defaults to this plugin. |
| `:Plugins` | `() -> { string }` | The ids of the plugins running in this window, for a console's target. |
| `:Get` | `(name: string) -> any` | A developer setting: an experiment (`sources.enabled` `"on"` or `"off"`, `sources.icons` `"site"` or `"letters"`), the animation clock (`speed` a number, `slow` a boolean), the tuning panel (`knobs` a boolean), or memory diagnostics (`memoryLog` `"off"`, `"on"`, or `"env"` when the environment turned them on for this launch), and the layout tools: `direction` (`"ltr"` or `"rtl"`, the window's layout direction) and `forceFocus` (a boolean: every control draws its keyboard focus ring). Read in a render, it redraws as the setting changes. |
| `:Set` | `(name: string, value: any) -> ()` | Changes a setting `Get` reads (`memoryLog` takes a boolean and keeps it across launches; `direction` overrides the language's direction for this launch, and setting the language's own direction drops the override), or opens a developer tool: `components` the components lab. |
| `:HeapSnapshotAsync` | `() -> SaveResult` | Captures heap snapshots in a new folder in the downloads folder: the app's own server's snapshot, where the memory is per process (`memory.json`), and `manifest.json`, written last. `path` is the folder; `error` says why nothing was captured, or which snapshot failed. Every build offers it. Yields. |

### Power

Display sleep. Halves: window, app.

| Member | Type | Description |
|---|---|---|
| `:KeepAwake` | `(reason: string) -> PowerLock` | Keeps the display awake until the lock is released or the plugin goes away. |

### Qr

QR codes. Halves: window, app.

| Member | Type | Description |
|---|---|---|
| `:Encode` | `(text: string) -> QrCode` | Encodes text as UTF-8 bytes at low error correction, in the smallest version that fits. |

### Askpass

Credential prompts from child processes such as `ssh`. Halves: app.

| Member | Type | Description |
|---|---|---|
| `:Listen` | `() -> AskpassListener` | Starts a loopback prompt endpoint. |

### Images

Images for `ui.image`: loaded from a source or handed over, decoded off the UI thread by the window's image cache (PNG and APNG, JPEG, GIF, WebP, BMP, ICO, AVIF, ICNS and SVG). Halves: window.

| Member | Type | Description |
|---|---|---|
| `:Load` | `(src: string) -> ImageId` | The image at `src`, loading off the UI thread: an `https:` or `http:` URL, fetched as a browser loads an image (no credentials, 15 s, at most 8 MB); a path from `/`, read from the window's server with its sign-in (`/api/...`); a `data:` URL; or a file of the plugin's own folder by its relative path (`icons/logo.svg`). The same source is the same image, and its state is shared: one that failed fails again at once. `Images:State` says where it stands. |
| `:State` | `(image: ImageId) -> ImageState` | Where the image stands. Read in a render, it redraws as the image lands or fails. |
| `:VideoFailed` | `(src: string) -> boolean` | Whether the `ui.video` playing `src` can't play: its box then shows what's behind it. Read in a render, it redraws when the clip fails; a render memoized with `view:Memo` should put it in the memo's version, as with `Images:State`. |
| `:FromEncoded` | `(data: buffer \| string) -> ImageId?` | A PNG, JPEG, GIF or WebP image, decoded off the UI thread. Nil when refused. |
| `:FromSvg` | `(svg: string) -> ImageId?` | An SVG image. |
| `:FromRgba` | `(pixels: buffer, width: number, height: number) -> ImageId?` | Straight-alpha RGBA8 pixels. |
| `:Release` | `(image: ImageId) -> ()` | Frees an image once nothing draws it. |

### I18n

The plugin's own messages in the interface language, and numbers, dates and lists written the way the app writes them. A plugin's catalogs are `locales/<id>.json` in its folder: flat objects of message keys, the app's format. Halves: window, app.

| Member | Type | Description |
|---|---|---|
| `:T` | `(key: string, args: { [string]: any }?) -> string` | Message `key` in the interface language, else in `locales/en.json`, else the key itself. `{{name}}` placeholders take `args`, and a missing one reads empty. With a number `args.count`, the form `key.<category>` for the count's plural category is used when the catalog has it, else `key.other`, else `key`. |
| `:Has` | `(key: string) -> boolean` | The interface language's catalog, or the English one, has message `key`. |
| `:Locale` | `() -> LocaleInfo` | The interface language. |
| `:Direction` | `() -> TextDirection` | Which way the interface language reads. Lay out rows from the reading start; the host mirrors its own layout. |
| `:Plural` | `(count: number) -> PluralCategory` | The plural category the interface language uses for `count`. |
| `:Number` | `(value: number, format: NumberFormat?) -> string` | `value` with the language's digits, separators and signs. |
| `:Date` | `(ms: number, style: DateStyle) -> string` | The moment `ms` (milliseconds since the Unix epoch) in local time, in `style`. |
| `:RelativeTime` | `(value: number, unit: TimeUnit) -> string` | A relative time: `-3, "day"` reads "3 days ago", `2, "hour"` "in 2 hours". |
| `:RelativeDay` | `(days: number) -> string?` | "today", "yesterday" or "tomorrow" for 0, -1 and 1 days from today, in the language. Nil for other days. |
| `:List` | `(items: { string }) -> string` | Items joined as the language's long conjunction: "a, b, and c". |
| `.Changed` | `Signal<LocaleInfo>` | Fires as the interface language changes. Everything drawn redraws on its own; connect for state you keep. |

## UI nodes

`ui.*` constructors build host elements straight into the frame's arena. Wrap subtrees in `view:Memo` so unchanged frames call no Luau at all.

| Constructor | Category | Props | Description |
|---|---|---|---|
| `ui.box((props: UiBoxProps, children: { UiNode }?) -> UiNode) & ((style: UiBoxProps, props: UiBoxProps?, children: { UiNode }?) -> UiNode)` | layout | [`UiBoxProps`](#uiboxprops) | A container with no direction of its own: children stack on top of each other. |
| `ui.row((props: UiBoxProps, children: { UiNode }?) -> UiNode) & ((style: UiBoxProps, props: UiBoxProps?, children: { UiNode }?) -> UiNode)` | layout | [`UiBoxProps`](#uiboxprops) | A horizontal stack. |
| `ui.col((props: UiBoxProps, children: { UiNode }?) -> UiNode) & ((style: UiBoxProps, props: UiBoxProps?, children: { UiNode }?) -> UiNode)` | layout | [`UiBoxProps`](#uiboxprops) | A vertical stack. |
| `ui.frame((props: UiFrameProps, children: { UiNode }?) -> UiNode) & ((style: UiFrameProps, props: UiFrameProps?, children: { UiNode }?) -> UiNode)` | layout | [`UiFrameProps`](#uiframeprops) | A layout container whose border takes room in layout, snapped to device pixels at every display scale as a CSS `border-box` border does. |
| `ui.spacer(props: UiSpacerProps?) -> UiNode` | layout | [`UiSpacerProps`](#uispacerprops) | Flexible space in a stack. |
| `ui.divider(props: UiDividerProps?) -> UiNode` | layout | [`UiDividerProps`](#uidividerprops) | A 1 px rule, laid out as a 1px CSS border is: one device pixel up to 1.5x, two at 2x. A box whose height counts its border holds it beside its content. |
| `ui.text((content: string, props: UiTextProps?, ...string \| number) -> UiNode) & ((content: string, style: UiTextProps, props: UiTextProps, ...string \| number) -> UiNode)` | content | [`UiTextProps`](#uitextprops) | Text. Values after the props fill `{1}`, `{2}`... in `content`, written straight into the text: `ui.text("{1} / {2}", VALUE, read, write)`. Other braces stay as they are. |
| `ui.icon((name: IconName, props: UiIconProps?) -> UiNode) & ((name: IconName, style: UiIconProps, props: UiIconProps) -> UiNode)` | content | [`UiIconProps`](#uiiconprops) | One of the app's icons. |
| `ui.image((props: UiImageProps, fallback: UiNode?) -> UiNode) & ((style: UiImageProps, props: UiImageProps, fallback: UiNode?) -> UiNode)` | content | [`UiImageProps`](#uiimageprops) | An image the host holds, or loads from `src`. While it loads its box shows the `placeholder`; `fallback` takes its place when it fails (a site's letter in place of its icon). |
| `ui.video(props: UiVideoProps) -> UiNode` | content | [`UiVideoProps`](#uivideoprops) | A video playing muted on a loop while it shows, as a page's `<video autoplay loop muted playsinline>`. Until its first frame, and if it can't play, its box shows what's behind it. |
| `ui.metric(props: UiMetricProps) -> UiNode` | content | [`UiMetricProps`](#uimetricprops) | A number shown as text. Hosts that can animate changed digits do. |
| `ui.shimmer(content: string, props: UiTextProps?) -> UiNode` | content | [`UiTextProps`](#uitextprops) | Text with the app's loading shimmer. |
| `ui.keybind(props: UiKeybindProps) -> UiNode` | content | [`UiKeybindProps`](#uikeybindprops) | A keyboard shortcut chip. |
| `ui.button(props: UiButtonProps) -> UiNode` | control | [`UiButtonProps`](#uibuttonprops) | The app's own button. |
| `ui.iconButton(props: UiIconButtonProps) -> UiNode` | control | [`UiIconButtonProps`](#uiiconbuttonprops) | The app's own icon button. |
| `ui.switch(props: UiSwitchProps) -> UiNode` | control | [`UiSwitchProps`](#uiswitchprops) | The app's own switch. |
| `ui.checkbox(props: UiCheckboxProps) -> UiNode` | control | [`UiCheckboxProps`](#uicheckboxprops) | Desktop's compact checkbox with its label. Since 1.7. |
| `ui.radio(props: UiRadioProps) -> UiNode` | control | [`UiRadioProps`](#uiradioprops) | A radio row. |
| `ui.field(props: UiFieldProps) -> UiNode` | control | [`UiFieldProps`](#uifieldprops) | The app's own text field. |
| `ui.listbox(props: UiListboxProps?, children: { UiNode }?) -> UiNode` | layout | [`UiListboxProps`](#uilistboxprops) | A listbox container (`role="listbox"`) of selectable `ui.option` rows. Since 1.12. |
| `ui.option(props: UiOptionProps?, children: { UiNode }?) -> UiNode` | layout | [`UiOptionProps`](#uioptionprops) | One selectable option row (`role="option"`) inside a `ui.listbox`. Since 1.12. |
| `ui.select(props: UiSelectProps) -> UiNode` | control | [`UiSelectProps`](#uiselectprops) | Desktop's select dropdown control. Since 1.12. |
| `ui.segmented(props: UiSegmentedProps) -> UiNode` | control | [`UiSegmentedProps`](#uisegmentedprops) | The app's segmented control. |
| `ui.loader(props: UiLoaderProps?) -> UiNode` | control | [`UiLoaderProps`](#uiloaderprops) | The app's spinner. |
| `ui.progressCircle(props: UiProgressCircleProps) -> UiNode` | content | [`UiProgressCircleProps`](#uiprogresscircleprops) | The app's progress ring: a track and an arc from twelve o'clock, clockwise. |
| `ui.serverIndicator(props: UiServerIndicatorProps) -> UiNode` | control | [`UiServerIndicatorProps`](#uiserverindicatorprops) | A server's health indicator, for a server source's row. |
| `ui.pill(props: UiPillProps) -> UiNode` | control | [`UiPillProps`](#uipillprops) | A status capsule. |
| `ui.badge(props: UiBadgeProps) -> UiNode` | control | [`UiBadgeProps`](#uibadgeprops) | Desktop's compact badge. Since 1.11. |
| `ui.settingsRow(props: UiSettingsRowProps, control: UiNode?) -> UiNode` | control | [`UiSettingsRowProps`](#uisettingsrowprops) | A settings row: title and description beside a control. |
| `ui.settingsList(props: UiSettingsListProps, rows: { UiNode }) -> UiNode` | control | [`UiSettingsListProps`](#uisettingslistprops) | A card of settings rows. |
| `ui.flyoutRow(props: UiFlyoutRowProps) -> UiNode` | control | [`UiFlyoutRowProps`](#uiflyoutrowprops) | A popover row that opens a nested popover. |
| `ui.dialogHeader(props: UiDialogHeaderProps) -> UiNode` | control | [`UiDialogHeaderProps`](#uidialogheaderprops) | The app's dialog header: title, description and close button. Its close button closes this plugin's dialog, as Esc does. |
| `ui.markdown(props: UiMarkdownProps) -> UiNode` | native | [`UiMarkdownProps`](#uimarkdownprops) | Markdown, as the timeline draws a reply. |
| `ui.virtualList(props: UiVirtualListProps) -> UiNode` | native | [`UiVirtualListProps`](#uivirtuallistprops) | A long list that builds only its visible rows. |
| `ui.terminalSurface(props: UiTerminalSurfaceProps) -> UiNode` | native | [`UiTerminalSurfaceProps`](#uiterminalsurfaceprops) | A terminal. |
| `ui.terminalTabLabel(props: UiTerminalTabLabelProps) -> UiNode` | native | [`UiTerminalTabLabelProps`](#uiterminaltablabelprops) | A terminal tab's label: its prompt glyph and title, at most 160 wide. |
| `ui.browserSurface(props: UiBrowserSurfaceProps) -> UiNode` | native | [`UiBrowserSurfaceProps`](#uibrowsersurfaceprops) | A web page. |
| `ui.codeView(props: UiCodeViewProps) -> UiNode` | native | [`UiCodeViewProps`](#uicodeviewprops) | A highlighted file. |
| `ui.diffView(props: UiDiffViewProps) -> UiNode` | native | [`UiDiffViewProps`](#uidiffviewprops) | Changes, split or unified. |
| `ui.fileTree(props: UiFileTreeProps) -> UiNode` | native | [`UiFileTreeProps`](#uifiletreeprops) | A folder's file tree. |
| `ui.panelSidebar(props: UiPanelSidebarProps, children: { UiNode }?) -> UiNode` | native | [`UiPanelSidebarProps`](#uipanelsidebarprops) | A resizable sidebar at a panel's start or end, its children stacked in it. |
| `ui.openInButton(props: UiOpenInButtonProps) -> UiNode` | native | [`UiOpenInButtonProps`](#uiopeninbuttonprops) | The app's Open in split button for a path. |
| `ui.officePages(props: UiOfficePagesProps) -> UiNode` | native | [`UiOfficePagesProps`](#uiofficepagesprops) | A Word document's pages or a deck's slides, fit to the pane's width, with zoom controls. |
| `ui.officeSheet(props: UiOfficeSheetProps) -> UiNode` | native | [`UiOfficeSheetProps`](#uiofficesheetprops) | A workbook's sheet: its headers and the cells under the scroll position. |
| `ui.genUi(props: UiGenUiProps) -> UiNode` | native | [`UiGenUiProps`](#uigenuiprops) | An interactive UI outline, from a ```` ```ui ```` code block. |

### Native nodes

Heavy engines stay native in the host. Luau composes them as opaque nodes and drives them through services.

| Node | Engine | Stays native | Moves to Luau |
|---|---|---|---|
| `ui.markdown` | Markdown renderer with Mermaid diagrams | Parsing and AST caching, paragraph shaping, code fences, Mermaid layout | What to show, streaming state, highlights |
| `ui.virtualList` | Virtualized rows | Viewport windowing, height cache, scroll anchoring, incremental mounting | Row count, version and each row's content |
| `ui.terminalSurface` | Terminal emulator over the server's PTY stream | VT parsing, scrollback, cell grid, IME and key encoding, the PTY stream | Tabs and their titles, commands, the + menu row, the narrow view |
| `ui.terminalTabLabel` | The terminal's rename | Double-click and the label's menu rename it in a field over the title | The title it shows |
| `ui.browserSurface` | Embedded browser engine | The out-of-process browser, GPU texture sharing, the DevTools transport, input forwarding | Address bar, history, element picker notes, commands, MCP tools |
| `ui.codeView` | Code view with an asynchronous highlighter | Highlighting, the line buffer, the virtualized gutter, selection, find in file | Tab routing, breadcrumb header, line links, viewer choice, MCP tools |
| `ui.diffView` | Diff engine and code view | Hunk layout, intraline diffing, highlighting, virtualized hunks, comment anchors | Source switcher, totals, split and unified toggle, contracts, MCP tools |
| `ui.fileTree` | Tree model | Tree flattening, file icons, keyboard navigation | Folder loading, filter input, expanded state, open and context menus |
| `ui.panelSidebar` | Pointer capture | The resize drag and the width it leaves | Whether it shows and what it holds |
| `ui.openInButton` | The system's app registry | Finding installed apps and their marks, launching them, the remembered choice | Which path it opens |
| `ui.officePages` | Office engines and a GPU vector renderer | Layout, the page column, scenes drawn at the display's scale, near-viewport loading, zoom | Opening the file, its binary placeholder, the words |
| `ui.officeSheet` | Office engines and a GPU vector renderer | Frames of the sheet drawn as it scrolls, headers, the selection (click, Shift-click, drag, header and corner presses, Excel's keys), copying cells as tab-separated text and an HTML table, and the cell text screen readers read | Opening the file, the sheet tabs, the words |
| `ui.genUi` | UI outline renderer | Outline parsing, widget primitives and their motion | Which blocks render, through `AddCodeBlock` |

## Semantic tokens

A `Color` is one of these tokens, or `ui.hex("#RRGGBB")`. The host resolves tokens for the current theme and color scheme.

| Token | Group | Meaning |
|---|---|---|
| `bg-base` | background | The window's base background. |
| `bg-weak` | background | A background one step quieter than `bg-base`, such as a sidebar. |
| `bg-strong` | background | A background one step stronger than `bg-base`, such as a title bar. |
| `surface-1` | surface | The first raised layer: cards and grouped lists. |
| `surface-2` | surface | The second raised layer: controls on a card. |
| `surface-3` | surface | The third raised layer: a control's track, such as an off switch. |
| `surface-4` | surface | The fourth raised layer: the strongest fill a control uses. |
| `surface-hover` | surface | The fill of a row or button under the pointer. |
| `surface-press` | surface | The fill of a row or button while it is pressed. |
| `surface-selected` | surface | The fill of a selected row or tab. |
| `surface-inverse` | surface | A high-contrast inverse fill, such as a progress bar fill or solid contrast indicator. Falls back to `text-strong` on older hosts. Since 1.5. |
| `text-strong` | text | Headings and the most important text. |
| `text-base` | text | Body text. |
| `text-weak` | text | Secondary text: descriptions and labels. |
| `text-weaker` | text | Tertiary text: hints, placeholders and timestamps. |
| `text-muted` | text | Secondary text in the app's chrome: header figures' labels, menu row names, tab titles. Hosts without a separate chrome scale use `text-weak`. |
| `text-faint` | text | Quiet figures and hints in the app's chrome, such as a header's counts. Hosts without a separate chrome scale use `text-weaker`. |
| `text-chrome` | text | Text in the app's chrome at full strength: menu and popover rows, header titles, list items. Hosts without a separate chrome scale use `text-strong`. |
| `text-on-accent` | text | Text on an `accent` fill. |
| `text-interactive` | text | Links and other interactive text. |
| `icon-strong` | icon | An icon that needs emphasis. |
| `icon-base` | icon | The default icon color. |
| `icon-weak` | icon | A secondary icon. |
| `icon-weaker` | icon | A decorative or disabled icon. |
| `icon-on-accent` | icon | An icon on an `accent` fill. |
| `border-strong` | border | A strong border, such as a focused field's outline. |
| `border-base` | border | The default border. |
| `border-weak` | border | A hairline divider or a card's border. |
| `border-focus` | border | The keyboard focus ring. |
| `accent` | accent | The brand accent: primary buttons and switches that are on. |
| `accent-hover` | accent | An `accent` fill under the pointer. |
| `accent-subtle` | accent | A quiet accent tint, such as a selected chip's fill. |
| `status-success` | status | Done or passing. |
| `status-warning` | status | Needs attention. |
| `status-danger` | status | Failed, blocked or destructive. |
| `status-info` | status | Neutral information. |
| `status-success-bg` | status | A tinted background for success callouts and banners. Falls back to `surface-2` on older hosts. Since 1.5. |
| `status-warning-bg` | status | A tinted background for warning callouts and banners. Falls back to `surface-2` on older hosts. Since 1.5. |
| `status-danger-bg` | status | A tinted background for danger callouts and banners. Falls back to `surface-2` on older hosts. Since 1.5. |
| `status-info-bg` | status | A tinted background for informational callouts and banners. Falls back to `surface-2` on older hosts. Since 1.5. |
| `status-danger-strong` | status | High-contrast danger ink for text and indicators on light or tinted danger surfaces. Falls back to `status-danger` on older hosts. Since 1.5. |
| `avatar-orange` | avatar | A letter tile's fill, as the app's project avatars draw: the first of eight a name hashes to. |
| `avatar-yellow` | avatar | A letter tile's fill, the second of eight. |
| `avatar-cyan` | avatar | A letter tile's fill, the third of eight. |
| `avatar-green` | avatar | A letter tile's fill, the fourth of eight. |
| `avatar-red` | avatar | A letter tile's fill, the fifth of eight. |
| `avatar-pink` | avatar | A letter tile's fill, the sixth of eight. |
| `avatar-blue` | avatar | A letter tile's fill, the seventh of eight. |
| `avatar-purple` | avatar | A letter tile's fill, the eighth of eight. |
| `avatar-text` | avatar | The letter on a tile. |
| `avatar-ring` | avatar | The hairline inside a tile's edge. |

## Enums

### Half

Which half of a plugin runs the code.

| Value | Meaning |
|---|---|
| `"window"` | One instance per window: the plugin's UI. |
| `"app"` | One instance per process: workers, processes and shared state. |

### Platform

The operating system the host runs on.

| Value | Meaning |
|---|---|
| `"windows"` | Windows. |
| `"macos"` | macOS. |
| `"linux"` | Linux. |
| `"web"` | A browser host. |

### Channel

The app build's release channel.

| Value | Meaning |
|---|---|
| `"local"` | A development build. |
| `"dev"` | A dev release build. |
| `"beta"` | A beta build. |
| `"prod"` | A production build. |

### ColorScheme

A light or dark color scheme.

| Value | Meaning |
|---|---|
| `"light"` | Light. |
| `"dark"` | Dark. |

### Tone

A status color for an icon or a pill.

| Value | Meaning |
|---|---|
| `"success"` | Done or passing. |
| `"warning"` | Needs attention. |
| `"danger"` | Failed or blocked. |
| `"accent"` | Under way, in the accent color. |

### ToastVariant

A toast's tone.

| Value | Meaning |
|---|---|
| `"default"` | Neutral information. |
| `"success"` | Something finished. Shows a check icon unless the toast sets `icon`. |
| `"error"` | Something failed. |
| `"loading"` | Something is in progress. Use it with `persistent` and dismiss the toast when the work ends. |

### CommandSection

The section of Settings › Shortcuts that lists a command.

| Value | Meaning |
|---|---|
| `"general"` | App-wide commands. |
| `"session"` | Commands on the current session. |
| `"navigation"` | Moving between views and tabs. |
| `"model"` | Model and agent choice. |
| `"terminal"` | The terminal. |
| `"prompt"` | The composer. |

### PanelMobileKind

Where a panel's narrow-screen view is offered.

| Value | Meaning |
|---|---|
| `"tab"` | A tab of the narrow view switcher, beside Session. It replaces the conversation. |
| `"menu"` | A row of the switcher's More drawer. It replaces the conversation. |
| `"drawer"` | A row of the More drawer that opens inside the drawer, over the conversation. |

### MenuName

A host menu a menu item joins.

| Value | Meaning |
|---|---|
| `"session.panel"` | The + menu before the side panel's tabs, and the New tab view. |
| `"server.add"` | The Add server menu in Settings › Servers. |
| `"server.row"` | The menu of each server row in Settings › Servers. `run` receives the row's server key. |

### MenubarMenu

A submenu of the app menu bar.

| Value | Meaning |
|---|---|
| `"app"` | The app menu. |
| `"file"` | File. |
| `"edit"` | Edit. |
| `"view"` | View. |
| `"go"` | Go. |
| `"window"` | Window. |
| `"help"` | Help. |

### SlotName

A fixed place in the app's shell that a slot fills.

| Value | Meaning |
|---|---|
| `"window.bottom"` | A full-width strip under the window content, above toasts. |
| `"session.header"` | The end of the session header, before the usage numbers. |
| `"session.location.menu"` | Rows in the session's project and location menu. |
| `"session.panel.end"` | The actions after the side panel's tabs. |
| `"session.panel.new"` | The top of the side panel's New tab view, above its launchers. |
| `"session.panel.new.list"` | Below the launchers of the side panel's New tab view, before Recently closed. |

### HostSettingsPage

A host settings page a settings page can add a section to.

| Value | Meaning |
|---|---|
| `"general"` | Settings › General. |
| `"servers"` | Settings › Servers. |

### ServerState

Where a contributed server stands.

| Value | Meaning |
|---|---|
| `"stopped"` | Not running. |
| `"starting"` | Starting up. |
| `"auth"` | Waiting for the user to sign in. |
| `"ready"` | Reachable. `http` holds its endpoint. |
| `"failed"` | Could not start. |
| `"incompatible"` | Runs a version this app does not support. |

### PanelState

Where a plugin's panel tab is in a session.

| Value | Meaning |
|---|---|
| `"closed"` | Not in the session's tab strip. |
| `"listed"` | In the strip, not selected. |
| `"selected"` | Selected in its pane, with the side region hidden. |
| `"visible"` | Selected, with the side region on screen. |

### Transition

How a layout change moves.

| Value | Meaning |
|---|---|
| `"default"` | The app's own motion. |
| `"none"` | Lands at once, without motion. |

### HoverTransition

How hover and press fills change.

| Value | Meaning |
|---|---|
| `"default"` | The app's own fade. |
| `"instant"` | Lands at once. |

### Side

A side of an anchor.

| Value | Meaning |
|---|---|
| `"top"` | Above. |
| `"bottom"` | Below. |
| `"left"` | Before, in left-to-right text. |
| `"right"` | After, in left-to-right text. |

### Align

Alignment along an edge or an axis.

| Value | Meaning |
|---|---|
| `"start"` | The start edge. |
| `"center"` | Centered. |
| `"end"` | The end edge. |

### UiAlign

How a stack places its children across its axis.

| Value | Meaning |
|---|---|
| `"start"` | At the start. |
| `"center"` | Centered. |
| `"end"` | At the end. |
| `"stretch"` | Stretched to fill. The default. |

### UiJustify

How a stack places its children along its axis.

| Value | Meaning |
|---|---|
| `"start"` | Packed at the start. The default. |
| `"center"` | Packed in the center. |
| `"end"` | Packed at the end. |
| `"between"` | The first and last child at the edges, equal space between. |

### UiDirection

How a `ui.frame` container arranges its children.

| Value | Meaning |
|---|---|
| `"col"` | A vertical stack. The default. |
| `"row"` | A horizontal stack. |
| `"box"` | No axis: children stack on top of each other. |

### PanelSidebarSide

The edge of a panel its file sidebar sits on.

| Value | Meaning |
|---|---|
| `"start"` | The inline start: left in a left-to-right layout. |
| `"end"` | The inline end: right in a left-to-right layout. |

### ComposerPicker

Which picker overlay the composer currently has open.

| Value | Meaning |
|---|---|
| `"agent"` | The agent picker. |
| `"model"` | The model picker. |
| `"effort"` | The model effort/variant picker. |
| `"speed"` | The model speed picker, for a model the server offers at faster or slower speeds. Since 1.8. |
| `"project"` | The draft project picker. |

### FontWeight

A text weight.

| Value | Meaning |
|---|---|
| `"regular"` | 400. |
| `"medium"` | 500. |
| `"semibold"` | 600. |
| `"bold"` | 700. |

### FontFamily

One of the user's font choices.

| Value | Meaning |
|---|---|
| `"sans"` | The interface font. |
| `"mono"` | The code font. |

### ButtonVariant

A button's look, as the app's own buttons.

| Value | Meaning |
|---|---|
| `"neutral"` | A filled neutral button. The default. |
| `"contrast"` | A high-contrast filled button. |
| `"primary"` | An `accent` filled button. |
| `"ghost"` | No fill until hovered. |
| `"ghost-muted"` | A ghost button with a muted label. |
| `"outline"` | A bordered button. |
| `"danger"` | A destructive action. |

### ControlSize

A control's size.

| Value | Meaning |
|---|---|
| `"small"` | Compact. |
| `"normal"` | The default. |
| `"large"` | Large. |

### Elevation

A shadow from the app's elevation scale.

| Value | Meaning |
|---|---|
| `"raised"` | Slightly above the page: cards. |
| `"floating"` | Floats over content: popovers and menus. |
| `"overlay"` | Over everything: dialogs. |

### Role

An element's accessibility role.

| Value | Meaning |
|---|---|
| `"none"` | No role. |
| `"group"` | A group of related elements. |
| `"region"` | A landmark region. Give it a `label`. |
| `"button"` | A button. |
| `"link"` | A link. |
| `"switch"` | An on and off switch. |
| `"checkbox"` | A checkbox. |
| `"radio"` | A radio button. |
| `"tab"` | A tab. |
| `"list"` | A list. |
| `"listitem"` | A list item. |
| `"heading"` | A heading. |
| `"img"` | An image. Give it a `label`. |
| `"status"` | A live status message. |
| `"timer"` | A countdown or clock, which assistive technology leaves unannounced as it changes. |
| `"alert"` | An urgent message that assistive technology announces at once, such as a failure. |

### LiveState

The state of a contract another plugin provides.

| Value | Meaning |
|---|---|
| `"pending"` | Its provider is still loading or reloading. |
| `"active"` | Provided. `Get` returns it. |
| `"inactive"` | Its provider is off, absent, or does not run on this platform. |
| `"failed"` | Its provider failed to load or stopped. |

### ResourceStatus

The state of a cached server read.

| Value | Meaning |
|---|---|
| `"loading"` | Not answered yet. Holds the previous answer while a newer read loads. |
| `"ready"` | Answered. |
| `"error"` | Failed. `error` says why. |

### HttpMethod

An HTTP method.

| Value | Meaning |
|---|---|
| `"GET"` | GET. |
| `"POST"` | POST. |
| `"PUT"` | PUT. |
| `"PATCH"` | PATCH. |
| `"DELETE"` | DELETE. |
| `"HEAD"` | HEAD. |

### ProcessStream

A process's output stream.

| Value | Meaning |
|---|---|
| `"stdout"` | Standard output. |
| `"stderr"` | Standard error. |

### TextEncoding

How a process encodes its output.

| Value | Meaning |
|---|---|
| `"utf8"` | UTF-8. The default. |
| `"utf16le"` | UTF-16 little-endian, such as `wsl.exe` on Windows. |

### DiffMode

How a diff lays out its changes.

| Value | Meaning |
|---|---|
| `"split"` | Old and new side by side. |
| `"unified"` | One column. |

### DiffSource

What a diff compares.

| Value | Meaning |
|---|---|
| `"git"` | The working tree against HEAD. |
| `"branch"` | The branch against its merge base. |
| `"turn"` | One session turn's changes. |

### DiffStatus

How a diff's read went.

| Value | Meaning |
|---|---|
| `"loading"` | Not read yet, or a newer read is in flight. |
| `"ready"` | Read. `Files` holds the changes. |
| `"no-vcs"` | The folder is not under version control. |
| `"offline"` | The server can't be reached. |
| `"error"` | The read failed. `Retry` reads again. |

### ChangeKind

How a file or folder changed.

| Value | Meaning |
|---|---|
| `"add"` | Added. |
| `"del"` | Deleted. |
| `"mix"` | Modified, or a folder whose files changed in more than one way. |

### PanelPane

A pane of the session's side region.

| Value | Meaning |
|---|---|
| `"side"` | The side pane. The default. |
| `"bottom"` | The bottom pane, under the conversation and the side pane. |
| `"dock"` | Where Settings › Terminal places a new terminal: the side or the bottom pane. |

### FieldVariant

A text field's look.

| Value | Meaning |
|---|---|
| `"default"` | The app's bordered field. |
| `"filter"` | A panel sidebar's filter: a search icon, no fill until hovered or focused, and a clear button while it holds text. |
| `"bare"` | Only the text, caret and selection, at 13 points: the box around it is the plugin's, such as a browser's address field. |

### BrowserEngineState

Where the browser engine stands.

| Value | Meaning |
|---|---|
| `"unsupported"` | This build has no engine for its platform. |
| `"missing"` | Not downloaded yet. |
| `"downloading"` | Downloading. |
| `"verifying"` | Checking the download. |
| `"paused"` | The background download waits, on a metered network or on battery. |
| `"failed"` | The download or the engine failed. |
| `"starting"` | Installed, and starting or connecting. |
| `"ready"` | Running. |

### BrowserZoom

A page zoom step.

| Value | Meaning |
|---|---|
| `"in"` | One step closer. |
| `"out"` | One step further. |
| `"reset"` | Back to 100%. |

### ImageFit

How an image fills its box.

| Value | Meaning |
|---|---|
| `"contain"` | Whole, aspect kept. The default. |
| `"cover"` | Fills the box, aspect kept, cropped. |
| `"fill"` | Stretched to the box. |

### ImageStatus

Where an image stands (`Images:State`): an explicit machine, loading then loaded or failed.

| Value | Meaning |
|---|---|
| `"loading"` | Its bytes are on their way or decoding, off the UI thread. |
| `"loaded"` | Decoded, with a size. |
| `"failed"` | It could not be read or decoded, or it has no pixels. It stays failed for the window's life, so later loads of the same source fail at once, as a browser remembers a broken image. |

### OfficeKind

An Office document's format.

| Value | Meaning |
|---|---|
| `"docx"` | Word. |
| `"xlsx"` | Excel. |
| `"pptx"` | PowerPoint. |

### ToastActionVariant

A toast button's look.

| Value | Meaning |
|---|---|
| `"primary"` | The app's neutral button. |
| `"secondary"` | A ghost button. |

### OfficeSignature

What an Office file's first bytes are.

| Value | Meaning |
|---|---|
| `"zip"` | A zip archive, as every Office Open XML package is. |
| `"compound"` | A Compound File: an encrypted file, or one in the binary .doc, .xls or .ppt formats. |
| `"other"` | Neither. |
| `"unreadable"` | The file could not be read. |

### UpdaterPhase

Where the app updater stands.

| Value | Meaning |
|---|---|
| `"disabled"` | Updates are off in this build or for this install. |
| `"idle"` | Nothing checked yet. |
| `"checking"` | Asking for a newer version. |
| `"up-to-date"` | This is the newest version. |
| `"downloading"` | Downloading the update. `progress` says how far. |
| `"ready"` | Downloaded. `Install` restarts into it. |
| `"download-required"` | A newer version this install can't update to by itself (Linux). `Install` opens its download. |
| `"installed"` | The release is installed. This process still runs the previous version until the next launch. |
| `"installing"` | Installing. |
| `"error"` | Failed. `error` says why. |

### UiEventKind

What a `UiEvent` reports.

| Value | Meaning |
|---|---|
| `"press"` | A click, a tap, or Enter or Space on a focused element. |
| `"middle-press"` | A middle click: the wheel button went down on the element and came up over it. |
| `"hover"` | The pointer entered or left. `entered` says which. |
| `"submit"` | Enter in a field. `text` holds its text. |
| `"change"` | A switch or radio changed. `checked` holds the new state. |
| `"select"` | A file tree row was picked. `path` holds it. |
| `"visible-rows"` | A virtual list's visible rows changed. `first` and `last` hold them. |

### MenuEntryKind

A kind of menu entry.

| Value | Meaning |
|---|---|
| `"item"` | A row the user can pick. The default. |
| `"group"` | A group title over the rows that follow. |
| `"separator"` | A divider. |

### DialogSize

A dialog's size: one of Desktop's four presets, each clamped to the window.

| Value | Meaning |
|---|---|
| `"normal"` | Confirmations, sign-ins and small forms: 480 points wide, as tall as its content. The default. |
| `"tall"` | A short form that reveals a list under a choice: 480 points wide, as tall as its content up to 600 points, its top edge kept so that it only grows downward. Since 1.11. |
| `"large"` | Palettes, pickers and lists: 680 by 600 points. |
| `"x-large"` | Long documents and settings-like panels: 880 by 680 points. |
| `"small"` | Deprecated: the same as `normal`. |
| `"medium"` | Deprecated: the same as `normal`. |

### DialogLayer

Which modal overlay layer a plugin dialog sits in.

| Value | Meaning |
|---|---|
| `"standard"` | Standard modal dialog layer, beneath elevated system recovery and server dialogs. Since 1.5. |
| `"elevated"` | Elevated modal dialog layer, above standard dialogs and recovery prompts. The default. Since 1.5. |

### StoreScope

What a declared store keeps a value for.

| Value | Meaning |
|---|---|
| `"global"` | One value for the whole app. |
| `"session"` | One value per session. |

### ProgressAppearance

How `ui.progressCircle` draws, as the app's ProgressCircle.

| Value | Meaning |
|---|---|
| `"compact"` | A 14-unit ring with a 1.5 stroke: the session header's. |
| `"indicator"` | A 16-unit ring with a thicker stroke and an overlay over its track: a tab's. |

### ConsoleLevel

How serious a console line is.

| Value | Meaning |
|---|---|
| `"info"` | `print` and `Console:Log`, and the host's own notes such as reloads. |
| `"warn"` | `warn` and `Console:Warn`. |
| `"error"` | An error a plugin raised or wrote with `Console:Error`. |

### TextDirection

Which way the interface language reads.

| Value | Meaning |
|---|---|
| `"ltr"` | Left to right. |
| `"rtl"` | Right to left: Arabic, Hebrew, Urdu, Punjabi, Persian, Dhivehi. |

### PluralCategory

A CLDR plural category: which form of a counted message the language uses for a count.

| Value | Meaning |
|---|---|
| `"zero"` | Zero. |
| `"one"` | One. |
| `"two"` | Two. |
| `"few"` | Few. |
| `"many"` | Many. |
| `"other"` | Every other count, and the fallback form. |

### DateStyle

A date and time format, as the app writes dates.

| Value | Meaning |
|---|---|
| `"time"` | The time: `3:45 PM`, `15:45`. |
| `"monthDay"` | Month and day: `Nov 3`. |
| `"monthDayYear"` | Month, day and year: `Nov 3, 2026`. |
| `"dateTime"` | Medium date and short time: `Nov 3, 2026, 3:45 PM`. |
| `"monthDayTime"` | Month, day and time: `Nov 3, 3:45 PM`. |
| `"monthDayYearTime"` | Month, day, year and time: `Nov 3, 2026, 3:45 PM`. |
| `"monthDayTimeZone"` | Month, day, time and the zone's offset: `Nov 3, 3:45 PM GMT-7`. |
| `"timeZone"` | The local time zone's short offset at that moment: `GMT-7`, `GMT+5:30`, `UTC`. Since 1.5. |

### TimeUnit

A unit of relative time.

| Value | Meaning |
|---|---|
| `"second"` | Seconds. |
| `"minute"` | Minutes. |
| `"hour"` | Hours. |
| `"day"` | Days. |
| `"week"` | Weeks. |
| `"month"` | Months. |
| `"year"` | Years. |

### NumberStyle

What a number means.

| Value | Meaning |
|---|---|
| `"decimal"` | A plain number: `12,345.6`. |
| `"percent"` | A share, where 1 is 100%: `42%`. |
| `"currency"` | Money in `currency`: `$1,234.50`. |

### NumberNotation

How a number is written.

| Value | Meaning |
|---|---|
| `"standard"` | Every digit: `12,345`. |
| `"compact"` | Short, with a unit, lowercased: `12k`, `3m`. |

## Types

### HexColor

A literal color from `ui.hex("#RRGGBB")` or `ui.hex("#RRGGBBAA")`. Prefer a `SemanticToken`: a literal color does not follow the theme.

Opaque: the host owns it, and Luau sees no fields.

### Color

A semantic token, or a literal color from `ui.hex`.

`type Color = SemanticToken | HexColor`

### IconName

The name of one of the app's icons, such as `"circle-check"`, `file:<name>` for the type icon of a file so named, or `<plugin>:<name>` for an icon declared by another plugin. A plugin's own `icons/<name>.svg` files go by their file name and win over the app's wherever that plugin names an icon: its `ui.*` nodes and every icon field it hands the host (a settings page, composer pill, titlebar item, session panel tab, closed tab, launcher, mobile view, or code block action). Each is a sanitized SVG document whose `viewBox` (or `width` and `height`) frames safe vector geometry and optional SMIL animation elements (`animate`, `animateTransform`, `set`, held on frame 0 under reduced motion) drawn with `currentColor`. Malformed, oversized, or disallowed SVGs fall back to a default icon.

`type IconName = string`

### MemoVersion

A memo's version. The host rebuilds the memo only when it changes.

`type MemoVersion = string | number | boolean`

### UiSize

A length in logical points, or `"full"` to fill the parent.

`type UiSize = number | "full"`

### UiNode

An element a `ui.*` constructor built. Pass it to a parent or return it from a render.

Opaque: the host owns it, and Luau sees no fields.

### UiHandler

An event handler `view:Handler` made. It runs as a coroutine, so it may yield.

Opaque: the host owns it, and Luau sees no fields.

### ImageId

An image the host holds, for `ui.image`.

Opaque: the host owns it, and Luau sees no fields.

### PluginHalf

What `plugin.defineWindow` and `plugin.defineApp` return. Return it from the entry module.

Opaque: the host owns it, and Luau sees no fields.

### Signal

An event source. Connections disconnect on their own when the plugin half that made them goes away. Written `Signal<T...>`.

| Member | Type | Description |
|---|---|---|
| `:Connect` | `(callback: (T...) -> ()) -> Connection` | Calls `callback` on every fire, as a coroutine. |
| `:Once` | `(callback: (T...) -> ()) -> Connection` | Calls `callback` on the next fire only. |
| `:Wait` | `() -> T...` | Yields until the next fire and returns its values. Yields. |

### ScriptSignal

A signal a plugin fires itself, from `ctx:CreateSignal()`. Its version counts fires: put it in a memo's version. Written `ScriptSignal<T...>`.

| Member | Type | Description |
|---|---|---|
| `:Connect` | `(callback: (T...) -> ()) -> Connection` | Calls `callback` on every fire, as a coroutine. |
| `:Once` | `(callback: (T...) -> ()) -> Connection` | Calls `callback` on the next fire only. |
| `:Wait` | `() -> T...` | Yields until the next fire and returns its values. Yields. |
| `:Fire` | `(T...) -> ()` | Fires the signal, adds 1 to its version and asks the window to redraw. |
| `:Version` | `() -> number` | How many times the signal fired. |

### Connection

One connection to a signal.

| Member | Type | Description |
|---|---|---|
| `.Connected` | `boolean` | False once disconnected. |
| `:Disconnect` | `() -> ()` | Stops the callback. Does nothing when already disconnected. |

### Contribution

A contribution a plugin added. The host withdraws it when the plugin half goes away.

| Member | Type | Description |
|---|---|---|
| `:Withdraw` | `() -> ()` | Removes the contribution now. |
| `:Refresh` | `() -> ()` | Asks the host to call the contribution's `show`, `list` or `render` again. |

### Live

A contract another plugin provides, followed live. Written `Live<T>`.

| Member | Type | Description |
|---|---|---|
| `:State` | `() -> LiveState` | The provider's state. |
| `:Get` | `() -> T?` | The provided value while the state is `active`. |
| `:Generation` | `() -> number` | Changes on every state change and every new value: put it in a memo's version. |
| `.Changed` | `Signal<LiveState>` | Fires on every state change and every new value. |

### View

What a render callback receives first. Render callbacks must not yield.

| Member | Type | Description |
|---|---|---|
| `:Memo` | `(key: string, version: MemoVersion, build: () -> UiNode?) -> UiNode?` | Returns the subtree `build` returns, and calls `build` again only when `version` changes. An unchanged frame costs no Luau call. Keep `key` unique within the render. |
| `:Handler` | `(callback: (event: UiEvent) -> ()) -> UiHandler` | Wraps `callback` for an `on*` property. The handler runs as a coroutine, so it may call `Async` methods and `task.wait`. |

### UiEvent

What a handler receives.

| Field | Type | Description |
|---|---|---|
| `kind` | `UiEventKind` | What happened. |
| `key` | `string?` | The `key` of the element the event came from. |
| `entered` | `boolean?` | A `hover` event: true as the pointer enters, false as it leaves. |
| `text` | `string?` | A `submit` event: the field's text. |
| `checked` | `boolean?` | A `change` event: the new state. |
| `path` | `string?` | A `select` event: the picked path. |
| `value` | `string?` | A `change` event of `ui.segmented`: the picked item's id. |
| `first` | `number?` | A `visible-rows` event: the first visible row, from 1. |
| `last` | `number?` | A `visible-rows` event: the last visible row. |
| `shift` | `boolean?` | Shift was held. |
| `alt` | `boolean?` | Alt (Option) was held. |
| `mod` | `boolean?` | The platform's command key was held: Ctrl, or Cmd on macOS. |

### Editor

A host-owned text field's content: typing, IME, selection and undo stay in the host and survive a hot reload.

| Member | Type | Description |
|---|---|---|
| `:Text` | `() -> string` | The field's text. |
| `:SetText` | `(text: string) -> ()` | Replaces the text and puts the caret at its end. |
| `:Version` | `() -> number` | Changes whenever the text, selection, composition or focus does: put it in a memo's version. |
| `:Focus` | `() -> ()` | Moves keyboard focus to the field. |
| `:Blur` | `() -> ()` | Takes keyboard focus from the field. |
| `:SelectAll` | `() -> ()` | Selects the whole text. |
| `:Focused` | `() -> boolean` | The field has keyboard focus. |
| `.Changed` | `Signal<string>` | Fires with the new text after each edit. |
| `.Submitted` | `Signal<string>` | Fires with the text when the user presses Enter. |
| `.FocusChanged` | `Signal<boolean>` | Fires as the field takes keyboard focus (true) or loses it (false). |
| `.KeyPressed` | `Signal<string>` | Fires with a key the field's `keys` hands over (`ArrowUp`, `ArrowDown`, `Escape`) instead of handling it. |

### UiLayoutProps

Size, spacing, position and accessibility: every `ui.*` node takes these. A props table frozen with `table.freeze` is decoded once and its node copied after that, so keep styles many nodes share in frozen tables made once (`local LABEL = table.freeze({ size = 13, color = "text-weak" })`). A node whose props are partly its own takes the frozen style first and a small table of its own props after it, laid over the style: `ui.row(ROW, { key = id, onPress = open }, children)`, `ui.text(words, LABEL, { color = ink })`, `ui.icon(name, ICON, { color = ink })`. Only those are decoded; a prop in both takes the second table's value.

| Field | Type | Description |
|---|---|---|
| `key` | `string?` | Identity among siblings across renders, and the anchor key for popovers and menus. |
| `w` | `UiSize?` | Width. |
| `h` | `UiSize?` | Height. |
| `minW` | `number?` | Minimum width. |
| `minH` | `number?` | Minimum height. |
| `maxW` | `number?` | Maximum width. |
| `maxH` | `number?` | Maximum height. |
| `grow` | `number?` | Share of a stack's free space. |
| `shrink` | `boolean?` | May shrink below its content size. |
| `pad` | `number?` | Padding on every side. |
| `px` | `number?` | Left and right padding. |
| `py` | `number?` | Top and bottom padding. |
| `pt` | `number?` | Top padding. |
| `pr` | `number?` | Right padding. |
| `pb` | `number?` | Bottom padding. |
| `pl` | `number?` | Left padding. |
| `m` | `number?` | Margin on every side. |
| `mx` | `number?` | Left and right margin. |
| `my` | `number?` | Top and bottom margin. |
| `mt` | `number?` | Top margin. |
| `mr` | `number?` | Right margin. |
| `mb` | `number?` | Bottom margin. |
| `ml` | `number?` | Left margin. |
| `pin` | `UiPin?` | Takes the node out of flow, at these offsets from its parent's edges. |
| `opacity` | `number?` | 0 to 1. |
| `role` | `Role?` | The accessibility role. |
| `label` | `string?` | The accessible name. |
| `help` | `string?` | A tooltip. |
| `helpSide` | `Side?` | Where the tooltip shows. Defaults to below. |
| `helpOpen` | `boolean?` | Keeps the tooltip showing while true, whatever the pointer does, as Desktop's `Tooltip forceOpen`: a Copy button's "Copied" for a moment after the press. Since 1.7. |
| `dir` | `("ltr" \| "rtl" \| "auto")?` | Which way the subtree reads, as HTML's `dir` does: `ltr` or `rtl` keeps a figure or a path in that order inside a window running the other way, `auto` takes a text's first strong direction (a name). On `ui.field` it is the value's direction, as on an `input`: `auto` starts an Arabic name at the right, `ltr` keeps an address or a host left to right. Defaults to the window's. |
| `cursor` | `("default" \| "pointer" \| "text" \| "grab" \| "grabbing" \| "not-allowed" \| "move" \| "col-resize" \| "row-resize" \| "ew-resize" \| "ns-resize" \| "nwse-resize" \| "nesw-resize" \| "crosshair" \| "cell" \| "zoom-in" \| "zoom-out")?` | The pointer's shape over the node and over what is inside it that sets none, by its CSS `cursor` name. Desktop shows the arrow on controls; set this only where its style sheet names another cursor, as the browser's address field does (`text`). Since 1.10. |

### UiPin

Offsets from the parent's edges for an out-of-flow node. Give one or two per axis.

| Field | Type | Description |
|---|---|---|
| `left` | `number?` | From the left edge. |
| `top` | `number?` | From the top edge. |
| `right` | `number?` | From the right edge. |
| `bottom` | `number?` | From the bottom edge. |

### UiBoxProps

A container's props: `ui.box`, `ui.row` and `ui.col`.

Also takes every field of [`UiLayoutProps`](#uilayoutprops).

| Field | Type | Description |
|---|---|---|
| `gap` | `number?` | Space between children. |
| `align` | `UiAlign?` | How children sit across the axis. |
| `justify` | `UiJustify?` | How children sit along the axis. |
| `center` | `boolean?` | Centers children on both axes. |
| `wrap` | `boolean?` | Children wrap onto more lines. |
| `fill` | `Color?` | Background. |
| `hover` | `Color?` | Background under the pointer. |
| `press` | `Color?` | Background while pressed. |
| `selected` | `boolean?` | Draws the `surface-selected` fill and reports the selected state to assistive technology. |
| `border` | `Color?` | Border color. |
| `borderWidth` | `number?` | Border width. Defaults to 1 when `border` is set. |
| `radius` | `number?` | Corner radius. |
| `corners` | `{ number }?` | Per-corner radii: top-left, top-right, bottom-right, bottom-left. |
| `shadow` | `Elevation?` | A shadow from the app's elevation scale. |
| `clip` | `boolean?` | Clips children to the box. |
| `scrollY` | `boolean?` | Scrolls vertical overflow. |
| `scrollX` | `boolean?` | Scrolls horizontal overflow. |
| `stickBottom` | `boolean?` | With `scrollY`: stays scrolled to the bottom as content grows, as a log or a chat does, until the reader scrolls up. |
| `focusable` | `boolean?` | Takes keyboard focus. |
| `disabled` | `boolean?` | Takes no presses. |
| `onPress` | `UiHandler?` | Runs on a press. |
| `menuTrigger` | `boolean?` | Its `onPress` opens a menu, as Kobalte's `Menu.Trigger` does: a mouse runs it as the button goes down, and that press's release runs nothing. A key, a screen reader's Invoke or a tap run it on the click. Contract 1.1; a 1.0 host ignores it with one warning and opens on the click. |
| `onMiddlePress` | `UiHandler?` | Runs on a middle click, as a browser's `auxclick`: a tab closes, a row opens in the background. A `middle-press` event, with the modifier keys. |
| `onHover` | `UiHandler?` | Runs as the pointer enters and leaves. |
| `hoverTransition` | `HoverTransition?` | How hover and press fills change. |
| `hoverGroup` | `boolean?` | Starts a hover group: descendants with `hoverGroupColor` or `revealOnHover` react while this box is hovered. |
| `hoverGroupColor` | `Color?` | The text or icon color while the nearest hover group is hovered. |
| `revealOnHover` | `boolean?` | Shows only while the nearest hover group is hovered or focused. |
| `color` | `Color?` | The default text and icon color for descendants. |

### UiFrameProps

`ui.frame` props: a layout container whose border takes layout space snapped to device pixels (`box-sizing: border-box`).

Also takes every field of [`UiBoxProps`](#uiboxprops).

| Field | Type | Description |
|---|---|---|
| `direction` | `UiDirection?` | Stack direction: `"col"` (default), `"row"` or `"box"`. |
| `borderSides` | `{ number }?` | Per-side border widths in points: `{ top, right, bottom, left }`. Overrides `borderWidth`. |
| `focusReturn` | `string?` | The key of the element that takes focus when this frame leaves while focus is on it or inside it: one view of a popover handing focus to the control that leads back to it. That element, or its first focusable child; nothing happens when none shows. Since 1.7. |
| `focusGroup` | `boolean?` | One Tab stop for the focusable controls inside, which the arrow keys move between, as a roving `tabindex` does: Up and Down with `role` `list`, `listbox`, `menu` or `tree`, Left and Right as well otherwise. Home and End go to the ends; a `list` or `tree` stops at its ends, the others wrap. In a `list`, `listbox`, `menu`, `tree` or `tablist`, typing moves to the first control whose label starts with it. Space presses the focused control, and in a list Return does too. Since 1.7. |
| `keys` | `{ string }?` | Keys `onKey` takes while focus is on the frame or inside it, outside a text field, pressed with no modifier: any of `Enter`, `Escape`, `ArrowUp`, `ArrowDown`, `ArrowLeft`, `ArrowRight`, `Home`, `End`, `PageUp`, `PageDown`, `Delete` and `Backspace`. A key it takes does nothing else there: Return in a `focusGroup` list then no longer presses the focused control. Since 1.7. |
| `onKey` | `UiHandler?` | Runs as a key `keys` names is pressed, with its name in `event.keyName`. Since 1.7. |

### UiTextProps

Text styling: `ui.text` and `ui.shimmer`.

Also takes every field of [`UiLayoutProps`](#uilayoutprops).

| Field | Type | Description |
|---|---|---|
| `size` | `number?` | Font size in points. Defaults to 13. |
| `weight` | `FontWeight?` | Weight. |
| `lineHeight` | `number?` | Line height in points. |
| `tracking` | `number?` | Letter spacing in points. |
| `color` | `Color?` | Text color. Defaults to `text-base`. |
| `mono` | `boolean?` | The code font. |
| `tabular` | `boolean?` | Tabular figures. |
| `wrap` | `boolean?` | Wraps onto more lines. The default for text in a column. |
| `noWrap` | `boolean?` | Keeps to one line. |
| `truncate` | `boolean?` | Ends with an ellipsis when it does not fit. |
| `fadeOut` | `boolean?` | Fades out at the end when it does not fit, instead of an ellipsis. |
| `selectable` | `boolean?` | The user can select and copy it. |
| `textAlign` | `Align?` | Alignment within its box. |
| `hoverGroupColor` | `Color?` | The color while the nearest hover group is hovered. |
| `revealOnHover` | `boolean?` | Shows only while the nearest hover group is hovered. |

### UiSpacerProps

`ui.spacer` props.

Also takes every field of [`UiLayoutProps`](#uilayoutprops).

### UiDividerProps

`ui.divider` props.

Also takes every field of [`UiLayoutProps`](#uilayoutprops).

| Field | Type | Description |
|---|---|---|
| `vertical` | `boolean?` | A vertical rule. Defaults to horizontal. |
| `color` | `Color?` | Defaults to `border-weak`. |

### UiIconProps

`ui.icon` props.

Also takes every field of [`UiLayoutProps`](#uilayoutprops).

| Field | Type | Description |
|---|---|---|
| `size` | `number?` | Width and height in points. Defaults to 16. |
| `color` | `Color?` | Defaults to `icon-base`. |
| `tone` | `Tone?` | A status color, instead of `color`. |
| `hoverGroupColor` | `Color?` | The color while the nearest hover group is hovered. |
| `revealOnHover` | `boolean?` | Shows only while the nearest hover group is hovered. |

### UiImageProps

`ui.image` props. Give it a size (`w`, `h`); without one it takes the image's own size in points once it has loaded. Inside a `view:Memo`, put `Images:State(image).status` in the memo's version, so the memo rebuilds as the image lands or fails.

Also takes every field of [`UiLayoutProps`](#uilayoutprops).

| Field | Type | Description |
|---|---|---|
| `image` | `ImageId?` | The image, from the `Images` service or an `Async` render. |
| `src` | `string?` | Instead of `image`: the source `Images:Load` takes (a URL, a server path, a data URL or a file of the plugin's folder). |
| `fit` | `ImageFit?` | How it fills its box. |
| `radius` | `number?` | Corner radius. |
| `placeholder` | `Color?` | The fill under the image: shown while it loads and through its transparent pixels. |
| `fadeIn` | `boolean?` | Accepted for compatibility. Loaded images appear immediately. |
| `preview` | `boolean?` | A press opens it in the app's image viewer, which grows it out of its place, with zoom; Escape flies it back. Give it a `label`: the viewer's name for it. |

### UiVideoProps

`ui.video` props. Give it a size.

Also takes every field of [`UiLayoutProps`](#uilayoutprops).

| Field | Type | Description |
|---|---|---|
| `src` | `string` | An `https:` or `http:` URL of the video. |
| `fit` | `ImageFit?` | How it fills its box: `contain` letterboxes it, `cover` crops it. |
| `radius` | `number?` | Corner radius. |

### UiMetricProps

`ui.metric` props: a number shown as text.

Also takes every field of [`UiLayoutProps`](#uilayoutprops).

| Field | Type | Description |
|---|---|---|
| `value` | `number` | The number the text stands for. Hosts that animate compare it with the last value. |
| `text` | `string` | What it reads, such as `"12.4K"`. |
| `tabular` | `boolean?` | Tabular figures. Defaults to true. |
| `animate` | `boolean?` | Animates changed digits where the host can. Defaults to true. Other hosts show the text. |
| `size` | `number?` | Font size in points. |
| `weight` | `FontWeight?` | Weight. |
| `lineHeight` | `number?` | Line height in points. |
| `tracking` | `number?` | Letter spacing in points. |
| `color` | `Color?` | Color. |

### UiButtonProps

`ui.button` props: the app's own button.

Also takes every field of [`UiLayoutProps`](#uilayoutprops).

| Field | Type | Description |
|---|---|---|
| `text` | `string?` | The visible label. Also the accessible name unless `label` is set. |
| `icon` | `IconName?` | A leading icon. |
| `variant` | `ButtonVariant?` | The look. |
| `size` | `ControlSize?` | The size. |
| `disabled` | `boolean?` | Takes no presses. |
| `loading` | `boolean?` | Shows the app's loader and takes no presses. |
| `pressed` | `boolean?` | A toggle button's pressed state. |
| `onPress` | `UiHandler?` | Runs on a press. |
| `menuTrigger` | `boolean?` | Its `onPress` opens a menu, as Kobalte's `Menu.Trigger` does: a mouse runs it as the button goes down. A key, a screen reader's Invoke or a tap run it on the click. Contract 1.1. |
| `autofocus` | `boolean?` | Takes focus as the dialog it is in opens, as HTML's `autofocus` in a dialog: a confirmation's Cancel, say. Without one the dialog focuses its first text field or Tab stop. Since 1.7. |
| `focusReturn` | `string?` | The key of the element that takes focus when this one leaves while it has focus, as a page's `element.focus()` after a view changes: a drill-in's Back hands focus to the row that opened it. That element, or its first focusable child; nothing happens when none shows. Since 1.7. |

### UiIconButtonProps

`ui.iconButton` props: a square button with an icon. Give it a `label`.

Also takes every field of [`UiLayoutProps`](#uilayoutprops).

| Field | Type | Description |
|---|---|---|
| `icon` | `IconName` | The icon. |
| `variant` | `ButtonVariant?` | The look. Defaults to `ghost`. |
| `size` | `ControlSize?` | The size. |
| `disabled` | `boolean?` | Takes no presses. |
| `pressed` | `boolean?` | A toggle button's pressed state. |
| `color` | `Color?` | The icon's color, instead of the variant's. |
| `keybind` | `string?` | A published command id whose shortcut its tooltip shows after the `help` text, as keycaps. |
| `onPress` | `UiHandler?` | Runs on a press. |
| `menuTrigger` | `boolean?` | Its `onPress` opens a menu, as Kobalte's `Menu.Trigger` does: a mouse runs it as the button goes down. A key, a screen reader's Invoke or a tap run it on the click. Contract 1.1. |
| `autofocus` | `boolean?` | Takes focus as the dialog it is in opens, as HTML's `autofocus` in a dialog: a confirmation's Cancel, say. Without one the dialog focuses its first text field or Tab stop. Since 1.7. |
| `focusReturn` | `string?` | The key of the element that takes focus when this one leaves while it has focus, as a page's `element.focus()` after a view changes: a drill-in's Back hands focus to the row that opened it. That element, or its first focusable child; nothing happens when none shows. Since 1.7. |

### UiSwitchProps

`ui.switch` props: the app's own switch.

Also takes every field of [`UiLayoutProps`](#uilayoutprops).

| Field | Type | Description |
|---|---|---|
| `checked` | `boolean` | On. |
| `disabled` | `boolean?` | Cannot change. |
| `onChange` | `UiHandler?` | Runs with the new state in `event.checked`. |

### UiCheckboxProps

`ui.checkbox` props: Desktop's compact checkbox (`Checkbox appearance="compact"`) with its label.

Also takes every field of [`UiLayoutProps`](#uilayoutprops).

| Field | Type | Description |
|---|---|---|
| `text` | `string` | The label, beside the box. |
| `description` | `string?` | A line under the label. |
| `checked` | `boolean` | Ticked. |
| `indeterminate` | `boolean?` | Some of what it stands for is ticked: the box shows a dash, as Desktop's `indeterminate`. A press still reports the opposite of `checked`. Since 1.11. |
| `surface` | `("base" \| "layer-1")?` | The background the box sits on: `base` (the default) or `layer-1`, as Desktop's `surface`; an unticked box takes its fill. |
| `presentation` | `boolean?` | Draws only the checkbox box visually with no role, focus or click handler, for use inside an `option` row (Desktop's `Checkbox presentation`). Since 1.12. |
| `disabled` | `boolean?` | Cannot change; drawn at half opacity. |
| `onChange` | `UiHandler?` | Runs with the new state in `event.checked`. |

### UiRadioProps

`ui.radio` props: a radio row with its label and description.

Also takes every field of [`UiLayoutProps`](#uilayoutprops).

| Field | Type | Description |
|---|---|---|
| `text` | `string` | The row's label. |
| `description` | `string?` | A line under the label. |
| `checked` | `boolean` | Selected. |
| `tone` | `Tone?` | Colors the description, as a status line. |
| `lineHeight` | `number?` | The label's line height: 20 by default, as Desktop's radio label; 16 for a list row whose label sets its own. |
| `disabled` | `boolean?` | Cannot change. |
| `onChange` | `UiHandler?` | Runs as the user picks the row. |

### UiFieldProps

`ui.field` props: the app's own text field over an `Editor`.

Also takes every field of [`UiLayoutProps`](#uilayoutprops).

| Field | Type | Description |
|---|---|---|
| `editor` | `Editor` | The field's content, from `ctx:CreateEditor`. |
| `placeholder` | `string?` | Shows while the field is empty. |
| `size` | `ControlSize?` | The size. |
| `invalid` | `boolean?` | Drawn as invalid. |
| `secure` | `boolean?` | Hides the text, for a password. |
| `disabled` | `boolean?` | Takes no typing. |
| `readOnly` | `boolean?` | Takes no typing but looks as usual, such as while what was typed is sent. |
| `multiline` | `boolean?` | Grows to more lines. Enter inserts a line break; Mod+Enter submits. |
| `variant` | `FieldVariant?` | The look. Defaults to `default`. |
| `icon` | `IconName?` | An icon in the box before the text, as Desktop's `TextInput` `leadingIcon`: a search field's magnifying glass. The `default` variant only. Since 1.7. |
| `clearable` | `boolean?` | A Clear button at the box's end that empties the field, as Desktop's `TextInput` `showClearButton`: set it while the field has text. The `default` variant only. Since 1.7. |
| `focusDown` | `string?` | The key of the element Down moves the keyboard to from this field, or that element's first focusable control: a list under its search field, as official's site chooser does. The `default` variant only. Since 1.7. |
| `keys` | `{ string }?` | Keys the field hands to its editor's `KeyPressed` instead of handling them: any of `ArrowUp`, `ArrowDown` and `Escape`. |
| `onSubmit` | `UiHandler?` | Runs on Enter with the text in `event.text`. |

### UiSegmentedItem

One item of `ui.segmented`.

| Field | Type | Description |
|---|---|---|
| `id` | `string` | What `onChange` receives. |
| `icon` | `IconName` | Its icon. |
| `label` | `string` | Its accessible name and tooltip. |
| `key` | `string?` | Its element's key. Defaults to `id`. |

### UiListboxProps

`ui.listbox` props: a listbox container (`role="listbox"`) holding `ui.option` rows.

Also takes every field of [`UiFrameProps`](#uiframeprops).

### UiOptionProps

`ui.option` props: one selectable option row (`role="option"`) inside a `ui.listbox`.

Also takes every field of [`UiFrameProps`](#uiframeprops).

| Field | Type | Description |
|---|---|---|
| `selected` | `boolean?` | Whether this option is selected (`aria-selected`). Since 1.12. |

### UiSelectOption

One option of `ui.select`.

| Field | Type | Description |
|---|---|---|
| `value` | `string` | What `onChange` receives in `event.value`. |
| `label` | `string` | The option's visible text. |

### UiSelectProps

`ui.select` props: Desktop's `Select` dropdown control (`select-v2`). Desktop's plugin API calls it `Select`.

Also takes every field of [`UiLayoutProps`](#uilayoutprops).

| Field | Type | Description |
|---|---|---|
| `value` | `string?` | The selected option's `value`. |
| `placeholder` | `string?` | Text shown when no option matches `value`. |
| `options` | `{ UiSelectOption }` | The options, in order. |
| `disabled` | `boolean?` | Cannot open; drawn at half opacity. |
| `onChange` | `UiHandler?` | Runs when an option is picked, with its value in `event.value`. |

### UiSegmentedProps

`ui.segmented` props: the app's segmented control of icon items.

Also takes every field of [`UiLayoutProps`](#uilayoutprops).

| Field | Type | Description |
|---|---|---|
| `items` | `{ UiSegmentedItem }` | Its items, in order. |
| `value` | `string` | The picked item's id. |
| `onChange` | `UiHandler?` | Runs as the user picks another, with its id in `event.value`. |

### UiLoaderProps

`ui.loader` props: the app's spinner.

Also takes every field of [`UiLayoutProps`](#uilayoutprops).

| Field | Type | Description |
|---|---|---|
| `size` | `number?` | Width and height in points. Defaults to 16. |

### UiServerIndicatorProps

`ui.serverIndicator` props: a server's health as Settings > Servers shows it (Desktop's `ServerRow.Indicator`).

Also takes every field of [`UiLayoutProps`](#uilayoutprops).

| Field | Type | Description |
|---|---|---|
| `health` | `ServerHealth?` | Its latest health check: a green dot once it answered, red when it didn't, a faint one when unknown. |
| `auth` | `boolean?` | It waits for the user to sign in: a lock. |
| `connecting` | `boolean?` | It is starting or connecting: a spinner. |

### UiPillProps

`ui.pill` props: a status capsule like the composer's pills.

Also takes every field of [`UiLayoutProps`](#uilayoutprops).

| Field | Type | Description |
|---|---|---|
| `text` | `string` | The capsule's label. |
| `icon` | `IconName?` | A leading icon. |
| `tone` | `Tone?` | Colors the icon. |
| `pressed` | `boolean?` | Shows the capsule pressed. |
| `onPress` | `UiHandler?` | Runs on a press. |

### UiKeybindProps

`ui.keybind` props: a shortcut chip.

Also takes every field of [`UiLayoutProps`](#uilayoutprops).

| Field | Type | Description |
|---|---|---|
| `command` | `string?` | A published command id. The chip shows the user's current binding. |
| `keys` | `string?` | A literal binding, such as `"mod+shift+t"`, when there is no command. |

### UiSettingsRowProps

`ui.settingsRow` props: one settings row, as the app's own.

Also takes every field of [`UiLayoutProps`](#uilayoutprops).

| Field | Type | Description |
|---|---|---|
| `id` | `string` | The row's search id: settings search scrolls to it. |
| `title` | `string` | The row's title. |
| `description` | `string?` | A line under the title. |

### UiSettingsListProps

`ui.settingsList` props: a card of settings rows split by hairlines.

Also takes every field of [`UiLayoutProps`](#uilayoutprops).

| Field | Type | Description |
|---|---|---|
| `title` | `string?` | A section title above the card. |
| `catalog` | `boolean?` | Desktop's catalog list (`data-variant="catalog"`): rows 16 in from the card's sides, for lists of entries rather than controls. |

### UiFlyoutRowProps

`ui.flyoutRow` props: a popover row that opens a nested popover beside it. The host owns placement, hover intent and dismissal.

Also takes every field of [`UiLayoutProps`](#uilayoutprops).

| Field | Type | Description |
|---|---|---|
| `text` | `string` | The row's label. |
| `icon` | `IconName?` | A leading icon. |
| `detail` | `string?` | Muted text at the row's end, such as a count. |
| `popover` | `string` | The id of this plugin's popover the row opens. |
| `props` | `{ [string]: any }?` | Passed to the nested popover's render as `props.data`. |
| `alert` | `boolean?` | Shows a danger dot before the detail: something in the nested list failed. |
| `side` | `Side?` | The side of the row the nested popover opens on. Defaults to `right`; a popover at the window's right edge opens its rows' lists on the `left`. |
| `radius` | `number?` | Corner radius of the row's hover and open fill. Defaults to 4. |

### UiBadgeProps

`ui.badge` props: Desktop's compact badge, a short muted label in a 16-point tag. Desktop's plugin API calls it `Badge`.

Also takes every field of [`UiLayoutProps`](#uilayoutprops).

| Field | Type | Description |
|---|---|---|
| `text` | `string` | Its label, on one line; cut short where it has no room. |

### UiDialogHeaderProps

`ui.dialogHeader` props: the app's dialog header (`DialogHeader` with a `DialogTitleGroup`), 16 in from the frame, with its close button. Desktop's plugin API calls it `DialogHeader`.

Also takes every field of [`UiLayoutProps`](#uilayoutprops).

| Field | Type | Description |
|---|---|---|
| `title` | `string` | The dialog's title: 15 pt, wrapping anywhere unless `wrapTitle` is false. |
| `description` | `string?` | A muted line under the title. |
| `badge` | `string?` | A badge after the title on its line, such as a profile's name. The title then stays on one line, and each is cut short where the line has no room for both. Since 1.11. |
| `hideClose` | `boolean?` | Leaves out the close button. |
| `closeLabel` | `string?` | The close button's accessible name. Defaults to Close. |
| `wrapTitle` | `boolean?` | Whether the title wraps across lines (breaking anywhere for long identifiers). Defaults to true; set false for short single-line dialog titles. Since 1.5. |

### UiMarkdownProps

`ui.markdown` props. Native: parsing, shaping, code fences and Mermaid diagrams stay in the host.

Also takes every field of [`UiLayoutProps`](#uilayoutprops).

| Field | Type | Description |
|---|---|---|
| `source` | `string` | The Markdown. |
| `size` | `number?` | Body font size in points. Defaults to 14. |
| `lineHeight` | `number?` | Body line height in points. |
| `highlights` | `{ string }?` | Words to highlight, such as search matches. |
| `selectable` | `boolean?` | The user can select and copy it. Defaults to true. |
| `streaming` | `boolean?` | The source is still growing: the host keeps the last block open. |

### UiVirtualListProps

`ui.virtualList` props. Native: windowing, the height cache and scroll anchoring stay in the host; Luau builds only the rows that come into view.

Also takes every field of [`UiLayoutProps`](#uilayoutprops).

| Field | Type | Description |
|---|---|---|
| `count` | `number` | How many rows. |
| `estimatedRowHeight` | `number` | A row's height before it is measured. |
| `version` | `MemoVersion` | Change it when rows change. While it stays the same, scrolling builds only rows that enter the view. |
| `rowVersions` | `{ MemoVersion }?` | Each row's own version, beside `version`: a row is built again when `version` or its own changes, so opening one row rebuilds that row alone. |
| `scrollY` | `boolean?` | `false`: the list has no scroll of its own. It sits in a scrolling column (`scrollY = true` on an ancestor), under or among other content, and builds the rows that column shows. Defaults to `true`: the list is its own scroll. |
| `renderRow` | `(index: number) -> UiNode` | Builds row `index`, from 1. It must not yield. |
| `rowKey` | `((index: number) -> string)?` | A stable identity for row `index`, so rows keep their state as the list changes. Defaults to the index. |
| `gap` | `number?` | Space between rows. |
| `onVisibleRows` | `UiHandler?` | Runs as the visible rows change, with `event.first` and `event.last`. |

### UiTerminalSurfaceProps

`ui.terminalSurface` props. Native: the VT parser, scrollback, glyph grid, IME and PTY stream stay in the host.

Also takes every field of [`UiLayoutProps`](#uilayoutprops).

| Field | Type | Description |
|---|---|---|
| `session` | `PtySession?` | The terminal session, from the `Terminal` service. |
| `fontFamily` | `string?` | A font family. Defaults to the user's code font. |
| `fontSize` | `number?` | Font size in points. |
| `focused` | `boolean?` | Takes keyboard focus. |
| `width` | `number?` | The pane's width in points (`PanelProps.width`): the grid's columns fit it. |
| `height` | `number?` | The pane's height in points: the grid's rows fit it. |
| `narrow` | `boolean?` | The narrow window's Terminal view of session `sessionId`: its terminals' tabs over the active one. |
| `sessionId` | `string?` | The session a `narrow` view shows. |

### UiTerminalTabLabelProps

`ui.terminalTabLabel` props.

Also takes every field of [`UiLayoutProps`](#uilayoutprops).

| Field | Type | Description |
|---|---|---|
| `session` | `PtySession` | The terminal the tab shows. |
| `title` | `string` | Its label. |
| `selected` | `boolean?` | The tab is selected. |

### UiBrowserSurfaceProps

`ui.browserSurface` props. Native: the browser engine, its compositor, and input forwarding stay in the host.

Also takes every field of [`UiLayoutProps`](#uilayoutprops).

| Field | Type | Description |
|---|---|---|
| `areaWidth` | `number?` | Available width of the device preview area. |
| `areaHeight` | `number?` | Available height of the device preview area. |
| `tab` | `BrowserTab` | The page, from the `Browser` service. Until the engine runs, the surface shows the engine's state instead: its download, the download's progress, or why it failed. |
| `focused` | `boolean?` | Takes keyboard focus. |

### BrowserViewport

Device dimensions in CSS pixels and optional display scale.

| Field | Type | Description |
|---|---|---|
| `width` | `number` | Width from 1 to 4096. |
| `height` | `number` | Height from 1 to 4096. |
| `profile` | `string` | desktop, phone, tablet or windows_tablet. |
| `preset` | `string?` | Device preset id. |
| `scale` | `number?` | Display factor, or nil to fit the pane. |

### UiCodeViewProps

`ui.codeView` props. Native: highlighting, the line buffer, the gutter, selection and find stay in the host.

Also takes every field of [`UiLayoutProps`](#uilayoutprops).

| Field | Type | Description |
|---|---|---|
| `doc` | `CodeDocument` | The file, from `Workspace:OpenDocument`. |
| `lineWrap` | `boolean?` | Wraps long lines. |
| `showLineNumbers` | `boolean?` | Shows the gutter. Defaults to true. |
| `revealLine` | `number?` | Scrolls to this line, from 1, and highlights it. |

### UiDiffViewProps

`ui.diffView` props. Native: hunk layout, intraline diffing, highlighting and comment anchors stay in the host.

Also takes every field of [`UiLayoutProps`](#uilayoutprops).

| Field | Type | Description |
|---|---|---|
| `diff` | `DiffDocument` | The changes, from `Diff:Open`. |
| `mode` | `DiffMode` | Split or unified. |
| `expand` | `boolean?` | Shows unchanged lines in full. |
| `file` | `string?` | Shows only this file's changes, under its header. |
| `revealLine` | `number?` | Scrolls to this line of the new side, from 1, once it shows. |
| `list` | `boolean?` | Every file as a row that opens to its unified diff, under a header with the totals and Expand all: the narrow Changes view. Its children sit at the header's start. |

### UiFileTreeProps

`ui.fileTree` props. Native: tree flattening, file icons and keyboard navigation stay in the host. A secondary click on a row offers the apps that open it.

Also takes every field of [`UiLayoutProps`](#uilayoutprops).

| Field | Type | Description |
|---|---|---|
| `tree` | `FileTreeModel` | The tree, from `Workspace:OpenTree`. |
| `allowed` | `{ string }?` | Shows only these workspace paths and the folders above them, all known at once: a change list rather than the folder's listing. |
| `kinds` | `{ [string]: ChangeKind }?` | A change letter at the end of these paths' rows: `A`, `D` or `M`. |
| `filter` | `string?` | Lists only the files whose path holds this text, flat, with their counts, instead of the tree. The first is highlighted. |
| `selectedPath` | `string?` | The selected row. |
| `highlightedPath` | `string?` | A row to highlight, such as the open file. |
| `onSelect` | `UiHandler?` | Runs as the user picks a file, with `event.path`. |
| `onContextMenu` | `UiHandler?` | Runs on a secondary click, with `event.path`, instead of the host's menu. |
| `narrow` | `boolean?` | A workspace tree as the narrow Files view: All files and the open files' tabs over the tree, or the open file. |
| `width` | `number?` | The narrow Files view's width. |

### UiPanelSidebarProps

`ui.panelSidebar(props, { sidebar, main })` props: a panel's frame with the file sidebar at its start or end, resized by its inner edge (a double click resets it), and the sidebar toggle pinned at the panel's top corner on the sidebar's side. Whether the sidebar shows, its width and the edge it sits on are one preference every panel shares (`Layout:PanelSidebar`); the toggle and Mod+\ flip it, and a right-click on the sidebar or the toggle offers Sidebar position.

Also takes every field of [`UiLayoutProps`](#uilayoutprops).

| Field | Type | Description |
|---|---|---|
| `id` | `string` | Prefixes its parts' keys: `<id>-sidebar`, `<id>-sidebar-resize`, `<id>-tree-toggle`. |
| `width` | `number?` | Its width until the user drags it. Defaults to 240. |
| `minWidth` | `number?` | The narrowest it gets. Defaults to 200. |
| `maxWidth` | `number?` | The widest it gets. Defaults to 480; give 60% of the panel's width when that is less. |
| `locked` | `boolean?` | Shows the sidebar however the user left it, its toggle disabled (Files' Open file launcher). |

### PanelSidebarInfo

The panels' shared file sidebar.

| Field | Type | Description |
|---|---|---|
| `opened` | `boolean` | It shows. |
| `width` | `number` | Its width in points. |
| `side` | `PanelSidebarSide` | The edge of the panel it sits on. With `end`, the toggle is at the panel's top end corner: leave it room at the end of a header or toolbar under it, not at the start. Since 1.9. |

### UiOpenInButtonProps

`ui.openInButton` props: the app's Open in split button. Its mark opens the path in the app the user chose last; its chevron lists the installed apps and Copy path. Nothing where the server's files aren't on this machine.

Also takes every field of [`UiLayoutProps`](#uilayoutprops).

| Field | Type | Description |
|---|---|---|
| `path` | `string` | A workspace path, or empty for the workspace folder. |

### UiOfficePagesProps

`ui.officePages` props. Native: the page column, its scenes, near-viewport loading and the zoom stay in the host.

Also takes every field of [`UiLayoutProps`](#uilayoutprops).

| Field | Type | Description |
|---|---|---|
| `document` | `OfficeDocument` | A Word document or a deck, from `Office:OpenAsync`. |
| `labels` | `OfficePagesLabels` | The words it shows. |

### UiOfficeSheetProps

`ui.officeSheet` props. Native: frames of the sheet as it scrolls, its headers, the selection and copying stay in the host.

Also takes every field of [`UiLayoutProps`](#uilayoutprops).

| Field | Type | Description |
|---|---|---|
| `document` | `OfficeDocument` | A workbook, from `Office:OpenAsync`; it shows the sheet `Office:SetSheet` chose. |
| `labels` | `OfficeSheetLabels?` | The words the grid says, in the plugin's language. |

### UiGenUiProps

`ui.genUi` props. Native: the outline parser and its animated widgets stay in the host.

Also takes every field of [`UiLayoutProps`](#uilayoutprops).

| Field | Type | Description |
|---|---|---|
| `source` | `string` | The ```` ```ui ```` outline source. |
| `streaming` | `boolean?` | The source is still growing. |

### UiProgressCircleProps

`ui.progressCircle` props: the app's progress ring.

Also takes every field of [`UiLayoutProps`](#uilayoutprops).

| Field | Type | Description |
|---|---|---|
| `percentage` | `number` | How full, 0 to 100. The arc runs clockwise from twelve o'clock. |
| `appearance` | `ProgressAppearance?` | Its look. Defaults to `compact`. |
| `size` | `number?` | Width and height in points. Defaults to the appearance's own: 14 or 16. |
| `strokeWidth` | `number?` | The stroke in the ring's own units. Defaults to 1.5 for `compact`, 3 for `indicator`. |
| `track` | `Color?` | The track's color. Defaults to `surface-4`. |
| `overlay` | `Color?` | `indicator` only: a color over the track. Defaults to none. |
| `progress` | `Color?` | The arc's color. Defaults to `icon-base`. |

### LocaleInfo

The interface language.

| Field | Type | Description |
|---|---|---|
| `id` | `string` | The app's id for it, such as `"de"` or `"zht"`: the name of its catalog file, `locales/<id>.json`. |
| `tag` | `string` | Its BCP 47 tag, such as `"de-DE"` or `"zh-Hant"`. |
| `name` | `string` | Its name in itself, such as `"Deutsch"`. |
| `direction` | `TextDirection` | Which way it reads. |

### NumberFormat

How `I18n:Number` writes a number.

| Field | Type | Description |
|---|---|---|
| `style` | `NumberStyle?` | Defaults to `decimal`. |
| `notation` | `NumberNotation?` | Defaults to `standard`. |
| `currency` | `string?` | An ISO 4217 code for `currency`. Defaults to `"USD"`. |
| `maxFraction` | `number?` | The most fraction digits. Defaults to 3 for `decimal`, 2 for `currency`, 0 for `percent` and `compact`. |
| `localePattern` | `boolean?` | For `currency`, formats with the interface language's localized currency pattern (such as `1.234,50 $` or `1,84 USD`) instead of prefixing the narrow currency symbol. Defaults to false. Since 1.5. |

### SessionUsage

A session's token counts at its last step.

| Field | Type | Description |
|---|---|---|
| `input` | `number` | Input tokens. |
| `output` | `number` | Output tokens. |
| `reasoning` | `number` | Reasoning tokens. |
| `cacheRead` | `number` | Tokens read from the cache. |
| `cacheWrite` | `number` | Tokens written to the cache. |

### Session

A session as the app holds it.

| Field | Type | Description |
|---|---|---|
| `id` | `string` | The session id. |
| `serverId` | `string` | The key of the server that owns it: a `ServerInfo.key`, `local` for the app's own server. |
| `directory` | `string` | Its workspace folder. |
| `title` | `string` | Its title. |
| `parentId` | `string?` | The parent session of a subagent's session. |
| `projectId` | `string?` | The project it belongs to. Nil for a folder outside any project. |
| `agent` | `string?` | The agent it runs with, such as `build`. |
| `variant` | `string?` | The model variant it runs with, such as `high`. Nil for the model's default. |
| `busy` | `boolean` | A turn is running. |
| `version` | `number` | Changes whenever anything here does: put it in a memo's version. |
| `messagesVersion` | `number` | Changes when its newest messages or its inbox's waiting input do, not as older ones are read or the reader scrolls: the version for a read of its newest messages. |
| `cost` | `number` | What it cost in USD, its subagents' sessions included. |
| `usage` | `SessionUsage?` | Token counts at the last step with tokens. |
| `usageProviderId` | `string?` | The provider of the step `usage` came from. |
| `usageModelId` | `string?` | The model of the step `usage` came from. |
| `usageMessageId` | `string?` | The id of the message `usage` came from: the last step with tokens after the last finished compaction and before a staged revert. |
| `windowStart` | `string?` | The oldest message Desktop's timeline would hold: its first read on opening the session, a page of 20 per approach of the top since, and every message after. `usage` and the Context tab's counts come from these messages, as Desktop's do. Empty while that is not known yet; nil before this window opened the session (every message counts). |
| `usageCreated` | `number?` | When the message `usage` came from was created, in milliseconds since the Unix epoch. |
| `providerId` | `string?` | The provider of the model the session runs with. |
| `modelId` | `string?` | The model the session runs with. |
| `created` | `number?` | When it was created, in milliseconds since the Unix epoch. |
| `updated` | `number?` | When it last changed, in milliseconds since the Unix epoch. |
| `messages` | `number?` | How many messages the app holds for it. Nil before it has read them. |
| `lastErrorStatus` | `number?` | The HTTP status code of the session's last assistant message failure, when it failed with one (such as 402 or 429). Since 1.5. |

### ModelInfo

A model as the server's catalog names it.

| Field | Type | Description |
|---|---|---|
| `providerName` | `string?` | The provider's name. Nil when the catalog does not list the provider. |
| `modelName` | `string?` | The model's name. Nil when the catalog does not list it. |
| `contextLimit` | `number?` | Its context window in tokens. |
| `outputLimit` | `number?` | Its output limit in tokens. |
| `paid` | `boolean?` | Any of its token prices is above zero. |

### ModelEntry

An entry in the host's model catalog (`Models:List`).

| Field | Type | Description |
|---|---|---|
| `providerId` | `string` | The provider id. |
| `modelId` | `string` | The model id. |
| `name` | `string` | Its display name. |
| `providerName` | `string?` | The provider's display name. |
| `integration` | `string?` | The provider's integration id, when connected through an integration. |
| `free` | `boolean` | Whether the model is marked free. |
| `paid` | `boolean` | Whether any of its token prices is above zero. |

### ModelBadge

A short inline annotation a plugin attaches to a model row in the model picker (`Models:SetBadges`).

| Field | Type | Description |
|---|---|---|
| `providerId` | `string` | The provider id. |
| `modelId` | `string` | The model id. |
| `label` | `string` | Short text shown beside the model name and in its effort flyout. |
| `critical` | `boolean?` | Emphasizes the annotation in danger ink (for example, a reached limit or blocked model). |

### ComposerModel

The model selected in the composer.

| Field | Type | Description |
|---|---|---|
| `providerId` | `string` | The provider id. |
| `modelId` | `string` | The model id. |
| `name` | `string` | Its display name. |

### Resource

A cached server read.

| Field | Type | Description |
|---|---|---|
| `status` | `ResourceStatus` | Its state. |
| `json` | `any?` | The answer as JSON, when it is JSON and the read didn't ask for `items`. |
| `items` | `{ResourceItem}?` | For a read that asked for them, the items its answer lists (`{ "data": [...] }` or `[...]`), split off the UI thread; `json` is the rest of the answer then. |
| `text` | `string?` | The answer as text. |
| `error` | `string?` | Why it failed. |
| `generation` | `number` | Changes with every new answer: put it in a memo's version. |

### ResourceItem

One item a read's answer lists.

| Field | Type | Description |
|---|---|---|
| `id` | `string` | Its `id`. |
| `kind` | `string` | Its `type`. |
| `created` | `number?` | Its `time.created`. |
| `text` | `string` | Its JSON. |

### ServerResponse

A server's answer.

| Field | Type | Description |
|---|---|---|
| `ok` | `boolean` | A 2xx status. |
| `status` | `number` | The HTTP status. 0 when nothing answered. |
| `json` | `any?` | The body as JSON, when it is JSON. |
| `text` | `string` | The body as text. |
| `error` | `string?` | Why it failed. |

### ServerEvent

One event from the server's event stream.

| Field | Type | Description |
|---|---|---|
| `type` | `string` | The event type, such as `"session.updated"`. |
| `data` | `any` | The event's payload. |
| `serverId` | `string` | The key of the server that sent it: a `ServerInfo.key`, `local` for the app's own server. |
| `directory` | `string?` | The workspace folder it concerns. |

### ServerInfo

A server the app lists.

| Field | Type | Description |
|---|---|---|
| `key` | `string` | Its key: `local` for the app's own server, `<plugin>:<id>` for a contributed server, or the address of a server added by address. |
| `name` | `string` | Its display name. |
| `label` | `string?` | Its badge, such as `"SSH"`. |
| `state` | `ServerState` | Where it stands. |
| `isLocal` | `boolean` | It runs on this machine. |
| `current` | `boolean` | This window works with it now: the server of the session or draft it shows, or of its Home. |
| `version` | `string?` | The version it reported. |

### ServerHealth

A server's latest health check.

| Field | Type | Description |
|---|---|---|
| `healthy` | `boolean` | It answered. |
| `version` | `string?` | The version it reported. |
| `incompatible` | `boolean?` | This app does not support that version. |
| `checking` | `boolean?` | A check is in flight. |

### ServerEndpoint

How to reach a ready server.

| Field | Type | Description |
|---|---|---|
| `url` | `string` | The base URL. |
| `password` | `string?` | The HTTP basic password. The user name is always `opencode`. |

### ServerEntry

One server a server source lists. Its key is `<plugin>:<id>`.

| Field | Type | Description |
|---|---|---|
| `id` | `string` | The entry's id within the source. |
| `name` | `string` | The display name. |
| `label` | `string?` | A short badge after the name, such as `"WSL"`. |
| `state` | `ServerState` | Where it stands. |
| `listed` | `boolean?` | False keeps it out of the app's server list. Settings still shows it. Defaults to true. |
| `managed` | `boolean?` | The plugin keeps it reachable, as an SSH tunnel does: while the app checks a new endpoint, it shows a connecting spinner, not an unknown dot. Defaults to false. |
| `http` | `ServerEndpoint?` | Its endpoint, once `ready`. |

### BasicAuth

HTTP basic credentials.

| Field | Type | Description |
|---|---|---|
| `user` | `string` | The user name. |
| `password` | `string` | The password. |

### HttpRequest

An HTTP request.

| Field | Type | Description |
|---|---|---|
| `url` | `string` | The URL. |
| `method` | `HttpMethod?` | Defaults to GET. |
| `headers` | `{ [string]: string }?` | Request headers. |
| `body` | `string?` | A text body. |
| `json` | `any?` | A JSON body, instead of `body`. Sets the content type. |
| `basicAuth` | `BasicAuth?` | Sends `Authorization: Basic`. |
| `timeout` | `number?` | Seconds before it gives up. Defaults to 30. |

### HttpResponse

An HTTP answer.

| Field | Type | Description |
|---|---|---|
| `ok` | `boolean` | A 2xx status. |
| `status` | `number` | The HTTP status. 0 when nothing answered. |
| `headers` | `{ [string]: string }` | Response headers, with lowercase names. |
| `text` | `string` | The body as text. |
| `json` | `any?` | The body as JSON, when it is JSON. |
| `error` | `string?` | Why it failed. |

### ProcessSpec

A process to start.

| Field | Type | Description |
|---|---|---|
| `program` | `string` | The program, as a path or a name on PATH. |
| `args` | `{ string }?` | Its arguments, passed as they are, without a shell. |
| `env` | `{ [string]: string }?` | Variables to add to the app's environment. |
| `cwd` | `string?` | Its working folder. |
| `stdin` | `string?` | Written to its stdin first. Stdin then closes unless `keepStdin` is set. |
| `keepStdin` | `boolean?` | Keeps stdin open for `Write`. |
| `timeout` | `number?` | Seconds before the host kills it. |
| `encoding` | `TextEncoding?` | How it encodes its output. Defaults to UTF-8, or UTF-16LE when its first output reads as that. |
| `stderrFile` | `boolean?` | Sends stderr to a file rather than a pipe, read back as one `stderr` chunk as it exits: for programs that never exit after writing to a stderr pipe, as Windows' OpenSSH does after a failure. |

### ProcessExit

How a process ended.

| Field | Type | Description |
|---|---|---|
| `code` | `number?` | Its exit code, when it has one. |
| `error` | `string?` | Why it could not start. |
| `timedOut` | `boolean` | Its timeout killed it. |
| `killed` | `boolean` | `Kill`, or the plugin going away, ended it. |

### ProcessOutput

A finished process and all its output.

| Field | Type | Description |
|---|---|---|
| `ok` | `boolean` | It ran and exited with 0. |
| `code` | `number?` | Its exit code. |
| `stdout` | `string` | Everything it wrote to stdout. |
| `stderr` | `string` | Everything it wrote to stderr. |
| `error` | `string?` | Why it could not start. |
| `timedOut` | `boolean` | Its timeout killed it. |

### ProcessHandle

A process the host supervises. The host kills it when the plugin half that started it goes away.

| Member | Type | Description |
|---|---|---|
| `.Id` | `number` | The host's id for it. |
| `:Running` | `() -> boolean` | It has not exited. |
| `:Write` | `(text: string, close: boolean?) -> boolean` | Writes to its stdin, then closes stdin when `close` is true. False when stdin is closed. |
| `:Kill` | `() -> ()` | Kills it. `Exited` follows. |
| `:WaitAsync` | `() -> ProcessExit` | Yields until it exits. Yields. |
| `.Output` | `Signal<ProcessStream, string>` | Fires with each chunk of output, line-buffered. |
| `.Exited` | `Signal<ProcessExit>` | Fires once as it exits. |

### StoreValue

A value the host saves for this plugin with the app's state. Written `StoreValue<T>`.

| Member | Type | Description |
|---|---|---|
| `:Get` | `() -> T` | The value, or the default. |
| `:Set` | `(value: T) -> ()` | Saves a new value. It must be plain data: nil, booleans, numbers, strings and tables of them. Setting the value it already holds writes nothing. |
| `:Version` | `() -> number` | Changes with every `Set`: put it in a memo's version. |
| `.Changed` | `Signal<T>` | Fires with the new value, also when another window sets it. |

### SessionStoreValue

A value the host saves for this plugin per session. Each keeps the 50 sessions used last (read or set): a 51st session's value replaces the one used longest ago. Written `SessionStoreValue<T>`.

| Member | Type | Description |
|---|---|---|
| `:Get` | `(sessionId: string) -> T` | The session's value, or the default. |
| `:Set` | `(sessionId: string, value: T) -> ()` | Saves a new value for the session. It must be plain data. Setting the value it already holds writes nothing. |
| `:Version` | `() -> number` | Changes with every `Set` for any session: put it in a memo's version. |
| `.Changed` | `Signal<string, T>` | Fires with the session id and its new value. |

### ToastAction

A button on a toast.

| Field | Type | Description |
|---|---|---|
| `label` | `string` | The button's label. |
| `revealPath` | `string?` | Shows this path in the system's file manager. |
| `openUrl` | `string?` | Opens this URL in the system's browser. |
| `run` | `(() -> ())?` | Runs when the user clicks it. The toast then closes. |
| `variant` | `ToastActionVariant?` | Its look: `primary` (the default) or `secondary`. |

### ToastSpec

What a toast shows.

| Field | Type | Description |
|---|---|---|
| `title` | `string` | The headline. |
| `description` | `string?` | More detail under the title. |
| `variant` | `ToastVariant?` | The tone. Defaults to `default`. |
| `icon` | `IconName?` | An icon before the title. |
| `duration` | `number?` | Seconds before it closes. Ignored when `persistent`. Defaults to 5. |
| `persistent` | `boolean?` | Stays until the user or `Dismiss` closes it. |
| `actions` | `{ ToastAction }?` | Buttons, in order. |

### ToastHandle

One toast the plugin showed. It closes when the plugin goes away.

| Member | Type | Description |
|---|---|---|
| `:Dismiss` | `() -> ()` | Closes it. Does nothing once it closed. |
| `:Update` | `(spec: ToastSpec) -> ()` | Replaces what it shows, in place. |

### PopoverOpenSpec

Where to open one of this plugin's popovers.

| Field | Type | Description |
|---|---|---|
| `id` | `string` | The popover's id, from `ctx:AddPopover`. |
| `anchorKey` | `string` | The `key` of the element it opens beside. |
| `triggerKey` | `string?` | The `key` of the button that opens it, which takes focus back when it closes (Kobalte's `onCloseAutoFocus`), however it was opened. Defaults to what had focus when it opened. |
| `side` | `Side?` | The anchor's side. Defaults to below. |
| `align` | `Align?` | Alignment along that side. Defaults to start. |
| `gap` | `number?` | Points between the anchor and the popover. |
| `data` | `{ [string]: any }?` | Passed to the popover's render as `props.data`. |
| `transition` | `Transition?` | How it appears. |

### MenuEntry

One entry of a menu.

| Field | Type | Description |
|---|---|---|
| `kind` | `MenuEntryKind?` | Defaults to `item`. |
| `id` | `string?` | An item's id, returned when the user picks it. |
| `title` | `string?` | An item's label, or a group's title. |
| `icon` | `IconName?` | An item's icon. |
| `detail` | `string?` | Muted text at an item's end. |
| `keybind` | `string?` | A published command id whose shortcut the item shows. |
| `keys` | `string?` | A literal shortcut the item shows, such as `"mod+plus"`, when no command has it. |
| `enabled` | `boolean?` | False shows the item disabled. Defaults to true. |
| `danger` | `boolean?` | A destructive item. |
| `checked` | `boolean?` | Shows a check mark. |

### MenuOpenSpec

A menu to open beside an element.

| Field | Type | Description |
|---|---|---|
| `anchorKey` | `string` | The `key` of the element it opens beside. |
| `items` | `{ MenuEntry }` | Its entries. |
| `label` | `string?` | Its accessible name. |
| `side` | `Side?` | The anchor's side. Defaults to below. |
| `align` | `Align?` | Alignment along that side. Defaults to start. |
| `minWidth` | `number?` | Its minimum width in points. |
| `width` | `number?` | Its width in points. |

### DraftSpec

A new session's draft, opened with `Layout:OpenDraft`.

| Field | Type | Description |
|---|---|---|
| `serverKey` | `string?` | The server it runs on: a `ServerInfo.key`. Defaults to the server this window works with. |
| `directory` | `string` | Its workspace folder on that server. |
| `prompt` | `string?` | Text typed into its composer. |

### OpenPanelOptions

How to open a panel tab.

| Field | Type | Description |
|---|---|---|
| `sessionId` | `string?` | The session. Defaults to the routed session. |
| `instance` | `string?` | The tab of an instance panel. |
| `background` | `boolean?` | Opens the tab without selecting it or switching the narrow-screen view. |
| `transition` | `Transition?` | How the side region opens. |

### Link

A local link the app routes to a link handler.

| Field | Type | Description |
|---|---|---|
| `href` | `string` | The target: a path, a URL or a `file://` link. |
| `origin` | `string?` | The plugin that produced the linked item. |
| `exact` | `boolean?` | The path is a known workspace file, not a guess from text. |
| `background` | `boolean?` | Must not switch the narrow-screen view. |
| `base` | `string?` | The workspace-relative folder the link was written in. |
| `sessionId` | `string?` | The session the link belongs to. |

### NotificationSpec

A system notification. It follows the user's notification settings.

| Field | Type | Description |
|---|---|---|
| `title` | `string` | The title. |
| `body` | `string?` | The text under it. |
| `sessionId` | `string?` | Clicking it opens this session. |
| `sound` | `boolean?` | Plays the user's notification sound. |

### ThemeInfo

The window's theme.

| Field | Type | Description |
|---|---|---|
| `id` | `string` | The theme's key. |
| `name` | `string` | Its display name. |
| `scheme` | `ColorScheme` | The color scheme in use. |

### SaveResult

Where a file was saved.

| Field | Type | Description |
|---|---|---|
| `path` | `string?` | Its path. |
| `error` | `string?` | Why it could not be saved. |

### ReadResult

A file's text.

| Field | Type | Description |
|---|---|---|
| `text` | `string?` | Its text, as UTF-8. |
| `error` | `string?` | Why it could not be read. |

### OpenInApp

An app that can open a path, such as an editor or a file manager.

| Field | Type | Description |
|---|---|---|
| `id` | `string` | Its id, for `Workspace:OpenIn`. |
| `name` | `string` | Its display name. |
| `icon` | `ImageId?` | Its icon. |

### CodeDocument

A file open for `ui.codeView`. The host closes it when the plugin half goes away.

| Member | Type | Description |
|---|---|---|
| `.Path` | `string` | Its path. |
| `:Version` | `() -> number` | Changes when its content does: put it in a memo's version. |
| `:Missing` | `() -> boolean` | The file no longer exists. |
| `:Binary` | `() -> boolean` | The file is not text. |
| `:Reload` | `() -> ()` | Reads it again. |
| `:Close` | `() -> ()` | Releases it. |
| `.Changed` | `Signal<()>` | Fires when its content changes on disk. |

### FileTreeModel

A folder's tree for `ui.fileTree`.

| Member | Type | Description |
|---|---|---|
| `.Root` | `string` | The root folder. |
| `:Expand` | `(path: string) -> ()` | Expands a folder. |
| `:Collapse` | `(path: string) -> ()` | Collapses a folder. |
| `:SetFilter` | `(query: string) -> ()` | Shows only paths that match a fuzzy query. An empty query shows all. |
| `:Find` | `(query: string) -> { string }` | The files whose path holds `query`, best first: what `ui.fileTree`'s `filter` lists for it. |
| `:Reveal` | `(path: string) -> ()` | Expands the folders above a path and scrolls its row into view. |
| `:Refresh` | `() -> ()` | Reads the folder again. |
| `:Version` | `() -> number` | Changes when the tree does. |
| `:Close` | `() -> ()` | Releases it. |
| `.Changed` | `Signal<()>` | Fires when the tree changes. |

### DiffTotals

A diff's size.

| Field | Type | Description |
|---|---|---|
| `files` | `number` | Changed files. |
| `added` | `number` | Added lines. |
| `removed` | `number` | Removed lines. |

### DiffFile

One changed file.

| Field | Type | Description |
|---|---|---|
| `path` | `string` | Its path. |
| `status` | `string` | `added`, `modified`, `deleted` or `renamed`. |
| `added` | `number` | Added lines. |
| `removed` | `number` | Removed lines. |

### DiffOpenSpec

Which changes to open.

| Field | Type | Description |
|---|---|---|
| `sessionId` | `string` | The session. |
| `source` | `DiffSource` | What to compare. |
| `messageId` | `string?` | A `turn` source's user message. |

### DiffDocument

A set of changes for `ui.diffView`.

| Member | Type | Description |
|---|---|---|
| `:Totals` | `() -> DiffTotals` | Its size. |
| `:Files` | `() -> { DiffFile }` | Its files. |
| `:SetSource` | `(source: DiffSource, messageId: string?) -> ()` | Compares something else. |
| `:Source` | `() -> DiffSource` | What it compares. |
| `:Sources` | `() -> { DiffSource }` | What it can compare in this folder: `branch` only on a branch other than the default one. |
| `:Status` | `() -> DiffStatus` | How the read went. |
| `:Retry` | `() -> ()` | Reads again after a failure. |
| `:Error` | `() -> string?` | Why the read failed, in the app's words, while `Status` is `error` or `offline`. |
| `:Active` | `() -> string?` | The file the preview shows: the one focused last while it still changed, else the first the server lists. |
| `:Focus` | `(path: string) -> ()` | Shows a changed file in the preview, and selects it in the tree. |
| `:Tree` | `() -> FileTreeModel` | Its changed files as a tree for `ui.fileTree`, every folder open until the user shuts it. |
| `:Loading` | `() -> boolean` | A read is in flight. |
| `:Version` | `() -> number` | Changes when the changes do. |
| `:Close` | `() -> ()` | Releases it. |
| `.Changed` | `Signal<()>` | Fires when the changes change. |

### TerminalSpec

A terminal session to start on the server.

| Field | Type | Description |
|---|---|---|
| `directory` | `string` | The workspace folder. |
| `cwd` | `string?` | The shell's working folder. Defaults to `directory`. |
| `title` | `string?` | Its initial title. |
| `command` | `string?` | A program to run instead of the user's shell. |
| `args` | `{ string }?` | Its arguments. |

### PtySession

A terminal session for `ui.terminalSurface`.

| Member | Type | Description |
|---|---|---|
| `.Id` | `string` | The server's id for it. |
| `:Title` | `() -> string` | Its title, as the shell sets it. |
| `:Running` | `() -> boolean` | Its process has not exited. |
| `:Write` | `(text: string) -> ()` | Types text into it. |
| `:Close` | `() -> ()` | Ends it. |
| `.Exited` | `Signal<number?>` | Fires with the exit code as its process exits. |
| `.TitleChanged` | `Signal<string>` | Fires with the new title. |

### TerminalInfo

One terminal of a session, as its tab lists it.

| Field | Type | Description |
|---|---|---|
| `id` | `string` | The server's id for it. |
| `title` | `string` | Its title, as the shell or a rename set it. Empty before the server names it. |
| `number` | `number` | Its place among the session's terminals, from 1: what a default title says. |
| `hidden` | `boolean` | Listed only to hold a restored tab until the server lists the session's terminals. |

### OpenFileOptions

How `Workspace:OpenFile` opens a file.

| Field | Type | Description |
|---|---|---|
| `preview` | `boolean?` | Opens it in the preview tab, which the next preview replaces, instead of a kept one. |
| `startLine` | `number?` | The first line to select and scroll into view, from 1. |
| `endLine` | `number?` | The last line to select. Defaults to `startLine`. |

### TerminalOpened

The terminal tab `Terminal:OpenAsync` showed.

| Field | Type | Description |
|---|---|---|
| `id` | `string?` | The terminal's id; nil when the session has none and can't start one. |
| `placement` | `string` | Where a session's first terminal opens (Settings > Terminal): `side` or `bottom`. |

### BrowserOpenSpec

A page to open.

| Field | Type | Description |
|---|---|---|
| `url` | `string` | The URL, or a local `:port`. Defaults to `about:blank`. |
| `sessionId` | `string?` | The session whose side region gets the page's tab. Defaults to the routed session. |
| `background` | `boolean?` | Opens the tab without moving the keyboard: focus stays where it is. |
| `pane` | `string?` | The side-region pane (`side`, `split`, or `bottom`) to open the page's tab in. |

### BrowserShowOptions

Where to show a page's tab.

| Field | Type | Description |
|---|---|---|
| `sessionId` | `string?` | The session whose side region shows it. Defaults to the routed session. |
| `background` | `boolean?` | Selects the tab without moving the keyboard. |

### BrowserTabInfo

One page of a session, as its tab lists it.

| Field | Type | Description |
|---|---|---|
| `id` | `string` | The host's id for it. |
| `url` | `string` | Its address; empty for a blank page. |
| `title` | `string` | Its title; empty before the page names itself. |
| `open` | `boolean` | The page exists. A stored tab the engine never opened in this run is listed, not open. |

### BrowserEngine

Where the browser engine stands.

| Field | Type | Description |
|---|---|---|
| `state` | `BrowserEngineState` | Its state. |
| `message` | `string?` | Why it failed, for `failed`. |
| `downloads` | `boolean` | The engine that runs (or last ran) keeps a page's downloads for `BrowserTab:Files`; false before one has run, and for an engine from before downloads. |
| `policy` | `boolean` | The engine that runs (or last ran) confines frame and sub-resource requests to the page's URL policy; false before one has run, and for an engine from before URL policy. |

### BrowserPick

An element the user picked in a page with the element picker.

| Field | Type | Description |
|---|---|---|
| `tabId` | `string` | The page. |
| `ref` | `string` | The element's `@eN` reference. |
| `selector` | `string` | A CSS selector for it. |

### BrowserTab

A page in the app's browser, for `ui.browserSurface`.

| Member | Type | Description |
|---|---|---|
| `.Id` | `string` | The host's id for it. |
| `:IsOpen` | `() -> boolean` | The page exists: it hasn't closed. |
| `:Url` | `() -> string` | The current URL. |
| `:Title` | `() -> string` | The page title. |
| `:Loading` | `() -> boolean` | A load is in flight. |
| `:CanGoBack` | `() -> boolean` | History has an earlier page. |
| `:CanGoForward` | `() -> boolean` | History has a later page. |
| `:Zoom` | `() -> number` | The page's zoom: 1 at 100%. |
| `:Inspecting` | `() -> boolean` | The element picker waits for a click in the page. |
| `:Files` | `() -> { BrowserFile }` | What the page downloaded, oldest first: each file in the engine's private downloads folder, with no dialog, until its tab closes or the engine ends; one over 5 MiB is cancelled, and a tab keeps at most 64. Empty with an engine that doesn't keep downloads (`Browser:Engine().downloads`). |
| `:LoadError` | `() -> string?` | Why the page shows no document: the engine's error for a load that failed (`net::ERR_CONNECTION_REFUSED`), or `page_crashed` when its renderer went away; nil while it shows one. A reload or another address loads it again. |
| `:Refusal` | `() -> string?` | Why the last navigation was refused while keeping the page (`browser.address.credentials`, `browser.address.workspace`, `browser.address.web`, or `ERR_BLOCKED_BY_CLIENT`); nil once an allowed navigation starts. |
| `:Rejected` | `() -> number` | Count of refused navigations on this page, so a repeated refusal still ends the address field's hold. |
| `:Navigate` | `(url: string) -> ()` | Loads a URL, or a local `:port`. |
| `:Back` | `() -> ()` | Goes back. |
| `:Forward` | `() -> ()` | Goes forward. |
| `:Reload` | `() -> ()` | Reloads. |
| `:Stop` | `() -> ()` | Stops loading. |
| `:SetZoom` | `(direction: BrowserZoom) -> ()` | Zooms the page in or out a step, or back to 100%. The site keeps its zoom. |
| `:Inspect` | `(enabled: boolean?) -> ()` | Starts or stops the element picker; without an argument, toggles it. A pick fires `Browser.Picked`. |
| `:Viewport` | `() -> BrowserViewport?` | The fixed CSS viewport, or nil when the page follows the pane. |
| `:SetViewport` | `(viewport: BrowserViewport?) -> ()` | Apply a device viewport in a local or dev build, or nil to restore the page. |
| `:Highlight` | `(nodeId: number?) -> ()` | Outlines the DevTools backend node `nodeId` in the page, or clears the outline. |
| `:Show` | `(options: BrowserShowOptions?) -> ()` | Selects its tab in a session's side region, opening the region and adding the tab when it isn't there. |
| `:Close` | `() -> ()` | Closes the page and its tab. |
| `:DevtoolsAsync` | `(method: string, params: { [string]: any }?, targetSessionId: string?) -> any` | Sends one allow-listed DevTools protocol command and returns its result. Raises when the method or parameters are not permitted, when the engine fails, or after 10 seconds. Yields. |
| `:SiteCookiesAsync` | `() -> number` | Counts the cookies in use for the page's HTTP(S) address; 0 for blank or workspace file pages. Yields. |
| `:ClearSiteAsync` | `() -> ()` | Deletes cookies and origin storage for the page's HTTP(S) address and reloads the page. Yields. |
| `:ReadFileAsync` | `(fileId: string) -> BrowserFileContents` | Reads a completed download of the page: its text when it is UTF-8, else its bytes in base64. Raises when the file is unknown, still downloading, failed, or over 5 MiB. Yields. |

### BrowserFile

A file a page downloaded (`BrowserTab:Files`).

| Field | Type | Description |
|---|---|---|
| `id` | `string` | `file_<n>`, for `BrowserTab:ReadFileAsync`. |
| `name` | `string` | The name the page gave it. |
| `mime` | `string` | Its MIME type. |
| `bytes` | `number` | Bytes received so far. |
| `state` | `string` | `pending`, `completed` or `failed`. |
| `error` | `string?` | Why it failed: `too_large`, `canceled`, or Chromium's interrupt reason (`network_failed`, `file_no_space`, ...). |

### BrowserFileContents

A downloaded file's contents (`BrowserTab:ReadFileAsync`).

| Field | Type | Description |
|---|---|---|
| `name` | `string` | The name the page gave it. |
| `mime` | `string` | Its MIME type. |
| `bytes` | `number` | Its size. |
| `text` | `string?` | Its text, when it is UTF-8. |
| `base64` | `string?` | Its bytes in base64, when it isn't. |

### OfficeDocument

An Office file the host opened, laid out on a thread of its own. `ui.officePages` and `ui.officeSheet` draw it; `Office:Close` releases it.

| Field | Type | Description |
|---|---|---|
| `id` | `number` | The host's id for it. |
| `kind` | `OfficeKind` | Its format. |
| `pages` | `number` | Pages or slides; 0 for a workbook. |
| `sheets` | `{ string }` | A workbook's sheet names, in order. |

### OfficeFile

What `Office:OpenAsync` found: the file's size and first bytes, and the document when they are an Office Open XML package that opened.

| Field | Type | Description |
|---|---|---|
| `size` | `number` | The file's size in bytes. |
| `signature` | `OfficeSignature` | What its first bytes are. |
| `document` | `OfficeDocument?` | The opened document; nil when the bytes are no package or the engine failed. |
| `error` | `string?` | Why it did not open. |

### OfficePagesLabels

The words `ui.officePages` shows, in the plugin's language.

| Field | Type | Description |
|---|---|---|
| `page` | `string` | A page's label, `{{number}}` its number from 1: "Page {{number}}" or "Slide {{number}}". |
| `hiddenSlide` | `string?` | The badge under a hidden slide. |
| `failed` | `string` | Over a page that could not be drawn. |
| `speakerNotes` | `string?` | The label of a slide's notes. |
| `layoutFailed` | `string?` | Under the last page when the rest could not be laid out. |

### OfficeSheetLabels

The words `ui.officeSheet` says: the grid's name for screen readers, and the notices a copy shows.

| Field | Type | Description |
|---|---|---|
| `name` | `string?` | The grid's accessible name: the sheet's name. |
| `copyTooLarge` | `string?` | The toast when a copy holds more than 200,000 cells. |
| `copyFailed` | `string?` | The toast when the cells could not be read. |

### UpdaterState

Where the app updater stands.

| Field | Type | Description |
|---|---|---|
| `phase` | `UpdaterPhase` | The phase. |
| `version` | `string?` | The available version. |
| `progress` | `number?` | Download progress, 0 to 1. |
| `notes` | `string?` | The release notes, as Markdown. |
| `error` | `string?` | Why it failed. |

### PowerLock

Keeps the display awake while held.

| Member | Type | Description |
|---|---|---|
| `:Release` | `() -> ()` | Lets the display sleep again. |

### QrCode

A QR code's module matrix.

| Field | Type | Description |
|---|---|---|
| `size` | `number` | Modules per side. |
| `modules` | `{ boolean }` | `size * size` modules, row by row: true is dark. |
| `icon` | `IconName` | Its dark modules inside a 4-module quiet zone, as an icon `ui.icon` draws crisp at any size in its color. |

### ImageState

Where an image stands, from `Images:State`.

| Field | Type | Description |
|---|---|---|
| `status` | `ImageStatus` | Loading, loaded or failed. |
| `width` | `number?` | Its width in pixels, once loaded. |
| `height` | `number?` | Its height in pixels, once loaded. |
| `error` | `string?` | Why it failed. |

### AskpassPrompt

A credential prompt from a child process.

| Member | Type | Description |
|---|---|---|
| `.Id` | `string` | Its id within the listener, as `Cleared` names it. |
| `.Prompt` | `string` | What the process asks. |
| `.Secret` | `boolean` | The answer is a password. False for a yes/no question, such as trusting a host's key. |
| `:Respond` | `(answer: string?) -> ()` | Answers it. Nil cancels. |

### AskpassListener

A loopback endpoint that turns `SSH_ASKPASS` prompts into signals.

| Member | Type | Description |
|---|---|---|
| `.Env` | `{ [string]: string }` | Environment variables to give the child process. |
| `.Prompted` | `Signal<AskpassPrompt>` | Fires with each prompt, one at a time. |
| `.Cleared` | `Signal<string>` | Fires with a prompt's id once it ends: answered, cancelled, or its process stopped asking. |
| `:Close` | `() -> ()` | Stops listening. |

### ConsoleLine

One line of the Studio console.

| Field | Type | Description |
|---|---|---|
| `plugin` | `string` | The plugin that wrote it. |
| `level` | `ConsoleLevel` | How serious it is. |
| `message` | `string` | What it says. An error's traceback follows its first line. |
| `file` | `string?` | The `.luau` file the message names, such as `init.luau`. |
| `line` | `number?` | The line in `file`. |
| `href` | `string?` | A `file://` link to that line, when the plugin runs from a folder. |
| `count` | `number` | How many times it was written in a row. |

### ConsoleEvalResult

What `Console:Eval` gave back.

| Field | Type | Description |
|---|---|---|
| `ok` | `boolean` | It ran without an error. |
| `value` | `string?` | Its value as text. |
| `error` | `string?` | The error it raised. |

### PluginMetrics

A plugin's live costs.

| Field | Type | Description |
|---|---|---|
| `memoryKb` | `number` | Luau heap in KB. |
| `renderUs` | `number` | The last render's cost in microseconds. |
| `memoHitRate` | `number` | Share of memo lookups that reused their subtree, 0 to 1. |

### CommandSlash

A command's slash form in the composer.

| Field | Type | Description |
|---|---|---|
| `name` | `string` | The name after the slash, such as `open` for `/open`. |
| `arguments` | `boolean?` | The text after the name goes to `run`. |
| `after` | `string?` | Lists the entry right after the slash entry with this name. |

### CommandSpec

A command in the palette, with an optional keybind and slash command. Published as `<plugin>.<id>`. Desktop's plugin API calls it `Command`.

| Field | Type | Description |
|---|---|---|
| `id` | `string` | Its id within the plugin. |
| `title` | `string` | The palette label. |
| `description` | `string?` | The palette's second line. |
| `keywords` | `string?` | More words the palette matches, separated by spaces. |
| `group` | `string?` | The palette category. |
| `order` | `number?` | Palette order, ascending. Commands without an order are listed after ordered commands. Since 1.5. |
| `section` | `CommandSection?` | The Settings › Shortcuts section. Defaults to `general`. |
| `bind` | `string?` | The default keybind, such as `"mod+shift+t"`. |
| `follows` | `string?` | The id of the command whose keybind this one shares, such as `"tab.close"`: Settings > Shortcuts lists that one and leaves this one out, unless the user has bound this one itself (Desktop's `follows`). Since 1.7. |
| `slash` | `CommandSlash?` | Also offers it as a slash command. |
| `hidden` | `(boolean \| (() -> boolean))?` | Keeps it out of the palette. The keybind still works. A function makes it reactive: the host calls it as it lists commands, and again only once something it read changes. Keep it cheap, and don't yield. |
| `suggested` | `boolean?` | Also lists it under Suggested. |
| `featured` | `boolean?` | Lists it when the palette opens without a query. |
| `enabled` | `(boolean \| (() -> boolean))?` | False takes it out of the palette and the slash list and turns its keybind off, and `Commands:Run` declines it. Defaults to true. A function makes it reactive, as Desktop's command accessor is: the host calls it as it lists commands or matches a key, and again only once something it read changes (`Layout:Narrow`, the routed session, a store value). Keep it cheap, and don't yield. |
| `editable` | `boolean?` | The keybind also fires while a text field has focus. |
| `requiresSession` | `boolean?` | Offered only while a session is open. |
| `run` | `(input: string?) -> ...any` | Runs it, as a coroutine. `input` is the slash command's arguments. Return `false` before the first yield to decline the key, so it reaches its target; any other return handles it. |

### PanelProps

What a panel's render receives.

| Field | Type | Description |
|---|---|---|
| `sessionId` | `string` | The routed session. |
| `directory` | `string` | Its workspace folder. |
| `instance` | `string?` | An instance panel's tab. |
| `active` | `boolean` | The tab is selected. |
| `placement` | `"side" \| "mobile"` | Where it renders. |
| `restored` | `boolean?` | In `onFocus`: the selection came back with a restored strip rather than from the user. |
| `width` | `number` | The pane's width in points: lay out for it as a container query would. |
| `height` | `number` | The pane's height in points. |
| `drawer` | `{ [string]: any }?` | In a narrow window's More drawer, the nested view this panel opened there with `Layout:OpenDrawerView`: its `data`. Nil while the drawer shows the panel's own view. |

### PanelTabLabelProps

What a panel's `label` render receives.

| Field | Type | Description |
|---|---|---|
| `sessionId` | `string` | The routed session. |
| `instance` | `string?` | An instance panel's tab. |
| `active` | `boolean` | The tab is selected. |
| `preview` | `boolean` | The tab is the replaceable preview tab. |

### PanelTab

One tab an instance panel lists.

| Field | Type | Description |
|---|---|---|
| `instance` | `string` | The instance id. |
| `title` | `string` | The accessible name, and the label without a `label` render. |
| `pinned` | `boolean?` | Listed before every other tab, never closed or dragged. |
| `missing` | `boolean?` | Struck through, such as a deleted file. |
| `file` | `string?` | The workspace file the tab shows. |
| `icon` | `IconName?` | Its icon, instead of the panel's. A tab showing `file` defaults to the file's type icon. |
| `transient` | `boolean?` | A launcher tab: the next preview replaces it, and closing it keeps nothing in Recently closed. |
| `draggable` | `boolean?` | Drags between panes. Defaults to true. |
| `fallback` | `boolean?` | Selected when the selected tab closes. |
| `hidden` | `boolean?` | Listed but not drawn in the strip: it holds a restored selection until the panel can show it. |

### ClosedTab

What Recently closed lists for a panel's closed tab.

| Field | Type | Description |
|---|---|---|
| `title` | `string?` | Its title, instead of the tab's. |
| `icon` | `IconName?` | Its icon. |
| `detail` | `string?` | Muted text after its title, such as a page's host. |
| `data` | `string?` | What `restore` gets back, such as a page's address. |

### PanelMobileView

A panel's view in a narrow window (below 768 points), where the side region folds into a Session / More switcher. Its render gets `placement` `"mobile"`. Desktop's plugin API calls it `MobileView`.

| Field | Type | Description |
|---|---|---|
| `title` | `string` | Its label in the switcher or the More drawer. |
| `order` | `number` | Views list in ascending order. |
| `kind` | `PanelMobileKind` | Where it is offered: a switcher tab, a More row that replaces the conversation, or a More row that opens in the drawer. |
| `icon` | `IconName?` | Its icon in the More drawer. |
| `cached` | `boolean?` | Once shown, it stays mounted while another view shows. Defaults to false: it mounts each time it is shown. |

### PanelSpec

A panel: tabs in a session's side region. Desktop's plugin API calls it `Panel`.

| Field | Type | Description |
|---|---|---|
| `id` | `string` | Its id within the plugin. |
| `title` | `string` | Its tab and launcher title. |
| `description` | `string?` | The launcher card's second line. |
| `icon` | `IconName?` | Its icon. |
| `order` | `number?` | Launcher order, ascending. |
| `wide` | `boolean?` | Asks for the wider session minimum while open. |
| `width` | `number?` | A width of its own for the side region, in points. |
| `transient` | `boolean?` | Its tabs are not restored. |
| `launcher` | `boolean?` | Offered in the + menu and the New tab view. Defaults to true. |
| `first` | `boolean?` | Its tab sits at the front of the strip. |
| `fallback` | `boolean?` | Its tab is selected when the selected tab closes. |
| `mobile` | `PanelMobileView?` | Its view in a narrow window: a switcher tab or a More drawer row. Without one, a narrow window doesn't offer it. |
| `enabled` | `(boolean \| ((sessionId: string) -> boolean))?` | False withdraws the panel: no tab, launcher or narrow view. Defaults to true. A function offers it per session, as Desktop's panel accessor does (`session.project` for Session details): the host calls it with the routed session as it lists panels, and again only once something it read changes. Keep it cheap, and don't yield. |
| `pane` | `PanelPane?` | The pane a new tab opens in beside none of the plugin's others. Defaults to `side`. |
| `closed` | `ClosedTab?` | What Recently closed lists for its closed tab. |
| `isWide` | `(() -> boolean)?` | Asks for the wider session minimum while it returns true. Read on every layout: keep it cheap, and don't yield. |
| `tabTitle` | `((sessionId: string) -> string)?` | Its tab's title in a session, instead of `title`, such as a count that changes. Keep it cheap, and don't yield. |
| `list` | `((sessionId: string, open: { string }) -> { PanelTab })?` | Makes it an instance panel: lists the session's tabs from the stored instance ids in `open`. A tab's key is `<plugin>:<instance>`; a panel without `list` has one tab, `<plugin>:<id>`. |
| `render` | `(view: View, props: PanelProps) -> UiNode?` | Draws a tab's content. |
| `label` | `((view: View, props: PanelTabLabelProps) -> UiNode?)?` | Draws the tab's label in the strip, instead of its icon and title. |
| `onClose` | `((props: PanelProps) -> ())?` | Runs after the tab leaves the strip. |
| `onFocus` | `((props: PanelProps) -> ())?` | Runs as the tab becomes selected. |
| `closedTab` | `((props: PanelProps) -> ClosedTab?)?` | What Recently closed keeps of a tab as it closes, instead of `closed`; nil keeps nothing. Keep it cheap, and don't yield. |
| `restore` | `((props: PanelProps, data: string?) -> ())?` | Brings a tab back from Recently closed with its `ClosedTab.data`. Without it, the tab's key opens again. |

### SettingSpec

A row in Settings › Plugins, under this plugin.

| Field | Type | Description |
|---|---|---|
| `id` | `string` | Its search id. |
| `title` | `string` | The row's title. |
| `description` | `string?` | A line under the title. |
| `keywords` | `string?` | More words settings search matches. |
| `render` | `(view: View) -> UiNode?` | Draws the row's control. |

### SettingEntry

A settings search entry, indexed without drawing the page.

| Field | Type | Description |
|---|---|---|
| `id` | `string` | The `ui.settingsRow` id search reveals. |
| `title` | `string` | The title in search results. |
| `description` | `string?` | Its second line. |
| `keywords` | `string?` | More words search matches. |

### SettingsPageProps

What a settings page's render receives.

| Field | Type | Description |
|---|---|---|
| `target` | `string?` | The entry id settings search is revealing. |

### SettingsPageSpec

A settings page, or a section on a host page. Desktop's plugin API calls it `SettingsPage`.

| Field | Type | Description |
|---|---|---|
| `id` | `string` | The page's id, also for `Layout:OpenSettings`. |
| `title` | `string` | Its navigation label. |
| `description` | `string?` | The line under the page's title. A section on a host page has none. |
| `icon` | `IconName?` | Its navigation icon. |
| `page` | `HostSettingsPage?` | Adds a section to this host page instead of a page of its own. |
| `about` | `boolean?` | Lists the page in the About group, such as release notes. |
| `entries` | `{ SettingEntry }?` | Search entries. |
| `render` | `(view: View, props: SettingsPageProps) -> UiNode?` | Draws the page or section. |
| `renderActions` | `((view: View) -> UiNode?)?` | Draws controls at the end of the page's header, such as a link out. |

### MenuItemSpec

An item of a host menu. Desktop's plugin API calls it `MenuItem`.

| Field | Type | Description |
|---|---|---|
| `menu` | `MenuName` | The menu. |
| `id` | `string` | Its id within the plugin. |
| `title` | `string` | Its label. |
| `order` | `number?` | Ascending order. |
| `icon` | `IconName?` | `session.panel`: its icon. |
| `keybind` | `string?` | `session.panel`: a published command id whose shortcut it shows. |
| `description` | `string?` | `session.panel`: muted detail in launcher lists. |
| `primary` | `boolean?` | `session.panel`: + runs this item directly. |
| `when` | `((target: string) -> boolean)?` | `server.row`: lists it only for rows where it returns true. `session.panel`: lists it only where it returns true for the session id. |
| `opened` | `((sessionId: string) -> boolean)?` | `session.panel`: what it opens is already in the session's strip, so the New tab view leaves it out. |
| `detail` | `((sessionId: string) -> string?)?` | `session.panel`: muted detail for the session, instead of `description`. |
| `enabled` | `((target: string) -> boolean)?` | `server.row`: shows it disabled while it returns false. |
| `hover` | `((sessionId: string) -> ())?` | `session.panel`: runs once when the pointer comes onto its row in the + menu or the New tab view, before any press: the moment to get ready for it, such as `Browser:Prepare`. |
| `run` | `(target: string?) -> ()` | Runs it. `server.row` passes the row's server key. |

### MenubarItemSpec

An item of the app menu bar. Desktop's plugin API calls it `MenubarItem`.

| Field | Type | Description |
|---|---|---|
| `menu` | `MenubarMenu` | The submenu. |
| `id` | `string` | Its id within the plugin. |
| `label` | `string` | Its label. |
| `order` | `number?` | Ascending order. |
| `bind` | `string?` | Its accelerator, such as `"mod+,"`. |
| `enabled` | `boolean?` | False shows it disabled. |
| `run` | `() -> ()` | Runs it. |

### LinkHandlerSpec

Opens local links it matches. The matching handler with the highest priority wins. Desktop's plugin API calls it `LinkHandler`.

| Field | Type | Description |
|---|---|---|
| `id` | `string` | Its id within the plugin. |
| `priority` | `number?` | Higher wins. Defaults to 0. |
| `match` | `(link: Link) -> boolean` | Whether it opens the link. It must not yield. |
| `open` | `(link: Link) -> ()` | Opens a link `match` accepted. |
| `exists` | `((link: Link) -> boolean)?` | Whether the target exists, so text renders as a link. It must not read file contents. |

### TitlebarItemView

What a titlebar item shows.

| Field | Type | Description |
|---|---|---|
| `label` | `string` | The visible label. |
| `title` | `string?` | The accessible name, when it differs from the label. |
| `icon` | `IconName?` | Its icon. Defaults to a download arrow. |
| `busy` | `boolean?` | Shows a spinner and disables it. |
| `pressed` | `boolean?` | Shows it pressed. |

### TitlebarItemSpec

A pill at the titlebar's end. Desktop's plugin API calls it `TitlebarItem`.

| Field | Type | Description |
|---|---|---|
| `id` | `string` | Its id within the plugin. |
| `show` | `() -> TitlebarItemView?` | What it shows, or nil to hide it. Called again after `Contribution:Refresh` and after any signal the plugin fires. |
| `run` | `(() -> ())?` | Runs when the user clicks it. |

### ComposerPillProps

What a composer pill receives.

| Field | Type | Description |
|---|---|---|
| `sessionId` | `string` | The session whose composer it sits above. |
| `directory` | `string` | Its workspace folder. |

### ComposerPillRow

One row of the menu a composer pill opens.

| Field | Type | Description |
|---|---|---|
| `id` | `string` | Its id within the pill. |
| `label` | `string` | Its text. |
| `icon` | `IconName?` | Its icon. |
| `tone` | `Tone?` | Colors its icon. |
| `detail` | `string?` | Muted text at its end. |
| `run` | `(() -> ())?` | Runs when the user picks it. |

### ComposerPillView

What a composer pill shows above one session's composer.

| Field | Type | Description |
|---|---|---|
| `label` | `string` | The capsule's text. |
| `icon` | `IconName?` | The capsule's icon. |
| `tone` | `Tone?` | Colors the icon. |
| `title` | `string?` | The accessible name, when it differs from `label`. It names the popover `renderMenu` fills, which has no heading of the host's. |
| `rows` | `{ ComposerPillRow }?` | The menu it opens, when the pill has no `renderMenu`. |

### ComposerPillSpec

A pill in the row above a session's composer. Desktop's plugin API calls it `ComposerPill`.

| Field | Type | Description |
|---|---|---|
| `id` | `string` | Its id within the plugin. |
| `order` | `number?` | Ascending order among plugin pills. |
| `show` | `(props: ComposerPillProps) -> ComposerPillView?` | What it shows above this session's composer, or nil for nothing. It must not yield. |
| `renderMenu` | `((view: View, props: ComposerPillProps) -> UiNode?)?` | Content for the popover the pill opens. The host owns the capsule and the popover's frame (its width, a 4 pt inset, placement, focus and dismissal); everything inside, any heading included, is yours. |
| `run` | `((props: ComposerPillProps) -> ())?` | Runs on a click when the pill has no menu. |

### CodeBlockProps

One complete fenced code block.

| Field | Type | Description |
|---|---|---|
| `code` | `string` | The source between the fences. |
| `language` | `string` | The fence's language, lowercased. Empty when it has none. |
| `sessionId` | `string?` | The session that shows it. |
| `streaming` | `boolean?` | The reply it is in still streams: the block may grow, and may not be closed yet. |

### CodeBlockSpec

Renders the fenced code blocks of some languages as your own UI. Desktop's plugin API calls it `CodeBlock`.

| Field | Type | Description |
|---|---|---|
| `id` | `string` | Its id within the plugin. |
| `languages` | `{ string }` | The languages it renders. The host resolves no aliases. |
| `render` | `(view: View, props: CodeBlockProps) -> UiNode?` | Draws a block. Return nil to keep the highlighted code. |

### CodeBlockActionSpec

A button beside a code block's Copy button. Desktop's plugin API calls it `CodeBlockAction`.

| Field | Type | Description |
|---|---|---|
| `id` | `string` | Its id within the plugin. |
| `languages` | `{ string }` | The languages whose blocks show it. |
| `icon` | `IconName` | Its icon. |
| `title` | `string` | Its accessible name and tooltip. |
| `order` | `number?` | Ascending order, before Copy. |
| `run` | `(props: CodeBlockProps) -> ()` | Runs on a click. |

### SlotProps

What a slot's render receives.

| Field | Type | Description |
|---|---|---|
| `sessionId` | `string?` | Session slots: the session. |
| `directory` | `string?` | Session slots: its workspace folder. |
| `active` | `boolean?` | `session.header`: the timeline is on screen. |
| `compact` | `boolean?` | `session.header`: the title needs the room, so draw the compact form. |
| `shown` | `boolean?` | `session.location.menu`: the menu is open. |
| `pane` | `string?` | New tab slots: the pane (`side`, `split`, or `bottom`). |
| `width` | `number?` | New tab slots: the pane's available width in points. |
| `focusToken` | `number?` | New tab slots: incremented when the user explicitly asked for this New tab, so its first field takes focus once. |

### SlotSpec

Content for one of the shell's slots. Desktop's plugin API calls it `Slot`.

| Field | Type | Description |
|---|---|---|
| `id` | `string` | Its id within the plugin. |
| `at` | `SlotName` | The slot. |
| `order` | `number?` | Ascending order. |
| `render` | `(view: View, props: SlotProps) -> UiNode?` | Draws its content, or nil for nothing. |

### ThemeSeeds

The seed colors a theme variant generates its scales from, as `#RRGGBB` strings.

| Field | Type | Description |
|---|---|---|
| `neutral` | `string` | The gray scale's base. |
| `primary` | `string` | The brand and accent color. |
| `success` | `string` | Success states. |
| `warning` | `string` | Warning states. |
| `error` | `string` | Error states. |
| `info` | `string` | Informational states. |
| `interactive` | `string` | Links and interactive elements. |
| `diffAdd` | `string` | Added lines. |
| `diffDelete` | `string` | Deleted lines. |

### ThemeVariant

One color scheme of a theme.

| Field | Type | Description |
|---|---|---|
| `seeds` | `ThemeSeeds` | The seed colors. |
| `overrides` | `{ [string]: string }?` | Exact values for single tokens of the app's theme file format. |

### ThemeSpec

A color theme, listed in Settings › Appearance as `<plugin>:<id>`. Desktop's plugin API calls it `Theme`.

| Field | Type | Description |
|---|---|---|
| `id` | `string` | Its id within the plugin. |
| `name` | `string` | The name the theme list shows. |
| `light` | `ThemeVariant` | The light scheme. |
| `dark` | `ThemeVariant` | The dark scheme. |

### FileViewerProps

What a file viewer's render receives.

| Field | Type | Description |
|---|---|---|
| `path` | `string` | The file. |
| `sessionId` | `string?` | The session that opened it. |
| `directory` | `string` | Its workspace folder. |

### FileViewerSpec

Shows files with some extensions in the file panel. Desktop's plugin API calls it `FileViewer`.

| Field | Type | Description |
|---|---|---|
| `id` | `string` | Its id within the plugin. |
| `title` | `string` | Its name, such as `"Word document"`. |
| `extensions` | `{ string }` | The extensions it shows, without the dot. |
| `render` | `(view: View, props: FileViewerProps) -> UiNode?` | Draws the file. |

### ServerRowProps

What a server source's row render receives.

| Field | Type | Description |
|---|---|---|
| `id` | `string` | The entry's id. |
| `key` | `string` | Its key: `<plugin>:<id>`. |
| `health` | `ServerHealth?` | Its latest health check. |

### ServerCoverProps

What a server source's cover render receives.

| Field | Type | Description |
|---|---|---|
| `id` | `string` | The entry's id. |
| `key` | `string` | Its key: `<plugin>:<id>`. |
| `tab` | `string` | The tab it covers: `<server>:<sessionId>` for a session, the draft's id for a draft. |
| `visit` | `number` | The window's page visit: it changes whenever the window shows another page, so a cover can offer something once per visit, as Desktop's SSH cover offers its sign-in. |
| `dialog` | `boolean` | A dialog shows over the window. A cover offers nothing that opens a dialog while one shows. |

### ServerSourceSpec

A source of servers, such as SSH hosts or WSL distributions. Add it in the app half to list entries, and in each window half to draw them: the host runs a window half's `row`, `cover`, `connect`, `reconnect` and `remove`, else the app half's. Desktop's plugin API calls it `Server`.

| Field | Type | Description |
|---|---|---|
| `id` | `string` | Its id within the plugin. |
| `title` | `string` | Its name in Settings › Servers. |
| `order` | `number?` | Ascending order among sources. |
| `row` | `((view: View, props: ServerRowProps) -> UiNode?)?` | Draws an entry's connection row in Settings › Servers. Window halves only. |
| `cover` | `((view: View, props: ServerCoverProps) -> UiNode?)?` | Draws over a session or draft tab of an entry that is not `ready`, such as a disconnected SSH host's Reconnect panel. The tab stays mounted under it. Window halves only. |
| `connect` | `((id: string) -> boolean)?` | Starts or signs in to an entry that is not ready, as a coroutine. Home's Authenticate and a signed-out tab's Sign in run it. Return true once it is ready. The host gives up after 120 seconds. |
| `reconnect` | `((id: string) -> ())?` | Runs when the event stream of a `managed` entry drops, so the plugin can reach it again, for example through a new tunnel. |
| `remove` | `((id: string) -> ())?` | Runs before the host forgets an entry that Settings › Servers removes. Raise an error to keep the entry. |

### ServerSource

A server source a plugin added.

| Member | Type | Description |
|---|---|---|
| `:SetEntries` | `(entries: { ServerEntry }, ready: boolean) -> ()` | Replaces its entries. Once `ready`, the list is complete and the host forgets servers it no longer lists. |
| `:Withdraw` | `() -> ()` | Removes the source. |

### DialogProps

What a dialog's render receives.

| Field | Type | Description |
|---|---|---|
| `data` | `{ [string]: any }?` | What `Dialog:Open` passed. |

### DialogSpec

A modal dialog, opened with `Dialog:Open`.

| Field | Type | Description |
|---|---|---|
| `id` | `string` | Its id within the plugin. |
| `title` | `string?` | Its heading and accessible name. |
| `size` | `DialogSize?` | Its size preset. |
| `layer` | `DialogLayer?` | Which modal overlay layer the dialog draws in (`"standard"` beneath elevated recovery/server dialogs, or `"elevated"` above them). Defaults to `"elevated"`. Since 1.5. |
| `radius` | `number?` | Corner radius in points. Defaults to 6 for `normal` dialogs and 12 for `tall`, `large` and `x-large`. Since 1.5. |
| `hideScrollbar` | `boolean?` | Hides the scroll bar when a `normal` dialog's content overflows, as Desktop's `dialog-content` does. Defaults to false. Since 1.5. |
| `render` | `(view: View, props: DialogProps) -> UiNode?` | Draws its body inside the host's dialog frame: Desktop's dialog with the overlay elevation. A normal one is as tall as its content on layer-01 with 6-point corners; a tall one is as tall as its content up to its size's height on bg-base with 12-point corners, and what in it may shrink (`shrink = true`, `minH = 0`) gives way once it is that tall; a large or x-large one is its size's height on bg-base with 12-point corners, so its body can fill it (`h = "full"`). |
| `onDismiss` | `(() -> ())?` | Runs when Esc or a click outside closes it. |

### PopoverProps

What a popover's render receives.

| Field | Type | Description |
|---|---|---|
| `data` | `{ [string]: any }?` | What `Popover:Open` or `ui.flyoutRow` passed. |

### PopoverSpec

Popover content, opened with `Popover:Open` or `ui.flyoutRow`. The host owns placement, focus and dismissal.

| Field | Type | Description |
|---|---|---|
| `id` | `string` | Its id within the plugin. |
| `title` | `string?` | Its heading and accessible name. |
| `width` | `number?` | Its width in points. |
| `render` | `(view: View, props: PopoverProps) -> UiNode?` | Draws its content. |

### McpToolSpec

A tool on the app's client MCP server, named `<plugin>_<name>`. Desktop's plugin API calls it `McpTool`.

| Field | Type | Description |
|---|---|---|
| `name` | `string` | Its name within the plugin. |
| `description` | `string` | What it does, for the model. |
| `inputSchema` | `{ [string]: any }` | Its arguments' JSON Schema. |
| `call` | `(args: { [string]: any }, context: McpCallContext) -> any` | Runs it, as a task, and returns its result: a string, a table sent as JSON, or an `McpImage`. Raise an error to fail. It may yield (call `...Async` methods): the call answers when it returns, or fails after 30 seconds. |

### McpImage

An MCP tool result with an image: a text part and the image.

| Field | Type | Description |
|---|---|---|
| `text` | `string` | The text part, such as JSON metadata. |
| `image` | `string` | The image, base64-encoded. |
| `mimeType` | `string` | Its type, such as `image/png`. |

### McpCallContext

Where an MCP tool call comes from.

| Field | Type | Description |
|---|---|---|
| `sessionId` | `string` | The session whose agent called it. |
| `directory` | `string` | That session's workspace folder. |

## plugin.toml

plugin.toml: a plugin folder's manifest. Editors validate it with `plugins/plugin.schema.json`: add `#:schema ../plugin.schema.json` as its first line.

| Key | Type | Required | Description |
|---|---|---|---|
| `id` | `string` | yes | The plugin's id: lowercase letters, digits and hyphens. It prefixes commands (`<id>.<command>`) and contracts. |
| `name` | `string` | yes | The display name. |
| `version` | `string` | yes | The plugin's own version, as semver. |
| `contract` | `string` | yes | The contract version the plugin targets, such as `"1.0"`. |
| `description` | `string` | no | One sentence about what the plugin adds. |
| `entry` | `string` | no | The window half's module, relative to the plugin folder. Defaults to `"init.luau"`. |
| `app` | `string` | no | The app half's module, such as `"app.luau"`. Omit it for a plugin with no app half. |
| `os` | array of [`Platform`](#platform) | no | The platforms it runs on. Omit it for all. |
| `channels` | array of [`Channel`](#channel) | no | The app channels it loads in. Omit it for all. |
| `authors` | `string[]` | no | Its authors. |
| `homepage` | `string` | no | Its homepage. |
| `license` | `string` | no | An SPDX license expression. |
| `stores` | `table[]` | no | The stores it uses, by name and scope. Every store of a plugin is in memory before its setup runs, declared or not; the list documents them. |
| `stores.name` | `string` | yes | The name passed to `Store:Value` or `Store:SessionValue`. |
| `stores.scope` | [`StoreScope`](#storescope) | no | One value for the app, or one per session. Defaults to `"global"`. |
| `settings_search` | `table[]` | no | Settings search entries found without loading the plugin. |
| `settings_search.id` | `string` | yes | The `ui.settingsRow` id search reveals. |
| `settings_search.page` | `string` | yes | The page: `plugins`, a host page, or one of this plugin's settings pages. |
| `settings_search.title` | `string` | yes | The title in search results. |
| `settings_search.description` | `string` | no | Its second line. |
| `settings_search.keywords` | `string` | no | More words search matches. |
