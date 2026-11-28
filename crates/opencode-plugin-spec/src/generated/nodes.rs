//! The `ui.*` node vocabulary and each node's props, for decoding props tables by key.
//! Generated from crates/opencode-plugin-spec/contract.json by `cargo run -p opencode-plugin-spec -- --write`. Do not edit.

use crate::{PropSpec, PropType, PropsSpec, string_enum};

string_enum! {
    /// A `ui.*` node constructor.
    pub enum NodeKind {
        /// A container with no direction of its own: children stack on top of each other.
        Box = "box",
        /// A horizontal stack.
        Row = "row",
        /// A vertical stack.
        Col = "col",
        /// A layout container whose border takes room in layout, snapped to device pixels at every display scale as a CSS `border-box` border does.
        Frame = "frame",
        /// Flexible space in a stack.
        Spacer = "spacer",
        /// A 1 px rule, laid out as a 1px CSS border is: one device pixel up to 1.5x, two at 2x. A box whose height counts its border holds it beside its content.
        Divider = "divider",
        /// Text. Values after the props fill `{1}`, `{2}`... in `content`, written straight into the text: `ui.text("{1} / {2}", VALUE, read, write)`. Other braces stay as they are.
        Text = "text",
        /// One of the app's icons.
        Icon = "icon",
        /// An image the host holds, or loads from `src`. While it loads its box shows the `placeholder`; `fallback` takes its place when it fails (a site's letter in place of its icon).
        Image = "image",
        /// A video playing muted on a loop while it shows, as a page's `<video autoplay loop muted playsinline>`. Until its first frame, and if it can't play, its box shows what's behind it.
        Video = "video",
        /// A number shown as text. Hosts that can animate changed digits do.
        Metric = "metric",
        /// Text with the app's loading shimmer.
        Shimmer = "shimmer",
        /// A keyboard shortcut chip.
        Keybind = "keybind",
        /// The app's own button.
        Button = "button",
        /// The app's own icon button.
        IconButton = "iconButton",
        /// The app's own switch.
        Switch = "switch",
        /// Desktop's compact checkbox with its label.
        Checkbox = "checkbox",
        /// A radio row.
        Radio = "radio",
        /// The app's own text field.
        Field = "field",
        /// A listbox container (`role="listbox"`) of selectable `ui.option` rows.
        Listbox = "listbox",
        /// One selectable option row (`role="option"`) inside a `ui.listbox`.
        Option = "option",
        /// Desktop's select dropdown control.
        Select = "select",
        /// The app's segmented control.
        Segmented = "segmented",
        /// The app's spinner.
        Loader = "loader",
        /// The app's progress ring: a track and an arc from twelve o'clock, clockwise.
        ProgressCircle = "progressCircle",
        /// A server's health indicator, for a server source's row.
        ServerIndicator = "serverIndicator",
        /// A status capsule.
        Pill = "pill",
        /// Desktop's compact badge.
        Badge = "badge",
        /// A settings row: title and description beside a control.
        SettingsRow = "settingsRow",
        /// A card of settings rows.
        SettingsList = "settingsList",
        /// A popover row that opens a nested popover.
        FlyoutRow = "flyoutRow",
        /// The app's dialog header: title, description and close button. Its close button closes this plugin's dialog, as Esc does.
        DialogHeader = "dialogHeader",
        /// Markdown, as the timeline draws a reply.
        Markdown = "markdown",
        /// A long list that builds only its visible rows.
        VirtualList = "virtualList",
        /// A terminal.
        TerminalSurface = "terminalSurface",
        /// A terminal tab's label: its prompt glyph and title, at most 160 wide.
        TerminalTabLabel = "terminalTabLabel",
        /// A web page.
        BrowserSurface = "browserSurface",
        /// A highlighted file.
        CodeView = "codeView",
        /// Changes, split or unified.
        DiffView = "diffView",
        /// A folder's file tree.
        FileTree = "fileTree",
        /// A resizable sidebar at a panel's start or end, its children stacked in it.
        PanelSidebar = "panelSidebar",
        /// The app's Open in split button for a path.
        OpenInButton = "openInButton",
        /// A Word document's pages or a deck's slides, fit to the pane's width, with zoom controls.
        OfficePages = "officePages",
        /// A workbook's sheet: its headers and the cells under the scroll position.
        OfficeSheet = "officeSheet",
        /// An interactive UI outline, from a ```` ```ui ```` code block.
        GenUi = "genUi",
    }
}

string_enum! {
    /// What a node is for.
    pub enum NodeCategory {
        /// `layout` nodes.
        Layout = "layout",
        /// `content` nodes.
        Content = "content",
        /// `control` nodes.
        Control = "control",
        /// `native` nodes.
        Native = "native",
    }
}

impl NodeKind {
    pub fn category(self) -> NodeCategory {
        match self {
            Self::Box | Self::Row | Self::Col | Self::Frame | Self::Spacer | Self::Divider | Self::Listbox | Self::Option => NodeCategory::Layout,
            Self::Text | Self::Icon | Self::Image | Self::Video | Self::Metric | Self::Shimmer | Self::Keybind | Self::ProgressCircle => NodeCategory::Content,
            Self::Button | Self::IconButton | Self::Switch | Self::Checkbox | Self::Radio | Self::Field | Self::Select | Self::Segmented | Self::Loader | Self::ServerIndicator | Self::Pill | Self::Badge | Self::SettingsRow | Self::SettingsList | Self::FlyoutRow | Self::DialogHeader => NodeCategory::Control,
            Self::Markdown | Self::VirtualList | Self::TerminalSurface | Self::TerminalTabLabel | Self::BrowserSurface | Self::CodeView | Self::DiffView | Self::FileTree | Self::PanelSidebar | Self::OpenInButton | Self::OfficePages | Self::OfficeSheet | Self::GenUi => NodeCategory::Native,
        }
    }

    /// The host renders it with a native engine.
    pub fn is_native(self) -> bool {
        self.category() == NodeCategory::Native
    }

    /// Its props, by key.
    pub fn props(self) -> &'static PropsSpec {
        match self {
            Self::Box | Self::Row | Self::Col => &UI_BOX_PROPS,
            Self::Frame => &UI_FRAME_PROPS,
            Self::Spacer => &UI_SPACER_PROPS,
            Self::Divider => &UI_DIVIDER_PROPS,
            Self::Text | Self::Shimmer => &UI_TEXT_PROPS,
            Self::Icon => &UI_ICON_PROPS,
            Self::Image => &UI_IMAGE_PROPS,
            Self::Video => &UI_VIDEO_PROPS,
            Self::Metric => &UI_METRIC_PROPS,
            Self::Keybind => &UI_KEYBIND_PROPS,
            Self::Button => &UI_BUTTON_PROPS,
            Self::IconButton => &UI_ICON_BUTTON_PROPS,
            Self::Switch => &UI_SWITCH_PROPS,
            Self::Checkbox => &UI_CHECKBOX_PROPS,
            Self::Radio => &UI_RADIO_PROPS,
            Self::Field => &UI_FIELD_PROPS,
            Self::Listbox => &UI_LISTBOX_PROPS,
            Self::Option => &UI_OPTION_PROPS,
            Self::Select => &UI_SELECT_PROPS,
            Self::Segmented => &UI_SEGMENTED_PROPS,
            Self::Loader => &UI_LOADER_PROPS,
            Self::ProgressCircle => &UI_PROGRESS_CIRCLE_PROPS,
            Self::ServerIndicator => &UI_SERVER_INDICATOR_PROPS,
            Self::Pill => &UI_PILL_PROPS,
            Self::Badge => &UI_BADGE_PROPS,
            Self::SettingsRow => &UI_SETTINGS_ROW_PROPS,
            Self::SettingsList => &UI_SETTINGS_LIST_PROPS,
            Self::FlyoutRow => &UI_FLYOUT_ROW_PROPS,
            Self::DialogHeader => &UI_DIALOG_HEADER_PROPS,
            Self::Markdown => &UI_MARKDOWN_PROPS,
            Self::VirtualList => &UI_VIRTUAL_LIST_PROPS,
            Self::TerminalSurface => &UI_TERMINAL_SURFACE_PROPS,
            Self::TerminalTabLabel => &UI_TERMINAL_TAB_LABEL_PROPS,
            Self::BrowserSurface => &UI_BROWSER_SURFACE_PROPS,
            Self::CodeView => &UI_CODE_VIEW_PROPS,
            Self::DiffView => &UI_DIFF_VIEW_PROPS,
            Self::FileTree => &UI_FILE_TREE_PROPS,
            Self::PanelSidebar => &UI_PANEL_SIDEBAR_PROPS,
            Self::OpenInButton => &UI_OPEN_IN_BUTTON_PROPS,
            Self::OfficePages => &UI_OFFICE_PAGES_PROPS,
            Self::OfficeSheet => &UI_OFFICE_SHEET_PROPS,
            Self::GenUi => &UI_GEN_UI_PROPS,
        }
    }
}

/// `UiBadgeProps`.
pub static UI_BADGE_PROPS: PropsSpec = PropsSpec::new("UiBadgeProps", Some(&UI_LAYOUT_PROPS), &[
    PropSpec::new("text", PropType::String, true),
]);

/// `UiBoxProps`.
pub static UI_BOX_PROPS: PropsSpec = PropsSpec::new("UiBoxProps", Some(&UI_LAYOUT_PROPS), &[
    PropSpec::new("gap", PropType::Number, false),
    PropSpec::new("align", PropType::Enum("UiAlign"), false),
    PropSpec::new("justify", PropType::Enum("UiJustify"), false),
    PropSpec::new("center", PropType::Bool, false),
    PropSpec::new("wrap", PropType::Bool, false),
    PropSpec::new("fill", PropType::Color, false),
    PropSpec::new("hover", PropType::Color, false),
    PropSpec::new("press", PropType::Color, false),
    PropSpec::new("selected", PropType::Bool, false),
    PropSpec::new("border", PropType::Color, false),
    PropSpec::new("borderWidth", PropType::Number, false),
    PropSpec::new("radius", PropType::Number, false),
    PropSpec::new("corners", PropType::List("number"), false),
    PropSpec::new("shadow", PropType::Enum("Elevation"), false),
    PropSpec::new("clip", PropType::Bool, false),
    PropSpec::new("scrollY", PropType::Bool, false),
    PropSpec::new("scrollX", PropType::Bool, false),
    PropSpec::new("stickBottom", PropType::Bool, false),
    PropSpec::new("focusable", PropType::Bool, false),
    PropSpec::new("disabled", PropType::Bool, false),
    PropSpec::new("onPress", PropType::Handler, false),
    PropSpec::new("menuTrigger", PropType::Bool, false),
    PropSpec::new("onMiddlePress", PropType::Handler, false),
    PropSpec::new("onHover", PropType::Handler, false),
    PropSpec::new("hoverTransition", PropType::Enum("HoverTransition"), false),
    PropSpec::new("hoverGroup", PropType::Bool, false),
    PropSpec::new("hoverGroupColor", PropType::Color, false),
    PropSpec::new("revealOnHover", PropType::Bool, false),
    PropSpec::new("color", PropType::Color, false),
]);

/// `UiBrowserSurfaceProps`.
pub static UI_BROWSER_SURFACE_PROPS: PropsSpec = PropsSpec::new("UiBrowserSurfaceProps", Some(&UI_LAYOUT_PROPS), &[
    PropSpec::new("areaWidth", PropType::Number, false),
    PropSpec::new("areaHeight", PropType::Number, false),
    PropSpec::new("tab", PropType::Handle("BrowserTab"), true),
    PropSpec::new("focused", PropType::Bool, false),
]);

/// `UiButtonProps`.
pub static UI_BUTTON_PROPS: PropsSpec = PropsSpec::new("UiButtonProps", Some(&UI_LAYOUT_PROPS), &[
    PropSpec::new("text", PropType::String, false),
    PropSpec::new("icon", PropType::String, false),
    PropSpec::new("variant", PropType::Enum("ButtonVariant"), false),
    PropSpec::new("size", PropType::Enum("ControlSize"), false),
    PropSpec::new("disabled", PropType::Bool, false),
    PropSpec::new("loading", PropType::Bool, false),
    PropSpec::new("pressed", PropType::Bool, false),
    PropSpec::new("onPress", PropType::Handler, false),
    PropSpec::new("menuTrigger", PropType::Bool, false),
    PropSpec::new("autofocus", PropType::Bool, false),
    PropSpec::new("focusReturn", PropType::String, false),
]);

/// `UiCheckboxProps`.
pub static UI_CHECKBOX_PROPS: PropsSpec = PropsSpec::new("UiCheckboxProps", Some(&UI_LAYOUT_PROPS), &[
    PropSpec::new("text", PropType::String, true),
    PropSpec::new("description", PropType::String, false),
    PropSpec::new("checked", PropType::Bool, true),
    PropSpec::new("indeterminate", PropType::Bool, false),
    PropSpec::new("surface", PropType::Any, false),
    PropSpec::new("presentation", PropType::Bool, false),
    PropSpec::new("disabled", PropType::Bool, false),
    PropSpec::new("onChange", PropType::Handler, false),
]);

/// `UiCodeViewProps`.
pub static UI_CODE_VIEW_PROPS: PropsSpec = PropsSpec::new("UiCodeViewProps", Some(&UI_LAYOUT_PROPS), &[
    PropSpec::new("doc", PropType::Handle("CodeDocument"), true),
    PropSpec::new("lineWrap", PropType::Bool, false),
    PropSpec::new("showLineNumbers", PropType::Bool, false),
    PropSpec::new("revealLine", PropType::Integer, false),
]);

/// `UiDialogHeaderProps`.
pub static UI_DIALOG_HEADER_PROPS: PropsSpec = PropsSpec::new("UiDialogHeaderProps", Some(&UI_LAYOUT_PROPS), &[
    PropSpec::new("title", PropType::String, true),
    PropSpec::new("description", PropType::String, false),
    PropSpec::new("badge", PropType::String, false),
    PropSpec::new("hideClose", PropType::Bool, false),
    PropSpec::new("closeLabel", PropType::String, false),
    PropSpec::new("wrapTitle", PropType::Bool, false),
]);

/// `UiDiffViewProps`.
pub static UI_DIFF_VIEW_PROPS: PropsSpec = PropsSpec::new("UiDiffViewProps", Some(&UI_LAYOUT_PROPS), &[
    PropSpec::new("diff", PropType::Handle("DiffDocument"), true),
    PropSpec::new("mode", PropType::Enum("DiffMode"), true),
    PropSpec::new("expand", PropType::Bool, false),
    PropSpec::new("file", PropType::String, false),
    PropSpec::new("revealLine", PropType::Integer, false),
    PropSpec::new("list", PropType::Bool, false),
]);

/// `UiDividerProps`.
pub static UI_DIVIDER_PROPS: PropsSpec = PropsSpec::new("UiDividerProps", Some(&UI_LAYOUT_PROPS), &[
    PropSpec::new("vertical", PropType::Bool, false),
    PropSpec::new("color", PropType::Color, false),
]);

/// `UiFieldProps`.
pub static UI_FIELD_PROPS: PropsSpec = PropsSpec::new("UiFieldProps", Some(&UI_LAYOUT_PROPS), &[
    PropSpec::new("editor", PropType::Handle("Editor"), true),
    PropSpec::new("placeholder", PropType::String, false),
    PropSpec::new("size", PropType::Enum("ControlSize"), false),
    PropSpec::new("invalid", PropType::Bool, false),
    PropSpec::new("secure", PropType::Bool, false),
    PropSpec::new("disabled", PropType::Bool, false),
    PropSpec::new("readOnly", PropType::Bool, false),
    PropSpec::new("multiline", PropType::Bool, false),
    PropSpec::new("variant", PropType::Enum("FieldVariant"), false),
    PropSpec::new("icon", PropType::String, false),
    PropSpec::new("clearable", PropType::Bool, false),
    PropSpec::new("focusDown", PropType::String, false),
    PropSpec::new("keys", PropType::List("string"), false),
    PropSpec::new("onSubmit", PropType::Handler, false),
]);

/// `UiFileTreeProps`.
pub static UI_FILE_TREE_PROPS: PropsSpec = PropsSpec::new("UiFileTreeProps", Some(&UI_LAYOUT_PROPS), &[
    PropSpec::new("tree", PropType::Handle("FileTreeModel"), true),
    PropSpec::new("allowed", PropType::List("string"), false),
    PropSpec::new("kinds", PropType::Map, false),
    PropSpec::new("filter", PropType::String, false),
    PropSpec::new("selectedPath", PropType::String, false),
    PropSpec::new("highlightedPath", PropType::String, false),
    PropSpec::new("onSelect", PropType::Handler, false),
    PropSpec::new("onContextMenu", PropType::Handler, false),
    PropSpec::new("narrow", PropType::Bool, false),
    PropSpec::new("width", PropType::Number, false),
]);

/// `UiFlyoutRowProps`.
pub static UI_FLYOUT_ROW_PROPS: PropsSpec = PropsSpec::new("UiFlyoutRowProps", Some(&UI_LAYOUT_PROPS), &[
    PropSpec::new("text", PropType::String, true),
    PropSpec::new("icon", PropType::String, false),
    PropSpec::new("detail", PropType::String, false),
    PropSpec::new("popover", PropType::String, true),
    PropSpec::new("props", PropType::Map, false),
    PropSpec::new("alert", PropType::Bool, false),
    PropSpec::new("side", PropType::Enum("Side"), false),
    PropSpec::new("radius", PropType::Number, false),
]);

/// `UiFrameProps`.
pub static UI_FRAME_PROPS: PropsSpec = PropsSpec::new("UiFrameProps", Some(&UI_BOX_PROPS), &[
    PropSpec::new("direction", PropType::Enum("UiDirection"), false),
    PropSpec::new("borderSides", PropType::List("number"), false),
    PropSpec::new("focusReturn", PropType::String, false),
    PropSpec::new("focusGroup", PropType::Bool, false),
    PropSpec::new("keys", PropType::List("string"), false),
    PropSpec::new("onKey", PropType::Handler, false),
]);

/// `UiGenUiProps`.
pub static UI_GEN_UI_PROPS: PropsSpec = PropsSpec::new("UiGenUiProps", Some(&UI_LAYOUT_PROPS), &[
    PropSpec::new("source", PropType::String, true),
    PropSpec::new("streaming", PropType::Bool, false),
]);

/// `UiIconButtonProps`.
pub static UI_ICON_BUTTON_PROPS: PropsSpec = PropsSpec::new("UiIconButtonProps", Some(&UI_LAYOUT_PROPS), &[
    PropSpec::new("icon", PropType::String, true),
    PropSpec::new("variant", PropType::Enum("ButtonVariant"), false),
    PropSpec::new("size", PropType::Enum("ControlSize"), false),
    PropSpec::new("disabled", PropType::Bool, false),
    PropSpec::new("pressed", PropType::Bool, false),
    PropSpec::new("color", PropType::Color, false),
    PropSpec::new("keybind", PropType::String, false),
    PropSpec::new("onPress", PropType::Handler, false),
    PropSpec::new("menuTrigger", PropType::Bool, false),
    PropSpec::new("autofocus", PropType::Bool, false),
    PropSpec::new("focusReturn", PropType::String, false),
]);

/// `UiIconProps`.
pub static UI_ICON_PROPS: PropsSpec = PropsSpec::new("UiIconProps", Some(&UI_LAYOUT_PROPS), &[
    PropSpec::new("size", PropType::Number, false),
    PropSpec::new("color", PropType::Color, false),
    PropSpec::new("tone", PropType::Enum("Tone"), false),
    PropSpec::new("hoverGroupColor", PropType::Color, false),
    PropSpec::new("revealOnHover", PropType::Bool, false),
]);

/// `UiImageProps`.
pub static UI_IMAGE_PROPS: PropsSpec = PropsSpec::new("UiImageProps", Some(&UI_LAYOUT_PROPS), &[
    PropSpec::new("image", PropType::Handle("ImageId"), false),
    PropSpec::new("src", PropType::String, false),
    PropSpec::new("fit", PropType::Enum("ImageFit"), false),
    PropSpec::new("radius", PropType::Number, false),
    PropSpec::new("placeholder", PropType::Color, false),
    PropSpec::new("fadeIn", PropType::Bool, false),
    PropSpec::new("preview", PropType::Bool, false),
]);

/// `UiKeybindProps`.
pub static UI_KEYBIND_PROPS: PropsSpec = PropsSpec::new("UiKeybindProps", Some(&UI_LAYOUT_PROPS), &[
    PropSpec::new("command", PropType::String, false),
    PropSpec::new("keys", PropType::String, false),
]);

/// `UiLayoutProps`.
pub static UI_LAYOUT_PROPS: PropsSpec = PropsSpec::new("UiLayoutProps", None, &[
    PropSpec::new("key", PropType::String, false),
    PropSpec::new("w", PropType::Size, false),
    PropSpec::new("h", PropType::Size, false),
    PropSpec::new("minW", PropType::Number, false),
    PropSpec::new("minH", PropType::Number, false),
    PropSpec::new("maxW", PropType::Number, false),
    PropSpec::new("maxH", PropType::Number, false),
    PropSpec::new("grow", PropType::Number, false),
    PropSpec::new("shrink", PropType::Bool, false),
    PropSpec::new("pad", PropType::Number, false),
    PropSpec::new("px", PropType::Number, false),
    PropSpec::new("py", PropType::Number, false),
    PropSpec::new("pt", PropType::Number, false),
    PropSpec::new("pr", PropType::Number, false),
    PropSpec::new("pb", PropType::Number, false),
    PropSpec::new("pl", PropType::Number, false),
    PropSpec::new("m", PropType::Number, false),
    PropSpec::new("mx", PropType::Number, false),
    PropSpec::new("my", PropType::Number, false),
    PropSpec::new("mt", PropType::Number, false),
    PropSpec::new("mr", PropType::Number, false),
    PropSpec::new("mb", PropType::Number, false),
    PropSpec::new("ml", PropType::Number, false),
    PropSpec::new("pin", PropType::Record("UiPin"), false),
    PropSpec::new("opacity", PropType::Number, false),
    PropSpec::new("role", PropType::Enum("Role"), false),
    PropSpec::new("label", PropType::String, false),
    PropSpec::new("help", PropType::String, false),
    PropSpec::new("helpSide", PropType::Enum("Side"), false),
    PropSpec::new("helpOpen", PropType::Bool, false),
    PropSpec::new("dir", PropType::Any, false),
    PropSpec::new("cursor", PropType::Any, false),
]);

/// `UiListboxProps`.
pub static UI_LISTBOX_PROPS: PropsSpec = PropsSpec::new("UiListboxProps", Some(&UI_FRAME_PROPS), &[
]);

/// `UiLoaderProps`.
pub static UI_LOADER_PROPS: PropsSpec = PropsSpec::new("UiLoaderProps", Some(&UI_LAYOUT_PROPS), &[
    PropSpec::new("size", PropType::Number, false),
]);

/// `UiMarkdownProps`.
pub static UI_MARKDOWN_PROPS: PropsSpec = PropsSpec::new("UiMarkdownProps", Some(&UI_LAYOUT_PROPS), &[
    PropSpec::new("source", PropType::String, true),
    PropSpec::new("size", PropType::Number, false),
    PropSpec::new("lineHeight", PropType::Number, false),
    PropSpec::new("highlights", PropType::List("string"), false),
    PropSpec::new("selectable", PropType::Bool, false),
    PropSpec::new("streaming", PropType::Bool, false),
]);

/// `UiMetricProps`.
pub static UI_METRIC_PROPS: PropsSpec = PropsSpec::new("UiMetricProps", Some(&UI_LAYOUT_PROPS), &[
    PropSpec::new("value", PropType::Number, true),
    PropSpec::new("text", PropType::String, true),
    PropSpec::new("tabular", PropType::Bool, false),
    PropSpec::new("animate", PropType::Bool, false),
    PropSpec::new("size", PropType::Number, false),
    PropSpec::new("weight", PropType::Enum("FontWeight"), false),
    PropSpec::new("lineHeight", PropType::Number, false),
    PropSpec::new("tracking", PropType::Number, false),
    PropSpec::new("color", PropType::Color, false),
]);

/// `UiOfficePagesProps`.
pub static UI_OFFICE_PAGES_PROPS: PropsSpec = PropsSpec::new("UiOfficePagesProps", Some(&UI_LAYOUT_PROPS), &[
    PropSpec::new("document", PropType::Record("OfficeDocument"), true),
    PropSpec::new("labels", PropType::Record("OfficePagesLabels"), true),
]);

/// `UiOfficeSheetProps`.
pub static UI_OFFICE_SHEET_PROPS: PropsSpec = PropsSpec::new("UiOfficeSheetProps", Some(&UI_LAYOUT_PROPS), &[
    PropSpec::new("document", PropType::Record("OfficeDocument"), true),
    PropSpec::new("labels", PropType::Record("OfficeSheetLabels"), false),
]);

/// `UiOpenInButtonProps`.
pub static UI_OPEN_IN_BUTTON_PROPS: PropsSpec = PropsSpec::new("UiOpenInButtonProps", Some(&UI_LAYOUT_PROPS), &[
    PropSpec::new("path", PropType::String, true),
]);

/// `UiOptionProps`.
pub static UI_OPTION_PROPS: PropsSpec = PropsSpec::new("UiOptionProps", Some(&UI_FRAME_PROPS), &[
    PropSpec::new("selected", PropType::Bool, false),
]);

/// `UiPanelSidebarProps`.
pub static UI_PANEL_SIDEBAR_PROPS: PropsSpec = PropsSpec::new("UiPanelSidebarProps", Some(&UI_LAYOUT_PROPS), &[
    PropSpec::new("id", PropType::String, true),
    PropSpec::new("width", PropType::Number, false),
    PropSpec::new("minWidth", PropType::Number, false),
    PropSpec::new("maxWidth", PropType::Number, false),
    PropSpec::new("locked", PropType::Bool, false),
]);

/// `UiPillProps`.
pub static UI_PILL_PROPS: PropsSpec = PropsSpec::new("UiPillProps", Some(&UI_LAYOUT_PROPS), &[
    PropSpec::new("text", PropType::String, true),
    PropSpec::new("icon", PropType::String, false),
    PropSpec::new("tone", PropType::Enum("Tone"), false),
    PropSpec::new("pressed", PropType::Bool, false),
    PropSpec::new("onPress", PropType::Handler, false),
]);

/// `UiProgressCircleProps`.
pub static UI_PROGRESS_CIRCLE_PROPS: PropsSpec = PropsSpec::new("UiProgressCircleProps", Some(&UI_LAYOUT_PROPS), &[
    PropSpec::new("percentage", PropType::Number, true),
    PropSpec::new("appearance", PropType::Enum("ProgressAppearance"), false),
    PropSpec::new("size", PropType::Number, false),
    PropSpec::new("strokeWidth", PropType::Number, false),
    PropSpec::new("track", PropType::Color, false),
    PropSpec::new("overlay", PropType::Color, false),
    PropSpec::new("progress", PropType::Color, false),
]);

/// `UiRadioProps`.
pub static UI_RADIO_PROPS: PropsSpec = PropsSpec::new("UiRadioProps", Some(&UI_LAYOUT_PROPS), &[
    PropSpec::new("text", PropType::String, true),
    PropSpec::new("description", PropType::String, false),
    PropSpec::new("checked", PropType::Bool, true),
    PropSpec::new("tone", PropType::Enum("Tone"), false),
    PropSpec::new("lineHeight", PropType::Number, false),
    PropSpec::new("disabled", PropType::Bool, false),
    PropSpec::new("onChange", PropType::Handler, false),
]);

/// `UiSegmentedProps`.
pub static UI_SEGMENTED_PROPS: PropsSpec = PropsSpec::new("UiSegmentedProps", Some(&UI_LAYOUT_PROPS), &[
    PropSpec::new("items", PropType::List("UiSegmentedItem"), true),
    PropSpec::new("value", PropType::String, true),
    PropSpec::new("onChange", PropType::Handler, false),
]);

/// `UiSelectProps`.
pub static UI_SELECT_PROPS: PropsSpec = PropsSpec::new("UiSelectProps", Some(&UI_LAYOUT_PROPS), &[
    PropSpec::new("value", PropType::String, false),
    PropSpec::new("placeholder", PropType::String, false),
    PropSpec::new("options", PropType::List("UiSelectOption"), true),
    PropSpec::new("disabled", PropType::Bool, false),
    PropSpec::new("onChange", PropType::Handler, false),
]);

/// `UiServerIndicatorProps`.
pub static UI_SERVER_INDICATOR_PROPS: PropsSpec = PropsSpec::new("UiServerIndicatorProps", Some(&UI_LAYOUT_PROPS), &[
    PropSpec::new("health", PropType::Record("ServerHealth"), false),
    PropSpec::new("auth", PropType::Bool, false),
    PropSpec::new("connecting", PropType::Bool, false),
]);

/// `UiSettingsListProps`.
pub static UI_SETTINGS_LIST_PROPS: PropsSpec = PropsSpec::new("UiSettingsListProps", Some(&UI_LAYOUT_PROPS), &[
    PropSpec::new("title", PropType::String, false),
    PropSpec::new("catalog", PropType::Bool, false),
]);

/// `UiSettingsRowProps`.
pub static UI_SETTINGS_ROW_PROPS: PropsSpec = PropsSpec::new("UiSettingsRowProps", Some(&UI_LAYOUT_PROPS), &[
    PropSpec::new("id", PropType::String, true),
    PropSpec::new("title", PropType::String, true),
    PropSpec::new("description", PropType::String, false),
]);

/// `UiSpacerProps`.
pub static UI_SPACER_PROPS: PropsSpec = PropsSpec::new("UiSpacerProps", Some(&UI_LAYOUT_PROPS), &[
]);

/// `UiSwitchProps`.
pub static UI_SWITCH_PROPS: PropsSpec = PropsSpec::new("UiSwitchProps", Some(&UI_LAYOUT_PROPS), &[
    PropSpec::new("checked", PropType::Bool, true),
    PropSpec::new("disabled", PropType::Bool, false),
    PropSpec::new("onChange", PropType::Handler, false),
]);

/// `UiTerminalSurfaceProps`.
pub static UI_TERMINAL_SURFACE_PROPS: PropsSpec = PropsSpec::new("UiTerminalSurfaceProps", Some(&UI_LAYOUT_PROPS), &[
    PropSpec::new("session", PropType::Handle("PtySession"), false),
    PropSpec::new("fontFamily", PropType::String, false),
    PropSpec::new("fontSize", PropType::Number, false),
    PropSpec::new("focused", PropType::Bool, false),
    PropSpec::new("width", PropType::Number, false),
    PropSpec::new("height", PropType::Number, false),
    PropSpec::new("narrow", PropType::Bool, false),
    PropSpec::new("sessionId", PropType::String, false),
]);

/// `UiTerminalTabLabelProps`.
pub static UI_TERMINAL_TAB_LABEL_PROPS: PropsSpec = PropsSpec::new("UiTerminalTabLabelProps", Some(&UI_LAYOUT_PROPS), &[
    PropSpec::new("session", PropType::Handle("PtySession"), true),
    PropSpec::new("title", PropType::String, true),
    PropSpec::new("selected", PropType::Bool, false),
]);

/// `UiTextProps`.
pub static UI_TEXT_PROPS: PropsSpec = PropsSpec::new("UiTextProps", Some(&UI_LAYOUT_PROPS), &[
    PropSpec::new("size", PropType::Number, false),
    PropSpec::new("weight", PropType::Enum("FontWeight"), false),
    PropSpec::new("lineHeight", PropType::Number, false),
    PropSpec::new("tracking", PropType::Number, false),
    PropSpec::new("color", PropType::Color, false),
    PropSpec::new("mono", PropType::Bool, false),
    PropSpec::new("tabular", PropType::Bool, false),
    PropSpec::new("wrap", PropType::Bool, false),
    PropSpec::new("noWrap", PropType::Bool, false),
    PropSpec::new("truncate", PropType::Bool, false),
    PropSpec::new("fadeOut", PropType::Bool, false),
    PropSpec::new("selectable", PropType::Bool, false),
    PropSpec::new("textAlign", PropType::Enum("Align"), false),
    PropSpec::new("hoverGroupColor", PropType::Color, false),
    PropSpec::new("revealOnHover", PropType::Bool, false),
]);

/// `UiVideoProps`.
pub static UI_VIDEO_PROPS: PropsSpec = PropsSpec::new("UiVideoProps", Some(&UI_LAYOUT_PROPS), &[
    PropSpec::new("src", PropType::String, true),
    PropSpec::new("fit", PropType::Enum("ImageFit"), false),
    PropSpec::new("radius", PropType::Number, false),
]);

/// `UiVirtualListProps`.
pub static UI_VIRTUAL_LIST_PROPS: PropsSpec = PropsSpec::new("UiVirtualListProps", Some(&UI_LAYOUT_PROPS), &[
    PropSpec::new("count", PropType::Integer, true),
    PropSpec::new("estimatedRowHeight", PropType::Number, true),
    PropSpec::new("version", PropType::MemoVersion, true),
    PropSpec::new("rowVersions", PropType::List("MemoVersion"), false),
    PropSpec::new("scrollY", PropType::Bool, false),
    PropSpec::new("renderRow", PropType::Function, true),
    PropSpec::new("rowKey", PropType::Function, false),
    PropSpec::new("gap", PropType::Number, false),
    PropSpec::new("onVisibleRows", PropType::Handler, false),
]);
