//! The contract's string enums.
//! Generated from crates/opencode-plugin-spec/contract.json by `cargo run -p opencode-plugin-spec -- --write`. Do not edit.

use crate::string_enum;

string_enum! {
    /// Which half of a plugin runs the code.
    pub enum Half {
        /// One instance per window: the plugin's UI.
        Window = "window",
        /// One instance per process: workers, processes and shared state.
        App = "app",
    }
}

string_enum! {
    /// The operating system the host runs on.
    pub enum Platform {
        /// Windows.
        Windows = "windows",
        /// macOS.
        Macos = "macos",
        /// Linux.
        Linux = "linux",
        /// A browser host.
        Web = "web",
    }
}

string_enum! {
    /// The app build's release channel.
    pub enum Channel {
        /// A development build.
        Local = "local",
        /// A dev release build.
        Dev = "dev",
        /// A beta build.
        Beta = "beta",
        /// A production build.
        Prod = "prod",
    }
}

string_enum! {
    /// A light or dark color scheme.
    pub enum ColorScheme {
        /// Light.
        Light = "light",
        /// Dark.
        Dark = "dark",
    }
}

string_enum! {
    /// A status color for an icon or a pill.
    pub enum Tone {
        /// Done or passing.
        Success = "success",
        /// Needs attention.
        Warning = "warning",
        /// Failed or blocked.
        Danger = "danger",
        /// Under way, in the accent color.
        Accent = "accent",
    }
}

string_enum! {
    /// A toast's tone.
    pub enum ToastVariant {
        /// Neutral information.
        Default = "default",
        /// Something finished. Shows a check icon unless the toast sets `icon`.
        Success = "success",
        /// Something failed.
        Error = "error",
        /// Something is in progress. Use it with `persistent` and dismiss the toast when the work ends.
        Loading = "loading",
    }
}

string_enum! {
    /// The section of Settings › Shortcuts that lists a command.
    pub enum CommandSection {
        /// App-wide commands.
        General = "general",
        /// Commands on the current session.
        Session = "session",
        /// Moving between views and tabs.
        Navigation = "navigation",
        /// Model and agent choice.
        Model = "model",
        /// The terminal.
        Terminal = "terminal",
        /// The composer.
        Prompt = "prompt",
    }
}

string_enum! {
    /// Where a panel's narrow-screen view is offered.
    pub enum PanelMobileKind {
        /// A tab of the narrow view switcher, beside Session. It replaces the conversation.
        Tab = "tab",
        /// A row of the switcher's More drawer. It replaces the conversation.
        Menu = "menu",
        /// A row of the More drawer that opens inside the drawer, over the conversation.
        Drawer = "drawer",
    }
}

string_enum! {
    /// A host menu a menu item joins.
    pub enum MenuName {
        /// The + menu before the side panel's tabs, and the New tab view.
        SessionPanel = "session.panel",
        /// The Add server menu in Settings › Servers.
        ServerAdd = "server.add",
        /// The menu of each server row in Settings › Servers. `run` receives the row's server key.
        ServerRow = "server.row",
    }
}

string_enum! {
    /// A submenu of the app menu bar.
    pub enum MenubarMenu {
        /// The app menu.
        App = "app",
        /// File.
        File = "file",
        /// Edit.
        Edit = "edit",
        /// View.
        View = "view",
        /// Go.
        Go = "go",
        /// Window.
        Window = "window",
        /// Help.
        Help = "help",
    }
}

string_enum! {
    /// A fixed place in the app's shell that a slot fills.
    pub enum SlotName {
        /// A full-width strip under the window content, above toasts.
        WindowBottom = "window.bottom",
        /// The end of the session header, before the usage numbers.
        SessionHeader = "session.header",
        /// Rows in the session's project and location menu.
        SessionLocationMenu = "session.location.menu",
        /// The actions after the side panel's tabs.
        SessionPanelEnd = "session.panel.end",
        /// The top of the side panel's New tab view, above its launchers.
        SessionPanelNew = "session.panel.new",
        /// Below the launchers of the side panel's New tab view, before Recently closed.
        SessionPanelNewList = "session.panel.new.list",
    }
}

string_enum! {
    /// A host settings page a settings page can add a section to.
    pub enum HostSettingsPage {
        /// Settings › General.
        General = "general",
        /// Settings › Servers.
        Servers = "servers",
    }
}

string_enum! {
    /// Where a contributed server stands.
    pub enum ServerState {
        /// Not running.
        Stopped = "stopped",
        /// Starting up.
        Starting = "starting",
        /// Waiting for the user to sign in.
        Auth = "auth",
        /// Reachable. `http` holds its endpoint.
        Ready = "ready",
        /// Could not start.
        Failed = "failed",
        /// Runs a version this app does not support.
        Incompatible = "incompatible",
    }
}

string_enum! {
    /// Where a plugin's panel tab is in a session.
    pub enum PanelState {
        /// Not in the session's tab strip.
        Closed = "closed",
        /// In the strip, not selected.
        Listed = "listed",
        /// Selected in its pane, with the side region hidden.
        Selected = "selected",
        /// Selected, with the side region on screen.
        Visible = "visible",
    }
}

string_enum! {
    /// How a layout change moves.
    pub enum Transition {
        /// The app's own motion.
        Default = "default",
        /// Lands at once, without motion.
        None = "none",
    }
}

string_enum! {
    /// How hover and press fills change.
    pub enum HoverTransition {
        /// The app's own fade.
        Default = "default",
        /// Lands at once.
        Instant = "instant",
    }
}

string_enum! {
    /// A side of an anchor.
    pub enum Side {
        /// Above.
        Top = "top",
        /// Below.
        Bottom = "bottom",
        /// Before, in left-to-right text.
        Left = "left",
        /// After, in left-to-right text.
        Right = "right",
    }
}

string_enum! {
    /// Alignment along an edge or an axis.
    pub enum Align {
        /// The start edge.
        Start = "start",
        /// Centered.
        Center = "center",
        /// The end edge.
        End = "end",
    }
}

string_enum! {
    /// How a stack places its children across its axis.
    pub enum UiAlign {
        /// At the start.
        Start = "start",
        /// Centered.
        Center = "center",
        /// At the end.
        End = "end",
        /// Stretched to fill. The default.
        Stretch = "stretch",
    }
}

string_enum! {
    /// How a stack places its children along its axis.
    pub enum UiJustify {
        /// Packed at the start. The default.
        Start = "start",
        /// Packed in the center.
        Center = "center",
        /// Packed at the end.
        End = "end",
        /// The first and last child at the edges, equal space between.
        Between = "between",
    }
}

string_enum! {
    /// How a `ui.frame` container arranges its children.
    pub enum UiDirection {
        /// A vertical stack. The default.
        Col = "col",
        /// A horizontal stack.
        Row = "row",
        /// No axis: children stack on top of each other.
        Box = "box",
    }
}

string_enum! {
    /// The edge of a panel its file sidebar sits on.
    pub enum PanelSidebarSide {
        /// The inline start: left in a left-to-right layout.
        Start = "start",
        /// The inline end: right in a left-to-right layout.
        End = "end",
    }
}

string_enum! {
    /// Which picker overlay the composer currently has open.
    pub enum ComposerPicker {
        /// The agent picker.
        Agent = "agent",
        /// The model picker.
        Model = "model",
        /// The model effort/variant picker.
        Effort = "effort",
        /// The model speed picker, for a model the server offers at faster or slower speeds.
        Speed = "speed",
        /// The draft project picker.
        Project = "project",
    }
}

string_enum! {
    /// A text weight.
    pub enum FontWeight {
        /// 400.
        Regular = "regular",
        /// 500.
        Medium = "medium",
        /// 600.
        Semibold = "semibold",
        /// 700.
        Bold = "bold",
    }
}

string_enum! {
    /// One of the user's font choices.
    pub enum FontFamily {
        /// The interface font.
        Sans = "sans",
        /// The code font.
        Mono = "mono",
    }
}

string_enum! {
    /// A button's look, as the app's own buttons.
    pub enum ButtonVariant {
        /// A filled neutral button. The default.
        Neutral = "neutral",
        /// A high-contrast filled button.
        Contrast = "contrast",
        /// An `accent` filled button.
        Primary = "primary",
        /// No fill until hovered.
        Ghost = "ghost",
        /// A ghost button with a muted label.
        GhostMuted = "ghost-muted",
        /// A bordered button.
        Outline = "outline",
        /// A destructive action.
        Danger = "danger",
    }
}

string_enum! {
    /// A control's size.
    pub enum ControlSize {
        /// Compact.
        Small = "small",
        /// The default.
        Normal = "normal",
        /// Large.
        Large = "large",
    }
}

string_enum! {
    /// A shadow from the app's elevation scale.
    pub enum Elevation {
        /// Slightly above the page: cards.
        Raised = "raised",
        /// Floats over content: popovers and menus.
        Floating = "floating",
        /// Over everything: dialogs.
        Overlay = "overlay",
    }
}

string_enum! {
    /// An element's accessibility role.
    pub enum Role {
        /// No role.
        None = "none",
        /// A group of related elements.
        Group = "group",
        /// A landmark region. Give it a `label`.
        Region = "region",
        /// A button.
        Button = "button",
        /// A link.
        Link = "link",
        /// An on and off switch.
        Switch = "switch",
        /// A checkbox.
        Checkbox = "checkbox",
        /// A radio button.
        Radio = "radio",
        /// A tab.
        Tab = "tab",
        /// A list.
        List = "list",
        /// A list item.
        Listitem = "listitem",
        /// A heading.
        Heading = "heading",
        /// An image. Give it a `label`.
        Img = "img",
        /// A live status message.
        Status = "status",
        /// A countdown or clock, which assistive technology leaves unannounced as it changes.
        Timer = "timer",
        /// An urgent message that assistive technology announces at once, such as a failure.
        Alert = "alert",
    }
}

string_enum! {
    /// The state of a contract another plugin provides.
    pub enum LiveState {
        /// Its provider is still loading or reloading.
        Pending = "pending",
        /// Provided. `Get` returns it.
        Active = "active",
        /// Its provider is off, absent, or does not run on this platform.
        Inactive = "inactive",
        /// Its provider failed to load or stopped.
        Failed = "failed",
    }
}

string_enum! {
    /// The state of a cached server read.
    pub enum ResourceStatus {
        /// Not answered yet. Holds the previous answer while a newer read loads.
        Loading = "loading",
        /// Answered.
        Ready = "ready",
        /// Failed. `error` says why.
        Error = "error",
    }
}

string_enum! {
    /// An HTTP method.
    pub enum HttpMethod {
        /// GET.
        Get = "GET",
        /// POST.
        Post = "POST",
        /// PUT.
        Put = "PUT",
        /// PATCH.
        Patch = "PATCH",
        /// DELETE.
        Delete = "DELETE",
        /// HEAD.
        Head = "HEAD",
    }
}

string_enum! {
    /// A process's output stream.
    pub enum ProcessStream {
        /// Standard output.
        Stdout = "stdout",
        /// Standard error.
        Stderr = "stderr",
    }
}

string_enum! {
    /// How a process encodes its output.
    pub enum TextEncoding {
        /// UTF-8. The default.
        Utf8 = "utf8",
        /// UTF-16 little-endian, such as `wsl.exe` on Windows.
        Utf16le = "utf16le",
    }
}

string_enum! {
    /// How a diff lays out its changes.
    pub enum DiffMode {
        /// Old and new side by side.
        Split = "split",
        /// One column.
        Unified = "unified",
    }
}

string_enum! {
    /// What a diff compares.
    pub enum DiffSource {
        /// The working tree against HEAD.
        Git = "git",
        /// The branch against its merge base.
        Branch = "branch",
        /// One session turn's changes.
        Turn = "turn",
    }
}

string_enum! {
    /// How a diff's read went.
    pub enum DiffStatus {
        /// Not read yet, or a newer read is in flight.
        Loading = "loading",
        /// Read. `Files` holds the changes.
        Ready = "ready",
        /// The folder is not under version control.
        NoVcs = "no-vcs",
        /// The server can't be reached.
        Offline = "offline",
        /// The read failed. `Retry` reads again.
        Error = "error",
    }
}

string_enum! {
    /// How a file or folder changed.
    pub enum ChangeKind {
        /// Added.
        Add = "add",
        /// Deleted.
        Del = "del",
        /// Modified, or a folder whose files changed in more than one way.
        Mix = "mix",
    }
}

string_enum! {
    /// A pane of the session's side region.
    pub enum PanelPane {
        /// The side pane. The default.
        Side = "side",
        /// The bottom pane, under the conversation and the side pane.
        Bottom = "bottom",
        /// Where Settings › Terminal places a new terminal: the side or the bottom pane.
        Dock = "dock",
    }
}

string_enum! {
    /// A text field's look.
    pub enum FieldVariant {
        /// The app's bordered field.
        Default = "default",
        /// A panel sidebar's filter: a search icon, no fill until hovered or focused, and a clear button while it holds text.
        Filter = "filter",
        /// Only the text, caret and selection, at 13 points: the box around it is the plugin's, such as a browser's address field.
        Bare = "bare",
    }
}

string_enum! {
    /// Where the browser engine stands.
    pub enum BrowserEngineState {
        /// This build has no engine for its platform.
        Unsupported = "unsupported",
        /// Not downloaded yet.
        Missing = "missing",
        /// Downloading.
        Downloading = "downloading",
        /// Checking the download.
        Verifying = "verifying",
        /// The background download waits, on a metered network or on battery.
        Paused = "paused",
        /// The download or the engine failed.
        Failed = "failed",
        /// Installed, and starting or connecting.
        Starting = "starting",
        /// Running.
        Ready = "ready",
    }
}

string_enum! {
    /// A page zoom step.
    pub enum BrowserZoom {
        /// One step closer.
        In = "in",
        /// One step further.
        Out = "out",
        /// Back to 100%.
        Reset = "reset",
    }
}

string_enum! {
    /// How an image fills its box.
    pub enum ImageFit {
        /// Whole, aspect kept. The default.
        Contain = "contain",
        /// Fills the box, aspect kept, cropped.
        Cover = "cover",
        /// Stretched to the box.
        Fill = "fill",
    }
}

string_enum! {
    /// An Office document's format.
    pub enum OfficeKind {
        /// Word.
        Docx = "docx",
        /// Excel.
        Xlsx = "xlsx",
        /// PowerPoint.
        Pptx = "pptx",
    }
}

string_enum! {
    /// A toast button's look.
    pub enum ToastActionVariant {
        /// The app's neutral button.
        Primary = "primary",
        /// A ghost button.
        Secondary = "secondary",
    }
}

string_enum! {
    /// What an Office file's first bytes are.
    pub enum OfficeSignature {
        /// A zip archive, as every Office Open XML package is.
        Zip = "zip",
        /// A Compound File: an encrypted file, or one in the binary .doc, .xls or .ppt formats.
        Compound = "compound",
        /// Neither.
        Other = "other",
        /// The file could not be read.
        Unreadable = "unreadable",
    }
}

string_enum! {
    /// Where the app updater stands.
    pub enum UpdaterPhase {
        /// Updates are off in this build or for this install.
        Disabled = "disabled",
        /// Nothing checked yet.
        Idle = "idle",
        /// Asking for a newer version.
        Checking = "checking",
        /// This is the newest version.
        UpToDate = "up-to-date",
        /// Downloading the update. `progress` says how far.
        Downloading = "downloading",
        /// Downloaded. `Install` restarts into it.
        Ready = "ready",
        /// A newer version this install can't update to by itself (Linux). `Install` opens its download.
        DownloadRequired = "download-required",
        /// The release is installed. This process still runs the previous version until the next launch.
        Installed = "installed",
        /// Installing.
        Installing = "installing",
        /// Failed. `error` says why.
        Error = "error",
    }
}

string_enum! {
    /// What a `UiEvent` reports.
    pub enum UiEventKind {
        /// A click, a tap, or Enter or Space on a focused element.
        Press = "press",
        /// A middle click: the wheel button went down on the element and came up over it.
        MiddlePress = "middle-press",
        /// The pointer entered or left. `entered` says which.
        Hover = "hover",
        /// Enter in a field. `text` holds its text.
        Submit = "submit",
        /// A switch or radio changed. `checked` holds the new state.
        Change = "change",
        /// A file tree row was picked. `path` holds it.
        Select = "select",
        /// A virtual list's visible rows changed. `first` and `last` hold them.
        VisibleRows = "visible-rows",
    }
}

string_enum! {
    /// A kind of menu entry.
    pub enum MenuEntryKind {
        /// A row the user can pick. The default.
        Item = "item",
        /// A group title over the rows that follow.
        Group = "group",
        /// A divider.
        Separator = "separator",
    }
}

string_enum! {
    /// A dialog's size: one of Desktop's four presets, each clamped to the window.
    pub enum DialogSize {
        /// Confirmations, sign-ins and small forms: 480 points wide, as tall as its content. The default.
        Normal = "normal",
        /// A short form that reveals a list under a choice: 480 points wide, as tall as its content up to 600 points, its top edge kept so that it only grows downward.
        Tall = "tall",
        /// Palettes, pickers and lists: 680 by 600 points.
        Large = "large",
        /// Long documents and settings-like panels: 880 by 680 points.
        XLarge = "x-large",
        /// Deprecated: the same as `normal`.
        Small = "small",
        /// Deprecated: the same as `normal`.
        Medium = "medium",
    }
}

string_enum! {
    /// Which modal overlay layer a plugin dialog sits in.
    pub enum DialogLayer {
        /// Standard modal dialog layer, beneath elevated system recovery and server dialogs.
        Standard = "standard",
        /// Elevated modal dialog layer, above standard dialogs and recovery prompts. The default.
        Elevated = "elevated",
    }
}

string_enum! {
    /// What a declared store keeps a value for.
    pub enum StoreScope {
        /// One value for the whole app.
        Global = "global",
        /// One value per session.
        Session = "session",
    }
}

string_enum! {
    /// How `ui.progressCircle` draws, as the app's ProgressCircle.
    pub enum ProgressAppearance {
        /// A 14-unit ring with a 1.5 stroke: the session header's.
        Compact = "compact",
        /// A 16-unit ring with a thicker stroke and an overlay over its track: a tab's.
        Indicator = "indicator",
    }
}

string_enum! {
    /// How serious a console line is.
    pub enum ConsoleLevel {
        /// `print` and `Console:Log`, and the host's own notes such as reloads.
        Info = "info",
        /// `warn` and `Console:Warn`.
        Warn = "warn",
        /// An error a plugin raised or wrote with `Console:Error`.
        Error = "error",
    }
}

string_enum! {
    /// Which way the interface language reads.
    pub enum TextDirection {
        /// Left to right.
        Ltr = "ltr",
        /// Right to left: Arabic, Hebrew, Urdu, Punjabi, Persian, Dhivehi.
        Rtl = "rtl",
    }
}

string_enum! {
    /// A CLDR plural category: which form of a counted message the language uses for a count.
    pub enum PluralCategory {
        /// Zero.
        Zero = "zero",
        /// One.
        One = "one",
        /// Two.
        Two = "two",
        /// Few.
        Few = "few",
        /// Many.
        Many = "many",
        /// Every other count, and the fallback form.
        Other = "other",
    }
}

string_enum! {
    /// A date and time format, as the app writes dates.
    pub enum DateStyle {
        /// The time: `3:45 PM`, `15:45`.
        Time = "time",
        /// Month and day: `Nov 3`.
        MonthDay = "monthDay",
        /// Month, day and year: `Nov 3, 2026`.
        MonthDayYear = "monthDayYear",
        /// Medium date and short time: `Nov 3, 2026, 3:45 PM`.
        DateTime = "dateTime",
        /// Month, day and time: `Nov 3, 3:45 PM`.
        MonthDayTime = "monthDayTime",
        /// Month, day, year and time: `Nov 3, 2026, 3:45 PM`.
        MonthDayYearTime = "monthDayYearTime",
        /// Month, day, time and the zone's offset: `Nov 3, 3:45 PM GMT-7`.
        MonthDayTimeZone = "monthDayTimeZone",
        /// The local time zone's short offset at that moment: `GMT-7`, `GMT+5:30`, `UTC`.
        TimeZone = "timeZone",
    }
}

string_enum! {
    /// A unit of relative time.
    pub enum TimeUnit {
        /// Seconds.
        Second = "second",
        /// Minutes.
        Minute = "minute",
        /// Hours.
        Hour = "hour",
        /// Days.
        Day = "day",
        /// Weeks.
        Week = "week",
        /// Months.
        Month = "month",
        /// Years.
        Year = "year",
    }
}

string_enum! {
    /// What a number means.
    pub enum NumberStyle {
        /// A plain number: `12,345.6`.
        Decimal = "decimal",
        /// A share, where 1 is 100%: `42%`.
        Percent = "percent",
        /// Money in `currency`: `$1,234.50`.
        Currency = "currency",
    }
}

string_enum! {
    /// How a number is written.
    pub enum NumberNotation {
        /// Every digit: `12,345`.
        Standard = "standard",
        /// Short, with a unit, lowercased: `12k`, `3m`.
        Compact = "compact",
    }
}
