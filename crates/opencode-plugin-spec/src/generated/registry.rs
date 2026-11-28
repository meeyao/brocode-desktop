//! Contribution kinds, services and context events, with the halves that offer them.
//! Generated from crates/opencode-plugin-spec/contract.json by `cargo run -p opencode-plugin-spec -- --write`. Do not edit.

use crate::{Half, string_enum};

/// The contract version this crate describes.
pub const CONTRACT: &str = "1.13";
pub const CONTRACT_MAJOR: u16 = 1;
pub const CONTRACT_MINOR: u16 = 13;

string_enum! {
    /// A contribution kind, by its contract name.
    pub enum ContributionKind {
        /// Adds a command to the palette, its keybind and its slash command.
        Command = "command",
        /// Adds a panel: tabs in the session's side region.
        Panel = "panel",
        /// Adds a row to Settings › Plugins.
        Setting = "setting",
        /// Adds a settings page, or a section on a host page.
        SettingsPage = "settingsPage",
        /// Adds an item to a host menu.
        MenuItem = "menuItem",
        /// Adds an item to the app menu bar.
        MenubarItem = "menubarItem",
        /// Opens local links it matches.
        LinkHandler = "linkHandler",
        /// Adds a pill at the titlebar's end.
        TitlebarItem = "titlebarItem",
        /// Adds a pill above the session composer.
        ComposerPill = "composerPill",
        /// Renders fenced code blocks of some languages.
        CodeBlock = "codeBlock",
        /// Adds a button beside code blocks' Copy button.
        CodeBlockAction = "codeBlockAction",
        /// Fills one of the shell's slots.
        Slot = "slot",
        /// Adds a color theme.
        Theme = "theme",
        /// Shows files with some extensions.
        FileViewer = "fileViewer",
        /// Adds a source of servers. Add it in the app half so every window shares one list.
        ServerSource = "serverSource",
        /// Adds a modal dialog, opened with `Dialog:Open`.
        Dialog = "dialog",
        /// Adds popover content, opened with `Popover:Open` or `ui.flyoutRow`.
        Popover = "popover",
        /// Adds a tool to the app's client MCP server.
        McpTool = "mcpTool",
    }
}

impl ContributionKind {
    /// The context method that adds it, such as `AddCommand`.
    pub fn method(self) -> &'static str {
        match self {
            Self::Command => "AddCommand",
            Self::Panel => "AddPanel",
            Self::Setting => "AddSetting",
            Self::SettingsPage => "AddSettingsPage",
            Self::MenuItem => "AddMenuItem",
            Self::MenubarItem => "AddMenubarItem",
            Self::LinkHandler => "AddLinkHandler",
            Self::TitlebarItem => "AddTitlebarItem",
            Self::ComposerPill => "AddComposerPill",
            Self::CodeBlock => "AddCodeBlock",
            Self::CodeBlockAction => "AddCodeBlockAction",
            Self::Slot => "AddSlot",
            Self::Theme => "AddTheme",
            Self::FileViewer => "AddFileViewer",
            Self::ServerSource => "AddServerSource",
            Self::Dialog => "AddDialog",
            Self::Popover => "AddPopover",
            Self::McpTool => "AddMcpTool",
        }
    }

    /// The spec record it takes.
    pub fn spec(self) -> &'static str {
        match self {
            Self::Command => "CommandSpec",
            Self::Panel => "PanelSpec",
            Self::Setting => "SettingSpec",
            Self::SettingsPage => "SettingsPageSpec",
            Self::MenuItem => "MenuItemSpec",
            Self::MenubarItem => "MenubarItemSpec",
            Self::LinkHandler => "LinkHandlerSpec",
            Self::TitlebarItem => "TitlebarItemSpec",
            Self::ComposerPill => "ComposerPillSpec",
            Self::CodeBlock => "CodeBlockSpec",
            Self::CodeBlockAction => "CodeBlockActionSpec",
            Self::Slot => "SlotSpec",
            Self::Theme => "ThemeSpec",
            Self::FileViewer => "FileViewerSpec",
            Self::ServerSource => "ServerSourceSpec",
            Self::Dialog => "DialogSpec",
            Self::Popover => "PopoverSpec",
            Self::McpTool => "McpToolSpec",
        }
    }

    /// The halves that may add it.
    pub fn halves(self) -> &'static [Half] {
        match self {
            Self::Command => &[Half::Window],
            Self::Panel => &[Half::Window],
            Self::Setting => &[Half::Window],
            Self::SettingsPage => &[Half::Window],
            Self::MenuItem => &[Half::Window],
            Self::MenubarItem => &[Half::App],
            Self::LinkHandler => &[Half::Window],
            Self::TitlebarItem => &[Half::Window],
            Self::ComposerPill => &[Half::Window],
            Self::CodeBlock => &[Half::Window],
            Self::CodeBlockAction => &[Half::Window],
            Self::Slot => &[Half::Window],
            Self::Theme => &[Half::Window],
            Self::FileViewer => &[Half::Window],
            Self::ServerSource => &[Half::Window, Half::App],
            Self::Dialog => &[Half::Window],
            Self::Popover => &[Half::Window],
            Self::McpTool => &[Half::Window, Half::App],
        }
    }
}

string_enum! {
    /// A host service, by the name `ctx:GetService` takes.
    pub enum ServiceName {
        /// The sessions the app holds.
        Sessions = "Sessions",
        /// The server's model catalog.
        Models = "Models",
        /// The OpenCode server this window works with: the server of the session or draft it shows, so a panel in a tab from another server talks to that server. A path that starts with `local:` goes to the app's own server. In the app half, the app's own server.
        Server = "Server",
        /// The servers the app lists.
        Servers = "Servers",
        /// HTTP requests off the UI thread.
        Http = "Http",
        /// Child processes the host supervises.
        Process = "Process",
        /// Values the host saves for this plugin in the app's database. Both halves and every window share one copy. Every value is in memory before the plugin's setup runs, so a read never waits and has the saved value. A `Set` changes the value at once; it reaches the disk within 250 ms, and always when a window hides or closes and when the app quits. Names starting `rune.` are the host's own.
        Store = "Store",
        /// Typed values plugins share. A plugin may provide only ids under its own id.
        Contracts = "Contracts",
        /// The Studio console. `print` and `warn` write here too.
        Console = "Console",
        /// Short messages in a corner of the window. From the app half, in the focused window.
        Toast = "Toast",
        /// This plugin's popovers. The host places them, moves focus and closes them.
        Popover = "Popover",
        /// This plugin's modal dialogs.
        Dialog = "Dialog",
        /// The app's own menus.
        Menu = "Menu",
        /// Panels, settings and drafts in this window.
        Layout = "Layout",
        /// Routes local links to the plugins that handle them.
        Links = "Links",
        /// Published commands, as the palette runs them.
        Commands = "Commands",
        /// The theme and the user's appearance settings.
        Theme = "Theme",
        /// The running app.
        App = "App",
        /// The clipboard, the system browser and file manager, and the clock.
        System = "System",
        /// System notifications that follow the user's notification settings.
        Notifications = "Notifications",
        /// Files in the workspace and the apps that open them.
        Workspace = "Workspace",
        /// Session changes for `ui.diffView`.
        Diff = "Diff",
        /// Terminal sessions on the server, for `ui.terminalSurface`.
        Terminal = "Terminal",
        /// Pages in this window's embedded browser, each a tab of a session's side region.
        Browser = "Browser",
        /// The active session's or draft's composer.
        Composer = "Composer",
        /// Office documents laid out and drawn by the host, each on a thread of its own.
        Office = "Office",
        /// The app's own updates (Desktop's `Updater` IPC). The host checks, downloads and verifies on its own schedule; a plugin draws where it stands and asks for a check or the install.
        Updater = "Updater",
        /// Developer diagnostics. Local and beta builds only, except `Get` of an experiment and `HeapSnapshotAsync`; elsewhere the rest return nil.
        Diagnostics = "Diagnostics",
        /// Display sleep.
        Power = "Power",
        /// QR codes.
        Qr = "Qr",
        /// Credential prompts from child processes such as `ssh`.
        Askpass = "Askpass",
        /// Images for `ui.image`: loaded from a source or handed over, decoded off the UI thread by the window's image cache (PNG and APNG, JPEG, GIF, WebP, BMP, ICO, AVIF, ICNS and SVG).
        Images = "Images",
        /// The plugin's own messages in the interface language, and numbers, dates and lists written the way the app writes them. A plugin's catalogs are `locales/<id>.json` in its folder: flat objects of message keys, the app's format.
        I18n = "I18n",
    }
}

impl ServiceName {
    /// The halves that may get it.
    pub fn halves(self) -> &'static [Half] {
        match self {
            Self::Sessions => &[Half::Window, Half::App],
            Self::Models => &[Half::Window, Half::App],
            Self::Server => &[Half::Window, Half::App],
            Self::Servers => &[Half::Window, Half::App],
            Self::Http => &[Half::Window, Half::App],
            Self::Process => &[Half::Window, Half::App],
            Self::Store => &[Half::Window, Half::App],
            Self::Contracts => &[Half::Window, Half::App],
            Self::Console => &[Half::Window, Half::App],
            Self::Toast => &[Half::Window, Half::App],
            Self::Popover => &[Half::Window],
            Self::Dialog => &[Half::Window],
            Self::Menu => &[Half::Window],
            Self::Layout => &[Half::Window],
            Self::Links => &[Half::Window],
            Self::Commands => &[Half::Window],
            Self::Theme => &[Half::Window, Half::App],
            Self::App => &[Half::Window, Half::App],
            Self::System => &[Half::Window, Half::App],
            Self::Notifications => &[Half::Window, Half::App],
            Self::Workspace => &[Half::Window, Half::App],
            Self::Diff => &[Half::Window],
            Self::Terminal => &[Half::Window, Half::App],
            Self::Browser => &[Half::Window],
            Self::Composer => &[Half::Window],
            Self::Office => &[Half::Window],
            Self::Updater => &[Half::Window, Half::App],
            Self::Diagnostics => &[Half::Window, Half::App],
            Self::Power => &[Half::Window, Half::App],
            Self::Qr => &[Half::Window, Half::App],
            Self::Askpass => &[Half::App],
            Self::Images => &[Half::Window],
            Self::I18n => &[Half::Window, Half::App],
        }
    }

    /// Its methods, signals and fields, by name.
    pub fn members(self) -> &'static [&'static str] {
        match self {
            Self::Sessions => &["Get", "Routed", "Open", "Changed", "RoutedChanged"],
            Self::Models => &["Get", "List", "SetBadges", "Changed"],
            Self::Server => &["RequestAsync", "Resource", "Invalidate", "OnEvent", "EncodeUri", "Version", "IsLocal", "IsLive", "CanWrite", "Connected"],
            Self::Servers => &["List", "Current", "Health", "Connect", "Changed"],
            Self::Http => &["FetchAsync"],
            Self::Process => &["Spawn", "RunAsync"],
            Self::Store => &["Value", "SessionValue"],
            Self::Contracts => &["Provide", "Use", "Summary"],
            Self::Console => &["Log", "Warn", "Error", "Lines", "Clear", "Eval"],
            Self::Toast => &["Show", "Info", "Success", "Error"],
            Self::Popover => &["Open", "Toggle", "Close", "IsOpen"],
            Self::Dialog => &["Open", "Close", "IsOpen"],
            Self::Menu => &["OpenAsync"],
            Self::Layout => &["OpenPanel", "TogglePanel", "ClosePanel", "PanelState", "OpenSettings", "NewDraft", "OpenDraft", "OpenProject", "Narrow", "PanelSidebar", "TogglePanelSidebar", "MovePanelSidebar", "OpenDrawerView", "RegionOpen", "RegionFocused", "Focus"],
            Self::Links => &["Open", "Exists"],
            Self::Commands => &["Run", "Keybind"],
            Self::Theme => &["Current", "IsDark", "Resolve", "Font", "Changed"],
            Self::App => &["Version", "Channel", "Platform", "Commit", "BetaNotes", "Locale", "Arg", "LogsDir", "DownloadsDir", "LocaleChanged"],
            Self::System => &["CopyText", "OpenExternal", "Reveal", "Now", "UtcOffset", "FreePort", "RandomId"],
            Self::Notifications => &["Notify"],
            Self::Workspace => &["ReadFileAsync", "SaveDownloadAsync", "OpenDocument", "OpenTree", "OpenInApps", "OpenIn", "Relative", "OpenFile", "FileChanged"],
            Self::Diff => &["Open", "Mode", "SetMode"],
            Self::Terminal => &["Create", "Attach", "List", "Available", "New", "Toggle", "Select", "OpenAsync"],
            Self::Browser => &["OpenAsync", "Tabs", "Get", "List", "Active", "Selected", "Focused", "Available", "Engine", "NormalizeUrl", "Prepare", "Changed", "Picked"],
            Self::Composer => &["InsertPill", "Active", "Directory", "Model", "Picker", "PickerChanged"],
            Self::Office => &["OpenAsync", "Version", "SetSheet", "Sheet", "Close"],
            Self::Updater => &["State", "CheckAsync", "InstallAsync", "StateChanged"],
            Self::Diagnostics => &["Memory", "Metrics", "Plugins", "Get", "Set", "HeapSnapshotAsync"],
            Self::Power => &["KeepAwake"],
            Self::Qr => &["Encode"],
            Self::Askpass => &["Listen"],
            Self::Images => &["Load", "State", "VideoFailed", "FromEncoded", "FromSvg", "FromRgba", "Release"],
            Self::I18n => &["T", "Has", "Locale", "Direction", "Plural", "Number", "Date", "RelativeTime", "RelativeDay", "List", "Changed"],
        }
    }
}

string_enum! {
    /// A signal on a plugin context.
    pub enum ContextEvent {
        /// Fires as the half goes away: the plugin turns off, reloads, or its window closes. The host disconnects every connection afterwards.
        Disposing = "Disposing",
        /// Fires as the window gains or loses focus.
        FocusChanged = "FocusChanged",
    }
}

impl ContextEvent {
    /// The halves whose context has it.
    pub fn halves(self) -> &'static [Half] {
        match self {
            Self::Disposing => &[Half::Window, Half::App],
            Self::FocusChanged => &[Half::Window],
        }
    }
}
