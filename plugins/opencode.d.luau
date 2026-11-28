-- Brocode plugin contract 1.13: global types for luau-lsp.
-- Generated from crates/opencode-plugin-spec/contract.json by `cargo run -p opencode-plugin-spec -- --write`. Do not edit.
-- Load it with `luau-lsp analyze --definitions=@opencode=plugins/opencode.d.luau`.

-- Semantic tokens ---------------------------------------------------------------------------------

--- A theme color by meaning. The host resolves it for the current theme and color scheme.
export type SemanticToken =
    "bg-base" -- The window's base background.
    | "bg-weak" -- A background one step quieter than `bg-base`, such as a sidebar.
    | "bg-strong" -- A background one step stronger than `bg-base`, such as a title bar.
    | "surface-1" -- The first raised layer: cards and grouped lists.
    | "surface-2" -- The second raised layer: controls on a card.
    | "surface-3" -- The third raised layer: a control's track, such as an off switch.
    | "surface-4" -- The fourth raised layer: the strongest fill a control uses.
    | "surface-hover" -- The fill of a row or button under the pointer.
    | "surface-press" -- The fill of a row or button while it is pressed.
    | "surface-selected" -- The fill of a selected row or tab.
    | "surface-inverse" -- A high-contrast inverse fill, such as a progress bar fill or solid contrast indicator. Since 1.5.
    | "text-strong" -- Headings and the most important text.
    | "text-base" -- Body text.
    | "text-weak" -- Secondary text: descriptions and labels.
    | "text-weaker" -- Tertiary text: hints, placeholders and timestamps.
    | "text-muted" -- Secondary text in the app's chrome: header figures' labels, menu row names, tab titles. Hosts without a separate chrome scale use `text-weak`.
    | "text-faint" -- Quiet figures and hints in the app's chrome, such as a header's counts. Hosts without a separate chrome scale use `text-weaker`.
    | "text-chrome" -- Text in the app's chrome at full strength: menu and popover rows, header titles, list items. Hosts without a separate chrome scale use `text-strong`.
    | "text-on-accent" -- Text on an `accent` fill.
    | "text-interactive" -- Links and other interactive text.
    | "icon-strong" -- An icon that needs emphasis.
    | "icon-base" -- The default icon color.
    | "icon-weak" -- A secondary icon.
    | "icon-weaker" -- A decorative or disabled icon.
    | "icon-on-accent" -- An icon on an `accent` fill.
    | "border-strong" -- A strong border, such as a focused field's outline.
    | "border-base" -- The default border.
    | "border-weak" -- A hairline divider or a card's border.
    | "border-focus" -- The keyboard focus ring.
    | "accent" -- The brand accent: primary buttons and switches that are on.
    | "accent-hover" -- An `accent` fill under the pointer.
    | "accent-subtle" -- A quiet accent tint, such as a selected chip's fill.
    | "status-success" -- Done or passing.
    | "status-warning" -- Needs attention.
    | "status-danger" -- Failed, blocked or destructive.
    | "status-info" -- Neutral information.
    | "status-success-bg" -- A tinted background for success callouts and banners. Since 1.5.
    | "status-warning-bg" -- A tinted background for warning callouts and banners. Since 1.5.
    | "status-danger-bg" -- A tinted background for danger callouts and banners. Since 1.5.
    | "status-info-bg" -- A tinted background for informational callouts and banners. Since 1.5.
    | "status-danger-strong" -- High-contrast danger ink for text and indicators on light or tinted danger surfaces. Since 1.5.
    | "avatar-orange" -- A letter tile's fill, as the app's project avatars draw: the first of eight a name hashes to.
    | "avatar-yellow" -- A letter tile's fill, the second of eight.
    | "avatar-cyan" -- A letter tile's fill, the third of eight.
    | "avatar-green" -- A letter tile's fill, the fourth of eight.
    | "avatar-red" -- A letter tile's fill, the fifth of eight.
    | "avatar-pink" -- A letter tile's fill, the sixth of eight.
    | "avatar-blue" -- A letter tile's fill, the seventh of eight.
    | "avatar-purple" -- A letter tile's fill, the eighth of eight.
    | "avatar-text" -- The letter on a tile.
    | "avatar-ring" -- The hairline inside a tile's edge.

-- Enums -------------------------------------------------------------------------------------------

--- Which half of a plugin runs the code.
export type Half = "window" | "app"

--- The operating system the host runs on.
export type Platform = "windows" | "macos" | "linux" | "web"

--- The app build's release channel.
export type Channel = "local" | "dev" | "beta" | "prod"

--- A light or dark color scheme.
export type ColorScheme = "light" | "dark"

--- A status color for an icon or a pill.
export type Tone = "success" | "warning" | "danger" | "accent"

--- A toast's tone.
export type ToastVariant = "default" | "success" | "error" | "loading"

--- The section of Settings › Shortcuts that lists a command.
export type CommandSection = "general" | "session" | "navigation" | "model" | "terminal" | "prompt"

--- Where a panel's narrow-screen view is offered.
export type PanelMobileKind = "tab" | "menu" | "drawer"

--- A host menu a menu item joins.
export type MenuName = "session.panel" | "server.add" | "server.row"

--- A submenu of the app menu bar.
export type MenubarMenu = "app" | "file" | "edit" | "view" | "go" | "window" | "help"

--- A fixed place in the app's shell that a slot fills.
export type SlotName = "window.bottom" | "session.header" | "session.location.menu" | "session.panel.end" | "session.panel.new" | "session.panel.new.list"

--- A host settings page a settings page can add a section to.
export type HostSettingsPage = "general" | "servers"

--- Where a contributed server stands.
export type ServerState = "stopped" | "starting" | "auth" | "ready" | "failed" | "incompatible"

--- Where a plugin's panel tab is in a session.
export type PanelState = "closed" | "listed" | "selected" | "visible"

--- How a layout change moves.
export type Transition = "default" | "none"

--- How hover and press fills change.
export type HoverTransition = "default" | "instant"

--- A side of an anchor.
export type Side = "top" | "bottom" | "left" | "right"

--- Alignment along an edge or an axis.
export type Align = "start" | "center" | "end"

--- How a stack places its children across its axis.
export type UiAlign = "start" | "center" | "end" | "stretch"

--- How a stack places its children along its axis.
export type UiJustify = "start" | "center" | "end" | "between"

--- How a `ui.frame` container arranges its children.
export type UiDirection = "col" | "row" | "box"

--- The edge of a panel its file sidebar sits on.
export type PanelSidebarSide = "start" | "end"

--- Which picker overlay the composer currently has open.
export type ComposerPicker = "agent" | "model" | "effort" | "speed" | "project"

--- A text weight.
export type FontWeight = "regular" | "medium" | "semibold" | "bold"

--- One of the user's font choices.
export type FontFamily = "sans" | "mono"

--- A button's look, as the app's own buttons.
export type ButtonVariant = "neutral" | "contrast" | "primary" | "ghost" | "ghost-muted" | "outline" | "danger"

--- A control's size.
export type ControlSize = "small" | "normal" | "large"

--- A shadow from the app's elevation scale.
export type Elevation = "raised" | "floating" | "overlay"

--- An element's accessibility role.
export type Role = "none" | "group" | "region" | "button" | "link" | "switch" | "checkbox" | "radio" | "tab" | "list" | "listitem" | "heading" | "img" | "status" | "timer" | "alert"

--- The state of a contract another plugin provides.
export type LiveState = "pending" | "active" | "inactive" | "failed"

--- The state of a cached server read.
export type ResourceStatus = "loading" | "ready" | "error"

--- An HTTP method.
export type HttpMethod = "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD"

--- A process's output stream.
export type ProcessStream = "stdout" | "stderr"

--- How a process encodes its output.
export type TextEncoding = "utf8" | "utf16le"

--- How a diff lays out its changes.
export type DiffMode = "split" | "unified"

--- What a diff compares.
export type DiffSource = "git" | "branch" | "turn"

--- How a diff's read went.
export type DiffStatus = "loading" | "ready" | "no-vcs" | "offline" | "error"

--- How a file or folder changed.
export type ChangeKind = "add" | "del" | "mix"

--- A pane of the session's side region.
export type PanelPane = "side" | "bottom" | "dock"

--- A text field's look.
export type FieldVariant = "default" | "filter" | "bare"

--- Where the browser engine stands.
export type BrowserEngineState = "unsupported" | "missing" | "downloading" | "verifying" | "paused" | "failed" | "starting" | "ready"

--- A page zoom step.
export type BrowserZoom = "in" | "out" | "reset"

--- How an image fills its box.
export type ImageFit = "contain" | "cover" | "fill"

--- Where an image stands (`Images:State`): an explicit machine, loading then loaded or failed.
export type ImageStatus = "loading" | "loaded" | "failed"

--- An Office document's format.
export type OfficeKind = "docx" | "xlsx" | "pptx"

--- A toast button's look.
export type ToastActionVariant = "primary" | "secondary"

--- What an Office file's first bytes are.
export type OfficeSignature = "zip" | "compound" | "other" | "unreadable"

--- Where the app updater stands.
export type UpdaterPhase = "disabled" | "idle" | "checking" | "up-to-date" | "downloading" | "ready" | "download-required" | "installed" | "installing" | "error"

--- What a `UiEvent` reports.
export type UiEventKind = "press" | "middle-press" | "hover" | "submit" | "change" | "select" | "visible-rows"

--- A kind of menu entry.
export type MenuEntryKind = "item" | "group" | "separator"

--- A dialog's size: one of Desktop's four presets, each clamped to the window.
export type DialogSize = "normal" | "tall" | "large" | "x-large" | "small" | "medium"

--- Which modal overlay layer a plugin dialog sits in.
export type DialogLayer = "standard" | "elevated"

--- What a declared store keeps a value for.
export type StoreScope = "global" | "session"

--- How `ui.progressCircle` draws, as the app's ProgressCircle.
export type ProgressAppearance = "compact" | "indicator"

--- How serious a console line is.
export type ConsoleLevel = "info" | "warn" | "error"

--- Which way the interface language reads.
export type TextDirection = "ltr" | "rtl"

--- A CLDR plural category: which form of a counted message the language uses for a count.
export type PluralCategory = "zero" | "one" | "two" | "few" | "many" | "other"

--- A date and time format, as the app writes dates.
export type DateStyle = "time" | "monthDay" | "monthDayYear" | "dateTime" | "monthDayTime" | "monthDayYearTime" | "monthDayTimeZone" | "timeZone"

--- A unit of relative time.
export type TimeUnit = "second" | "minute" | "hour" | "day" | "week" | "month" | "year"

--- What a number means.
export type NumberStyle = "decimal" | "percent" | "currency"

--- How a number is written.
export type NumberNotation = "standard" | "compact"

-- Opaque host types -------------------------------------------------------------------------------

--- A literal color from `ui.hex("#RRGGBB")` or `ui.hex("#RRGGBBAA")`. Prefer a `SemanticToken`: a literal color does not follow the theme.
declare extern type HexColor with end

--- An element a `ui.*` constructor built. Pass it to a parent or return it from a render.
declare extern type UiNode with end

--- An event handler `view:Handler` made. It runs as a coroutine, so it may yield.
declare extern type UiHandler with end

--- An image the host holds, for `ui.image`.
declare extern type ImageId with end

--- What `plugin.defineWindow` and `plugin.defineApp` return. Return it from the entry module.
declare extern type PluginHalf with end

-- Aliases, records and host objects ---------------------------------------------------------------

--- A semantic token, or a literal color from `ui.hex`.
export type Color = SemanticToken | HexColor

--- The name of one of the app's icons, such as `"circle-check"`, `file:<name>` for the type icon of a file so named, or `<plugin>:<name>` for an icon declared by another plugin. A plugin's own `icons/<name>.svg` files go by their file name and win over the app's wherever that plugin names an icon: its `ui.*` nodes and every icon field it hands the host (a settings page, composer pill, titlebar item, session panel tab, closed tab, launcher, mobile view, or code block action). Each is a sanitized SVG document whose `viewBox` (or `width` and `height`) frames safe vector geometry and optional SMIL animation elements (`animate`, `animateTransform`, `set`, held on frame 0 under reduced motion) drawn with `currentColor`. Malformed, oversized, or disallowed SVGs fall back to a default icon.
export type IconName = string

--- A memo's version. The host rebuilds the memo only when it changes.
export type MemoVersion = string | number | boolean

--- A length in logical points, or `"full"` to fill the parent.
export type UiSize = number | "full"

--- An event source. Connections disconnect on their own when the plugin half that made them goes away.
export type Signal<T...> = {
    --- Calls `callback` on every fire, as a coroutine.
    Connect: (self: Signal<T...>, callback: (T...) -> ()) -> Connection,
    --- Calls `callback` on the next fire only.
    Once: (self: Signal<T...>, callback: (T...) -> ()) -> Connection,
    --- Yields until the next fire and returns its values. Yields: call it from a handler, a task or a signal callback.
    Wait: (self: Signal<T...>) -> T...,
}

--- A signal a plugin fires itself, from `ctx:CreateSignal()`. Its version counts fires: put it in a memo's version.
export type ScriptSignal<T...> = {
    --- Calls `callback` on every fire, as a coroutine.
    Connect: (self: ScriptSignal<T...>, callback: (T...) -> ()) -> Connection,
    --- Calls `callback` on the next fire only.
    Once: (self: ScriptSignal<T...>, callback: (T...) -> ()) -> Connection,
    --- Yields until the next fire and returns its values. Yields: call it from a handler, a task or a signal callback.
    Wait: (self: ScriptSignal<T...>) -> T...,
    --- Fires the signal, adds 1 to its version and asks the window to redraw.
    Fire: (self: ScriptSignal<T...>, T...) -> (),
    --- How many times the signal fired.
    Version: (self: ScriptSignal<T...>) -> number,
}

--- One connection to a signal.
export type Connection = {
    --- False once disconnected.
    Connected: boolean,
    --- Stops the callback. Does nothing when already disconnected.
    Disconnect: (self: Connection) -> (),
}

--- A contribution a plugin added. The host withdraws it when the plugin half goes away.
export type Contribution = {
    --- Removes the contribution now.
    Withdraw: (self: Contribution) -> (),
    --- Asks the host to call the contribution's `show`, `list` or `render` again.
    Refresh: (self: Contribution) -> (),
}

--- A contract another plugin provides, followed live.
export type Live<T> = {
    --- The provider's state.
    State: (self: Live<T>) -> LiveState,
    --- The provided value while the state is `active`.
    Get: (self: Live<T>) -> T?,
    --- Changes on every state change and every new value: put it in a memo's version.
    Generation: (self: Live<T>) -> number,
    --- Fires on every state change and every new value.
    Changed: Signal<LiveState>,
}

--- What a render callback receives first. Render callbacks must not yield.
export type View = {
    --- Returns the subtree `build` returns, and calls `build` again only when `version` changes. An unchanged frame costs no Luau call. Keep `key` unique within the render.
    Memo: (self: View, key: string, version: MemoVersion, build: () -> UiNode?) -> UiNode?,
    --- Wraps `callback` for an `on*` property. The handler runs as a coroutine, so it may call `Async` methods and `task.wait`.
    Handler: (self: View, callback: (event: UiEvent) -> ()) -> UiHandler,
}

--- What a handler receives.
export type UiEvent = {
    --- What happened.
    kind: UiEventKind,
    --- The `key` of the element the event came from.
    key: string?,
    --- A `hover` event: true as the pointer enters, false as it leaves.
    entered: boolean?,
    --- A `submit` event: the field's text.
    text: string?,
    --- A `change` event: the new state.
    checked: boolean?,
    --- A `select` event: the picked path.
    path: string?,
    --- A `change` event of `ui.segmented`: the picked item's id.
    value: string?,
    --- A `visible-rows` event: the first visible row, from 1.
    first: number?,
    --- A `visible-rows` event: the last visible row.
    last: number?,
    --- Shift was held.
    shift: boolean?,
    --- Alt (Option) was held.
    alt: boolean?,
    --- The platform's command key was held: Ctrl, or Cmd on macOS.
    mod: boolean?,
}

--- A host-owned text field's content: typing, IME, selection and undo stay in the host and survive a hot reload.
export type Editor = {
    --- The field's text.
    Text: (self: Editor) -> string,
    --- Replaces the text and puts the caret at its end.
    SetText: (self: Editor, text: string) -> (),
    --- Changes whenever the text, selection, composition or focus does: put it in a memo's version.
    Version: (self: Editor) -> number,
    --- Moves keyboard focus to the field.
    Focus: (self: Editor) -> (),
    --- Takes keyboard focus from the field.
    Blur: (self: Editor) -> (),
    --- Selects the whole text.
    SelectAll: (self: Editor) -> (),
    --- The field has keyboard focus.
    Focused: (self: Editor) -> boolean,
    --- Fires with the new text after each edit.
    Changed: Signal<string>,
    --- Fires with the text when the user presses Enter.
    Submitted: Signal<string>,
    --- Fires as the field takes keyboard focus (true) or loses it (false).
    FocusChanged: Signal<boolean>,
    --- Fires with a key the field's `keys` hands over (`ArrowUp`, `ArrowDown`, `Escape`) instead of handling it.
    KeyPressed: Signal<string>,
}

--- Size, spacing, position and accessibility: every `ui.*` node takes these. A props table frozen with `table.freeze` is decoded once and its node copied after that, so keep styles many nodes share in frozen tables made once (`local LABEL = table.freeze({ size = 13, color = "text-weak" })`). A node whose props are partly its own takes the frozen style first and a small table of its own props after it, laid over the style: `ui.row(ROW, { key = id, onPress = open }, children)`, `ui.text(words, LABEL, { color = ink })`, `ui.icon(name, ICON, { color = ink })`. Only those are decoded; a prop in both takes the second table's value.
export type UiLayoutProps = {
    --- Identity among siblings across renders, and the anchor key for popovers and menus.
    key: string?,
    --- Width.
    w: UiSize?,
    --- Height.
    h: UiSize?,
    --- Minimum width.
    minW: number?,
    --- Minimum height.
    minH: number?,
    --- Maximum width.
    maxW: number?,
    --- Maximum height.
    maxH: number?,
    --- Share of a stack's free space.
    grow: number?,
    --- May shrink below its content size.
    shrink: boolean?,
    --- Padding on every side.
    pad: number?,
    --- Left and right padding.
    px: number?,
    --- Top and bottom padding.
    py: number?,
    --- Top padding.
    pt: number?,
    --- Right padding.
    pr: number?,
    --- Bottom padding.
    pb: number?,
    --- Left padding.
    pl: number?,
    --- Margin on every side.
    m: number?,
    --- Left and right margin.
    mx: number?,
    --- Top and bottom margin.
    my: number?,
    --- Top margin.
    mt: number?,
    --- Right margin.
    mr: number?,
    --- Bottom margin.
    mb: number?,
    --- Left margin.
    ml: number?,
    --- Takes the node out of flow, at these offsets from its parent's edges.
    pin: UiPin?,
    --- 0 to 1.
    opacity: number?,
    --- The accessibility role.
    role: Role?,
    --- The accessible name.
    label: string?,
    --- A tooltip.
    help: string?,
    --- Where the tooltip shows. Defaults to below.
    helpSide: Side?,
    --- Keeps the tooltip showing while true, whatever the pointer does, as Desktop's `Tooltip forceOpen`: a Copy button's "Copied" for a moment after the press.
    helpOpen: boolean?,
    --- Which way the subtree reads, as HTML's `dir` does: `ltr` or `rtl` keeps a figure or a path in that order inside a window running the other way, `auto` takes a text's first strong direction (a name). On `ui.field` it is the value's direction, as on an `input`: `auto` starts an Arabic name at the right, `ltr` keeps an address or a host left to right. Defaults to the window's.
    dir: ("ltr" | "rtl" | "auto")?,
    --- The pointer's shape over the node and over what is inside it that sets none, by its CSS `cursor` name. Desktop shows the arrow on controls; set this only where its style sheet names another cursor, as the browser's address field does (`text`).
    cursor: ("default" | "pointer" | "text" | "grab" | "grabbing" | "not-allowed" | "move" | "col-resize" | "row-resize" | "ew-resize" | "ns-resize" | "nwse-resize" | "nesw-resize" | "crosshair" | "cell" | "zoom-in" | "zoom-out")?,
}

--- Offsets from the parent's edges for an out-of-flow node. Give one or two per axis.
export type UiPin = {
    --- From the left edge.
    left: number?,
    --- From the top edge.
    top: number?,
    --- From the right edge.
    right: number?,
    --- From the bottom edge.
    bottom: number?,
}

--- A container's props: `ui.box`, `ui.row` and `ui.col`.
export type UiBoxProps = {
    --- Identity among siblings across renders, and the anchor key for popovers and menus.
    key: string?,
    --- Width.
    w: UiSize?,
    --- Height.
    h: UiSize?,
    --- Minimum width.
    minW: number?,
    --- Minimum height.
    minH: number?,
    --- Maximum width.
    maxW: number?,
    --- Maximum height.
    maxH: number?,
    --- Share of a stack's free space.
    grow: number?,
    --- May shrink below its content size.
    shrink: boolean?,
    --- Padding on every side.
    pad: number?,
    --- Left and right padding.
    px: number?,
    --- Top and bottom padding.
    py: number?,
    --- Top padding.
    pt: number?,
    --- Right padding.
    pr: number?,
    --- Bottom padding.
    pb: number?,
    --- Left padding.
    pl: number?,
    --- Margin on every side.
    m: number?,
    --- Left and right margin.
    mx: number?,
    --- Top and bottom margin.
    my: number?,
    --- Top margin.
    mt: number?,
    --- Right margin.
    mr: number?,
    --- Bottom margin.
    mb: number?,
    --- Left margin.
    ml: number?,
    --- Takes the node out of flow, at these offsets from its parent's edges.
    pin: UiPin?,
    --- 0 to 1.
    opacity: number?,
    --- The accessibility role.
    role: Role?,
    --- The accessible name.
    label: string?,
    --- A tooltip.
    help: string?,
    --- Where the tooltip shows. Defaults to below.
    helpSide: Side?,
    --- Keeps the tooltip showing while true, whatever the pointer does, as Desktop's `Tooltip forceOpen`: a Copy button's "Copied" for a moment after the press.
    helpOpen: boolean?,
    --- Which way the subtree reads, as HTML's `dir` does: `ltr` or `rtl` keeps a figure or a path in that order inside a window running the other way, `auto` takes a text's first strong direction (a name). On `ui.field` it is the value's direction, as on an `input`: `auto` starts an Arabic name at the right, `ltr` keeps an address or a host left to right. Defaults to the window's.
    dir: ("ltr" | "rtl" | "auto")?,
    --- The pointer's shape over the node and over what is inside it that sets none, by its CSS `cursor` name. Desktop shows the arrow on controls; set this only where its style sheet names another cursor, as the browser's address field does (`text`).
    cursor: ("default" | "pointer" | "text" | "grab" | "grabbing" | "not-allowed" | "move" | "col-resize" | "row-resize" | "ew-resize" | "ns-resize" | "nwse-resize" | "nesw-resize" | "crosshair" | "cell" | "zoom-in" | "zoom-out")?,
    --- Space between children.
    gap: number?,
    --- How children sit across the axis.
    align: UiAlign?,
    --- How children sit along the axis.
    justify: UiJustify?,
    --- Centers children on both axes.
    center: boolean?,
    --- Children wrap onto more lines.
    wrap: boolean?,
    --- Background.
    fill: Color?,
    --- Background under the pointer.
    hover: Color?,
    --- Background while pressed.
    press: Color?,
    --- Draws the `surface-selected` fill and reports the selected state to assistive technology.
    selected: boolean?,
    --- Border color.
    border: Color?,
    --- Border width. Defaults to 1 when `border` is set.
    borderWidth: number?,
    --- Corner radius.
    radius: number?,
    --- Per-corner radii: top-left, top-right, bottom-right, bottom-left.
    corners: { number }?,
    --- A shadow from the app's elevation scale.
    shadow: Elevation?,
    --- Clips children to the box.
    clip: boolean?,
    --- Scrolls vertical overflow.
    scrollY: boolean?,
    --- Scrolls horizontal overflow.
    scrollX: boolean?,
    --- With `scrollY`: stays scrolled to the bottom as content grows, as a log or a chat does, until the reader scrolls up.
    stickBottom: boolean?,
    --- Takes keyboard focus.
    focusable: boolean?,
    --- Takes no presses.
    disabled: boolean?,
    --- Runs on a press.
    onPress: UiHandler?,
    --- Its `onPress` opens a menu, as Kobalte's `Menu.Trigger` does: a mouse runs it as the button goes down, and that press's release runs nothing. A key, a screen reader's Invoke or a tap run it on the click. Contract 1.1; a 1.0 host ignores it with one warning and opens on the click.
    menuTrigger: boolean?,
    --- Runs on a middle click, as a browser's `auxclick`: a tab closes, a row opens in the background. A `middle-press` event, with the modifier keys.
    onMiddlePress: UiHandler?,
    --- Runs as the pointer enters and leaves.
    onHover: UiHandler?,
    --- How hover and press fills change.
    hoverTransition: HoverTransition?,
    --- Starts a hover group: descendants with `hoverGroupColor` or `revealOnHover` react while this box is hovered.
    hoverGroup: boolean?,
    --- The text or icon color while the nearest hover group is hovered.
    hoverGroupColor: Color?,
    --- Shows only while the nearest hover group is hovered or focused.
    revealOnHover: boolean?,
    --- The default text and icon color for descendants.
    color: Color?,
}

--- `ui.frame` props: a layout container whose border takes layout space snapped to device pixels (`box-sizing: border-box`).
export type UiFrameProps = {
    --- Identity among siblings across renders, and the anchor key for popovers and menus.
    key: string?,
    --- Width.
    w: UiSize?,
    --- Height.
    h: UiSize?,
    --- Minimum width.
    minW: number?,
    --- Minimum height.
    minH: number?,
    --- Maximum width.
    maxW: number?,
    --- Maximum height.
    maxH: number?,
    --- Share of a stack's free space.
    grow: number?,
    --- May shrink below its content size.
    shrink: boolean?,
    --- Padding on every side.
    pad: number?,
    --- Left and right padding.
    px: number?,
    --- Top and bottom padding.
    py: number?,
    --- Top padding.
    pt: number?,
    --- Right padding.
    pr: number?,
    --- Bottom padding.
    pb: number?,
    --- Left padding.
    pl: number?,
    --- Margin on every side.
    m: number?,
    --- Left and right margin.
    mx: number?,
    --- Top and bottom margin.
    my: number?,
    --- Top margin.
    mt: number?,
    --- Right margin.
    mr: number?,
    --- Bottom margin.
    mb: number?,
    --- Left margin.
    ml: number?,
    --- Takes the node out of flow, at these offsets from its parent's edges.
    pin: UiPin?,
    --- 0 to 1.
    opacity: number?,
    --- The accessibility role.
    role: Role?,
    --- The accessible name.
    label: string?,
    --- A tooltip.
    help: string?,
    --- Where the tooltip shows. Defaults to below.
    helpSide: Side?,
    --- Keeps the tooltip showing while true, whatever the pointer does, as Desktop's `Tooltip forceOpen`: a Copy button's "Copied" for a moment after the press.
    helpOpen: boolean?,
    --- Which way the subtree reads, as HTML's `dir` does: `ltr` or `rtl` keeps a figure or a path in that order inside a window running the other way, `auto` takes a text's first strong direction (a name). On `ui.field` it is the value's direction, as on an `input`: `auto` starts an Arabic name at the right, `ltr` keeps an address or a host left to right. Defaults to the window's.
    dir: ("ltr" | "rtl" | "auto")?,
    --- The pointer's shape over the node and over what is inside it that sets none, by its CSS `cursor` name. Desktop shows the arrow on controls; set this only where its style sheet names another cursor, as the browser's address field does (`text`).
    cursor: ("default" | "pointer" | "text" | "grab" | "grabbing" | "not-allowed" | "move" | "col-resize" | "row-resize" | "ew-resize" | "ns-resize" | "nwse-resize" | "nesw-resize" | "crosshair" | "cell" | "zoom-in" | "zoom-out")?,
    --- Space between children.
    gap: number?,
    --- How children sit across the axis.
    align: UiAlign?,
    --- How children sit along the axis.
    justify: UiJustify?,
    --- Centers children on both axes.
    center: boolean?,
    --- Children wrap onto more lines.
    wrap: boolean?,
    --- Background.
    fill: Color?,
    --- Background under the pointer.
    hover: Color?,
    --- Background while pressed.
    press: Color?,
    --- Draws the `surface-selected` fill and reports the selected state to assistive technology.
    selected: boolean?,
    --- Border color.
    border: Color?,
    --- Border width. Defaults to 1 when `border` is set.
    borderWidth: number?,
    --- Corner radius.
    radius: number?,
    --- Per-corner radii: top-left, top-right, bottom-right, bottom-left.
    corners: { number }?,
    --- A shadow from the app's elevation scale.
    shadow: Elevation?,
    --- Clips children to the box.
    clip: boolean?,
    --- Scrolls vertical overflow.
    scrollY: boolean?,
    --- Scrolls horizontal overflow.
    scrollX: boolean?,
    --- With `scrollY`: stays scrolled to the bottom as content grows, as a log or a chat does, until the reader scrolls up.
    stickBottom: boolean?,
    --- Takes keyboard focus.
    focusable: boolean?,
    --- Takes no presses.
    disabled: boolean?,
    --- Runs on a press.
    onPress: UiHandler?,
    --- Its `onPress` opens a menu, as Kobalte's `Menu.Trigger` does: a mouse runs it as the button goes down, and that press's release runs nothing. A key, a screen reader's Invoke or a tap run it on the click. Contract 1.1; a 1.0 host ignores it with one warning and opens on the click.
    menuTrigger: boolean?,
    --- Runs on a middle click, as a browser's `auxclick`: a tab closes, a row opens in the background. A `middle-press` event, with the modifier keys.
    onMiddlePress: UiHandler?,
    --- Runs as the pointer enters and leaves.
    onHover: UiHandler?,
    --- How hover and press fills change.
    hoverTransition: HoverTransition?,
    --- Starts a hover group: descendants with `hoverGroupColor` or `revealOnHover` react while this box is hovered.
    hoverGroup: boolean?,
    --- The text or icon color while the nearest hover group is hovered.
    hoverGroupColor: Color?,
    --- Shows only while the nearest hover group is hovered or focused.
    revealOnHover: boolean?,
    --- The default text and icon color for descendants.
    color: Color?,
    --- Stack direction: `"col"` (default), `"row"` or `"box"`.
    direction: UiDirection?,
    --- Per-side border widths in points: `{ top, right, bottom, left }`. Overrides `borderWidth`.
    borderSides: { number }?,
    --- The key of the element that takes focus when this frame leaves while focus is on it or inside it: one view of a popover handing focus to the control that leads back to it. That element, or its first focusable child; nothing happens when none shows.
    focusReturn: string?,
    --- One Tab stop for the focusable controls inside, which the arrow keys move between, as a roving `tabindex` does: Up and Down with `role` `list`, `listbox`, `menu` or `tree`, Left and Right as well otherwise. Home and End go to the ends; a `list` or `tree` stops at its ends, the others wrap. In a `list`, `listbox`, `menu`, `tree` or `tablist`, typing moves to the first control whose label starts with it. Space presses the focused control, and in a list Return does too.
    focusGroup: boolean?,
    --- Keys `onKey` takes while focus is on the frame or inside it, outside a text field, pressed with no modifier: any of `Enter`, `Escape`, `ArrowUp`, `ArrowDown`, `ArrowLeft`, `ArrowRight`, `Home`, `End`, `PageUp`, `PageDown`, `Delete` and `Backspace`. A key it takes does nothing else there: Return in a `focusGroup` list then no longer presses the focused control.
    keys: { string }?,
    --- Runs as a key `keys` names is pressed, with its name in `event.keyName`.
    onKey: UiHandler?,
}

--- Text styling: `ui.text` and `ui.shimmer`.
export type UiTextProps = {
    --- Identity among siblings across renders, and the anchor key for popovers and menus.
    key: string?,
    --- Width.
    w: UiSize?,
    --- Height.
    h: UiSize?,
    --- Minimum width.
    minW: number?,
    --- Minimum height.
    minH: number?,
    --- Maximum width.
    maxW: number?,
    --- Maximum height.
    maxH: number?,
    --- Share of a stack's free space.
    grow: number?,
    --- May shrink below its content size.
    shrink: boolean?,
    --- Padding on every side.
    pad: number?,
    --- Left and right padding.
    px: number?,
    --- Top and bottom padding.
    py: number?,
    --- Top padding.
    pt: number?,
    --- Right padding.
    pr: number?,
    --- Bottom padding.
    pb: number?,
    --- Left padding.
    pl: number?,
    --- Margin on every side.
    m: number?,
    --- Left and right margin.
    mx: number?,
    --- Top and bottom margin.
    my: number?,
    --- Top margin.
    mt: number?,
    --- Right margin.
    mr: number?,
    --- Bottom margin.
    mb: number?,
    --- Left margin.
    ml: number?,
    --- Takes the node out of flow, at these offsets from its parent's edges.
    pin: UiPin?,
    --- 0 to 1.
    opacity: number?,
    --- The accessibility role.
    role: Role?,
    --- The accessible name.
    label: string?,
    --- A tooltip.
    help: string?,
    --- Where the tooltip shows. Defaults to below.
    helpSide: Side?,
    --- Keeps the tooltip showing while true, whatever the pointer does, as Desktop's `Tooltip forceOpen`: a Copy button's "Copied" for a moment after the press.
    helpOpen: boolean?,
    --- Which way the subtree reads, as HTML's `dir` does: `ltr` or `rtl` keeps a figure or a path in that order inside a window running the other way, `auto` takes a text's first strong direction (a name). On `ui.field` it is the value's direction, as on an `input`: `auto` starts an Arabic name at the right, `ltr` keeps an address or a host left to right. Defaults to the window's.
    dir: ("ltr" | "rtl" | "auto")?,
    --- The pointer's shape over the node and over what is inside it that sets none, by its CSS `cursor` name. Desktop shows the arrow on controls; set this only where its style sheet names another cursor, as the browser's address field does (`text`).
    cursor: ("default" | "pointer" | "text" | "grab" | "grabbing" | "not-allowed" | "move" | "col-resize" | "row-resize" | "ew-resize" | "ns-resize" | "nwse-resize" | "nesw-resize" | "crosshair" | "cell" | "zoom-in" | "zoom-out")?,
    --- Font size in points. Defaults to 13.
    size: number?,
    --- Weight.
    weight: FontWeight?,
    --- Line height in points.
    lineHeight: number?,
    --- Letter spacing in points.
    tracking: number?,
    --- Text color. Defaults to `text-base`.
    color: Color?,
    --- The code font.
    mono: boolean?,
    --- Tabular figures.
    tabular: boolean?,
    --- Wraps onto more lines. The default for text in a column.
    wrap: boolean?,
    --- Keeps to one line.
    noWrap: boolean?,
    --- Ends with an ellipsis when it does not fit.
    truncate: boolean?,
    --- Fades out at the end when it does not fit, instead of an ellipsis.
    fadeOut: boolean?,
    --- The user can select and copy it.
    selectable: boolean?,
    --- Alignment within its box.
    textAlign: Align?,
    --- The color while the nearest hover group is hovered.
    hoverGroupColor: Color?,
    --- Shows only while the nearest hover group is hovered.
    revealOnHover: boolean?,
}

--- `ui.spacer` props.
export type UiSpacerProps = {
    --- Identity among siblings across renders, and the anchor key for popovers and menus.
    key: string?,
    --- Width.
    w: UiSize?,
    --- Height.
    h: UiSize?,
    --- Minimum width.
    minW: number?,
    --- Minimum height.
    minH: number?,
    --- Maximum width.
    maxW: number?,
    --- Maximum height.
    maxH: number?,
    --- Share of a stack's free space.
    grow: number?,
    --- May shrink below its content size.
    shrink: boolean?,
    --- Padding on every side.
    pad: number?,
    --- Left and right padding.
    px: number?,
    --- Top and bottom padding.
    py: number?,
    --- Top padding.
    pt: number?,
    --- Right padding.
    pr: number?,
    --- Bottom padding.
    pb: number?,
    --- Left padding.
    pl: number?,
    --- Margin on every side.
    m: number?,
    --- Left and right margin.
    mx: number?,
    --- Top and bottom margin.
    my: number?,
    --- Top margin.
    mt: number?,
    --- Right margin.
    mr: number?,
    --- Bottom margin.
    mb: number?,
    --- Left margin.
    ml: number?,
    --- Takes the node out of flow, at these offsets from its parent's edges.
    pin: UiPin?,
    --- 0 to 1.
    opacity: number?,
    --- The accessibility role.
    role: Role?,
    --- The accessible name.
    label: string?,
    --- A tooltip.
    help: string?,
    --- Where the tooltip shows. Defaults to below.
    helpSide: Side?,
    --- Keeps the tooltip showing while true, whatever the pointer does, as Desktop's `Tooltip forceOpen`: a Copy button's "Copied" for a moment after the press.
    helpOpen: boolean?,
    --- Which way the subtree reads, as HTML's `dir` does: `ltr` or `rtl` keeps a figure or a path in that order inside a window running the other way, `auto` takes a text's first strong direction (a name). On `ui.field` it is the value's direction, as on an `input`: `auto` starts an Arabic name at the right, `ltr` keeps an address or a host left to right. Defaults to the window's.
    dir: ("ltr" | "rtl" | "auto")?,
    --- The pointer's shape over the node and over what is inside it that sets none, by its CSS `cursor` name. Desktop shows the arrow on controls; set this only where its style sheet names another cursor, as the browser's address field does (`text`).
    cursor: ("default" | "pointer" | "text" | "grab" | "grabbing" | "not-allowed" | "move" | "col-resize" | "row-resize" | "ew-resize" | "ns-resize" | "nwse-resize" | "nesw-resize" | "crosshair" | "cell" | "zoom-in" | "zoom-out")?,
}

--- `ui.divider` props.
export type UiDividerProps = {
    --- Identity among siblings across renders, and the anchor key for popovers and menus.
    key: string?,
    --- Width.
    w: UiSize?,
    --- Height.
    h: UiSize?,
    --- Minimum width.
    minW: number?,
    --- Minimum height.
    minH: number?,
    --- Maximum width.
    maxW: number?,
    --- Maximum height.
    maxH: number?,
    --- Share of a stack's free space.
    grow: number?,
    --- May shrink below its content size.
    shrink: boolean?,
    --- Padding on every side.
    pad: number?,
    --- Left and right padding.
    px: number?,
    --- Top and bottom padding.
    py: number?,
    --- Top padding.
    pt: number?,
    --- Right padding.
    pr: number?,
    --- Bottom padding.
    pb: number?,
    --- Left padding.
    pl: number?,
    --- Margin on every side.
    m: number?,
    --- Left and right margin.
    mx: number?,
    --- Top and bottom margin.
    my: number?,
    --- Top margin.
    mt: number?,
    --- Right margin.
    mr: number?,
    --- Bottom margin.
    mb: number?,
    --- Left margin.
    ml: number?,
    --- Takes the node out of flow, at these offsets from its parent's edges.
    pin: UiPin?,
    --- 0 to 1.
    opacity: number?,
    --- The accessibility role.
    role: Role?,
    --- The accessible name.
    label: string?,
    --- A tooltip.
    help: string?,
    --- Where the tooltip shows. Defaults to below.
    helpSide: Side?,
    --- Keeps the tooltip showing while true, whatever the pointer does, as Desktop's `Tooltip forceOpen`: a Copy button's "Copied" for a moment after the press.
    helpOpen: boolean?,
    --- Which way the subtree reads, as HTML's `dir` does: `ltr` or `rtl` keeps a figure or a path in that order inside a window running the other way, `auto` takes a text's first strong direction (a name). On `ui.field` it is the value's direction, as on an `input`: `auto` starts an Arabic name at the right, `ltr` keeps an address or a host left to right. Defaults to the window's.
    dir: ("ltr" | "rtl" | "auto")?,
    --- The pointer's shape over the node and over what is inside it that sets none, by its CSS `cursor` name. Desktop shows the arrow on controls; set this only where its style sheet names another cursor, as the browser's address field does (`text`).
    cursor: ("default" | "pointer" | "text" | "grab" | "grabbing" | "not-allowed" | "move" | "col-resize" | "row-resize" | "ew-resize" | "ns-resize" | "nwse-resize" | "nesw-resize" | "crosshair" | "cell" | "zoom-in" | "zoom-out")?,
    --- A vertical rule. Defaults to horizontal.
    vertical: boolean?,
    --- Defaults to `border-weak`.
    color: Color?,
}

--- `ui.icon` props.
export type UiIconProps = {
    --- Identity among siblings across renders, and the anchor key for popovers and menus.
    key: string?,
    --- Width.
    w: UiSize?,
    --- Height.
    h: UiSize?,
    --- Minimum width.
    minW: number?,
    --- Minimum height.
    minH: number?,
    --- Maximum width.
    maxW: number?,
    --- Maximum height.
    maxH: number?,
    --- Share of a stack's free space.
    grow: number?,
    --- May shrink below its content size.
    shrink: boolean?,
    --- Padding on every side.
    pad: number?,
    --- Left and right padding.
    px: number?,
    --- Top and bottom padding.
    py: number?,
    --- Top padding.
    pt: number?,
    --- Right padding.
    pr: number?,
    --- Bottom padding.
    pb: number?,
    --- Left padding.
    pl: number?,
    --- Margin on every side.
    m: number?,
    --- Left and right margin.
    mx: number?,
    --- Top and bottom margin.
    my: number?,
    --- Top margin.
    mt: number?,
    --- Right margin.
    mr: number?,
    --- Bottom margin.
    mb: number?,
    --- Left margin.
    ml: number?,
    --- Takes the node out of flow, at these offsets from its parent's edges.
    pin: UiPin?,
    --- 0 to 1.
    opacity: number?,
    --- The accessibility role.
    role: Role?,
    --- The accessible name.
    label: string?,
    --- A tooltip.
    help: string?,
    --- Where the tooltip shows. Defaults to below.
    helpSide: Side?,
    --- Keeps the tooltip showing while true, whatever the pointer does, as Desktop's `Tooltip forceOpen`: a Copy button's "Copied" for a moment after the press.
    helpOpen: boolean?,
    --- Which way the subtree reads, as HTML's `dir` does: `ltr` or `rtl` keeps a figure or a path in that order inside a window running the other way, `auto` takes a text's first strong direction (a name). On `ui.field` it is the value's direction, as on an `input`: `auto` starts an Arabic name at the right, `ltr` keeps an address or a host left to right. Defaults to the window's.
    dir: ("ltr" | "rtl" | "auto")?,
    --- The pointer's shape over the node and over what is inside it that sets none, by its CSS `cursor` name. Desktop shows the arrow on controls; set this only where its style sheet names another cursor, as the browser's address field does (`text`).
    cursor: ("default" | "pointer" | "text" | "grab" | "grabbing" | "not-allowed" | "move" | "col-resize" | "row-resize" | "ew-resize" | "ns-resize" | "nwse-resize" | "nesw-resize" | "crosshair" | "cell" | "zoom-in" | "zoom-out")?,
    --- Width and height in points. Defaults to 16.
    size: number?,
    --- Defaults to `icon-base`.
    color: Color?,
    --- A status color, instead of `color`.
    tone: Tone?,
    --- The color while the nearest hover group is hovered.
    hoverGroupColor: Color?,
    --- Shows only while the nearest hover group is hovered.
    revealOnHover: boolean?,
}

--- `ui.image` props. Give it a size (`w`, `h`); without one it takes the image's own size in points once it has loaded. Inside a `view:Memo`, put `Images:State(image).status` in the memo's version, so the memo rebuilds as the image lands or fails.
export type UiImageProps = {
    --- Identity among siblings across renders, and the anchor key for popovers and menus.
    key: string?,
    --- Width.
    w: UiSize?,
    --- Height.
    h: UiSize?,
    --- Minimum width.
    minW: number?,
    --- Minimum height.
    minH: number?,
    --- Maximum width.
    maxW: number?,
    --- Maximum height.
    maxH: number?,
    --- Share of a stack's free space.
    grow: number?,
    --- May shrink below its content size.
    shrink: boolean?,
    --- Padding on every side.
    pad: number?,
    --- Left and right padding.
    px: number?,
    --- Top and bottom padding.
    py: number?,
    --- Top padding.
    pt: number?,
    --- Right padding.
    pr: number?,
    --- Bottom padding.
    pb: number?,
    --- Left padding.
    pl: number?,
    --- Margin on every side.
    m: number?,
    --- Left and right margin.
    mx: number?,
    --- Top and bottom margin.
    my: number?,
    --- Top margin.
    mt: number?,
    --- Right margin.
    mr: number?,
    --- Bottom margin.
    mb: number?,
    --- Left margin.
    ml: number?,
    --- Takes the node out of flow, at these offsets from its parent's edges.
    pin: UiPin?,
    --- 0 to 1.
    opacity: number?,
    --- The accessibility role.
    role: Role?,
    --- The accessible name.
    label: string?,
    --- A tooltip.
    help: string?,
    --- Where the tooltip shows. Defaults to below.
    helpSide: Side?,
    --- Keeps the tooltip showing while true, whatever the pointer does, as Desktop's `Tooltip forceOpen`: a Copy button's "Copied" for a moment after the press.
    helpOpen: boolean?,
    --- Which way the subtree reads, as HTML's `dir` does: `ltr` or `rtl` keeps a figure or a path in that order inside a window running the other way, `auto` takes a text's first strong direction (a name). On `ui.field` it is the value's direction, as on an `input`: `auto` starts an Arabic name at the right, `ltr` keeps an address or a host left to right. Defaults to the window's.
    dir: ("ltr" | "rtl" | "auto")?,
    --- The pointer's shape over the node and over what is inside it that sets none, by its CSS `cursor` name. Desktop shows the arrow on controls; set this only where its style sheet names another cursor, as the browser's address field does (`text`).
    cursor: ("default" | "pointer" | "text" | "grab" | "grabbing" | "not-allowed" | "move" | "col-resize" | "row-resize" | "ew-resize" | "ns-resize" | "nwse-resize" | "nesw-resize" | "crosshair" | "cell" | "zoom-in" | "zoom-out")?,
    --- The image, from the `Images` service or an `Async` render.
    image: ImageId?,
    --- Instead of `image`: the source `Images:Load` takes (a URL, a server path, a data URL or a file of the plugin's folder).
    src: string?,
    --- How it fills its box.
    fit: ImageFit?,
    --- Corner radius.
    radius: number?,
    --- The fill under the image: shown while it loads and through its transparent pixels.
    placeholder: Color?,
    --- Accepted for compatibility. Loaded images appear immediately.
    fadeIn: boolean?,
    --- A press opens it in the app's image viewer, which grows it out of its place, with zoom; Escape flies it back. Give it a `label`: the viewer's name for it.
    preview: boolean?,
}

--- `ui.video` props. Give it a size.
export type UiVideoProps = {
    --- Identity among siblings across renders, and the anchor key for popovers and menus.
    key: string?,
    --- Width.
    w: UiSize?,
    --- Height.
    h: UiSize?,
    --- Minimum width.
    minW: number?,
    --- Minimum height.
    minH: number?,
    --- Maximum width.
    maxW: number?,
    --- Maximum height.
    maxH: number?,
    --- Share of a stack's free space.
    grow: number?,
    --- May shrink below its content size.
    shrink: boolean?,
    --- Padding on every side.
    pad: number?,
    --- Left and right padding.
    px: number?,
    --- Top and bottom padding.
    py: number?,
    --- Top padding.
    pt: number?,
    --- Right padding.
    pr: number?,
    --- Bottom padding.
    pb: number?,
    --- Left padding.
    pl: number?,
    --- Margin on every side.
    m: number?,
    --- Left and right margin.
    mx: number?,
    --- Top and bottom margin.
    my: number?,
    --- Top margin.
    mt: number?,
    --- Right margin.
    mr: number?,
    --- Bottom margin.
    mb: number?,
    --- Left margin.
    ml: number?,
    --- Takes the node out of flow, at these offsets from its parent's edges.
    pin: UiPin?,
    --- 0 to 1.
    opacity: number?,
    --- The accessibility role.
    role: Role?,
    --- The accessible name.
    label: string?,
    --- A tooltip.
    help: string?,
    --- Where the tooltip shows. Defaults to below.
    helpSide: Side?,
    --- Keeps the tooltip showing while true, whatever the pointer does, as Desktop's `Tooltip forceOpen`: a Copy button's "Copied" for a moment after the press.
    helpOpen: boolean?,
    --- Which way the subtree reads, as HTML's `dir` does: `ltr` or `rtl` keeps a figure or a path in that order inside a window running the other way, `auto` takes a text's first strong direction (a name). On `ui.field` it is the value's direction, as on an `input`: `auto` starts an Arabic name at the right, `ltr` keeps an address or a host left to right. Defaults to the window's.
    dir: ("ltr" | "rtl" | "auto")?,
    --- The pointer's shape over the node and over what is inside it that sets none, by its CSS `cursor` name. Desktop shows the arrow on controls; set this only where its style sheet names another cursor, as the browser's address field does (`text`).
    cursor: ("default" | "pointer" | "text" | "grab" | "grabbing" | "not-allowed" | "move" | "col-resize" | "row-resize" | "ew-resize" | "ns-resize" | "nwse-resize" | "nesw-resize" | "crosshair" | "cell" | "zoom-in" | "zoom-out")?,
    --- An `https:` or `http:` URL of the video.
    src: string,
    --- How it fills its box: `contain` letterboxes it, `cover` crops it.
    fit: ImageFit?,
    --- Corner radius.
    radius: number?,
}

--- `ui.metric` props: a number shown as text.
export type UiMetricProps = {
    --- Identity among siblings across renders, and the anchor key for popovers and menus.
    key: string?,
    --- Width.
    w: UiSize?,
    --- Height.
    h: UiSize?,
    --- Minimum width.
    minW: number?,
    --- Minimum height.
    minH: number?,
    --- Maximum width.
    maxW: number?,
    --- Maximum height.
    maxH: number?,
    --- Share of a stack's free space.
    grow: number?,
    --- May shrink below its content size.
    shrink: boolean?,
    --- Padding on every side.
    pad: number?,
    --- Left and right padding.
    px: number?,
    --- Top and bottom padding.
    py: number?,
    --- Top padding.
    pt: number?,
    --- Right padding.
    pr: number?,
    --- Bottom padding.
    pb: number?,
    --- Left padding.
    pl: number?,
    --- Margin on every side.
    m: number?,
    --- Left and right margin.
    mx: number?,
    --- Top and bottom margin.
    my: number?,
    --- Top margin.
    mt: number?,
    --- Right margin.
    mr: number?,
    --- Bottom margin.
    mb: number?,
    --- Left margin.
    ml: number?,
    --- Takes the node out of flow, at these offsets from its parent's edges.
    pin: UiPin?,
    --- 0 to 1.
    opacity: number?,
    --- The accessibility role.
    role: Role?,
    --- The accessible name.
    label: string?,
    --- A tooltip.
    help: string?,
    --- Where the tooltip shows. Defaults to below.
    helpSide: Side?,
    --- Keeps the tooltip showing while true, whatever the pointer does, as Desktop's `Tooltip forceOpen`: a Copy button's "Copied" for a moment after the press.
    helpOpen: boolean?,
    --- Which way the subtree reads, as HTML's `dir` does: `ltr` or `rtl` keeps a figure or a path in that order inside a window running the other way, `auto` takes a text's first strong direction (a name). On `ui.field` it is the value's direction, as on an `input`: `auto` starts an Arabic name at the right, `ltr` keeps an address or a host left to right. Defaults to the window's.
    dir: ("ltr" | "rtl" | "auto")?,
    --- The pointer's shape over the node and over what is inside it that sets none, by its CSS `cursor` name. Desktop shows the arrow on controls; set this only where its style sheet names another cursor, as the browser's address field does (`text`).
    cursor: ("default" | "pointer" | "text" | "grab" | "grabbing" | "not-allowed" | "move" | "col-resize" | "row-resize" | "ew-resize" | "ns-resize" | "nwse-resize" | "nesw-resize" | "crosshair" | "cell" | "zoom-in" | "zoom-out")?,
    --- The number the text stands for. Hosts that animate compare it with the last value.
    value: number,
    --- What it reads, such as `"12.4K"`.
    text: string,
    --- Tabular figures. Defaults to true.
    tabular: boolean?,
    --- Animates changed digits where the host can. Defaults to true. Other hosts show the text.
    animate: boolean?,
    --- Font size in points.
    size: number?,
    --- Weight.
    weight: FontWeight?,
    --- Line height in points.
    lineHeight: number?,
    --- Letter spacing in points.
    tracking: number?,
    --- Color.
    color: Color?,
}

--- `ui.button` props: the app's own button.
export type UiButtonProps = {
    --- Identity among siblings across renders, and the anchor key for popovers and menus.
    key: string?,
    --- Width.
    w: UiSize?,
    --- Height.
    h: UiSize?,
    --- Minimum width.
    minW: number?,
    --- Minimum height.
    minH: number?,
    --- Maximum width.
    maxW: number?,
    --- Maximum height.
    maxH: number?,
    --- Share of a stack's free space.
    grow: number?,
    --- May shrink below its content size.
    shrink: boolean?,
    --- Padding on every side.
    pad: number?,
    --- Left and right padding.
    px: number?,
    --- Top and bottom padding.
    py: number?,
    --- Top padding.
    pt: number?,
    --- Right padding.
    pr: number?,
    --- Bottom padding.
    pb: number?,
    --- Left padding.
    pl: number?,
    --- Margin on every side.
    m: number?,
    --- Left and right margin.
    mx: number?,
    --- Top and bottom margin.
    my: number?,
    --- Top margin.
    mt: number?,
    --- Right margin.
    mr: number?,
    --- Bottom margin.
    mb: number?,
    --- Left margin.
    ml: number?,
    --- Takes the node out of flow, at these offsets from its parent's edges.
    pin: UiPin?,
    --- 0 to 1.
    opacity: number?,
    --- The accessibility role.
    role: Role?,
    --- The accessible name.
    label: string?,
    --- A tooltip.
    help: string?,
    --- Where the tooltip shows. Defaults to below.
    helpSide: Side?,
    --- Keeps the tooltip showing while true, whatever the pointer does, as Desktop's `Tooltip forceOpen`: a Copy button's "Copied" for a moment after the press.
    helpOpen: boolean?,
    --- Which way the subtree reads, as HTML's `dir` does: `ltr` or `rtl` keeps a figure or a path in that order inside a window running the other way, `auto` takes a text's first strong direction (a name). On `ui.field` it is the value's direction, as on an `input`: `auto` starts an Arabic name at the right, `ltr` keeps an address or a host left to right. Defaults to the window's.
    dir: ("ltr" | "rtl" | "auto")?,
    --- The pointer's shape over the node and over what is inside it that sets none, by its CSS `cursor` name. Desktop shows the arrow on controls; set this only where its style sheet names another cursor, as the browser's address field does (`text`).
    cursor: ("default" | "pointer" | "text" | "grab" | "grabbing" | "not-allowed" | "move" | "col-resize" | "row-resize" | "ew-resize" | "ns-resize" | "nwse-resize" | "nesw-resize" | "crosshair" | "cell" | "zoom-in" | "zoom-out")?,
    --- The visible label. Also the accessible name unless `label` is set.
    text: string?,
    --- A leading icon.
    icon: IconName?,
    --- The look.
    variant: ButtonVariant?,
    --- The size.
    size: ControlSize?,
    --- Takes no presses.
    disabled: boolean?,
    --- Shows the app's loader and takes no presses.
    loading: boolean?,
    --- A toggle button's pressed state.
    pressed: boolean?,
    --- Runs on a press.
    onPress: UiHandler?,
    --- Its `onPress` opens a menu, as Kobalte's `Menu.Trigger` does: a mouse runs it as the button goes down. A key, a screen reader's Invoke or a tap run it on the click. Contract 1.1.
    menuTrigger: boolean?,
    --- Takes focus as the dialog it is in opens, as HTML's `autofocus` in a dialog: a confirmation's Cancel, say. Without one the dialog focuses its first text field or Tab stop.
    autofocus: boolean?,
    --- The key of the element that takes focus when this one leaves while it has focus, as a page's `element.focus()` after a view changes: a drill-in's Back hands focus to the row that opened it. That element, or its first focusable child; nothing happens when none shows.
    focusReturn: string?,
}

--- `ui.iconButton` props: a square button with an icon. Give it a `label`.
export type UiIconButtonProps = {
    --- Identity among siblings across renders, and the anchor key for popovers and menus.
    key: string?,
    --- Width.
    w: UiSize?,
    --- Height.
    h: UiSize?,
    --- Minimum width.
    minW: number?,
    --- Minimum height.
    minH: number?,
    --- Maximum width.
    maxW: number?,
    --- Maximum height.
    maxH: number?,
    --- Share of a stack's free space.
    grow: number?,
    --- May shrink below its content size.
    shrink: boolean?,
    --- Padding on every side.
    pad: number?,
    --- Left and right padding.
    px: number?,
    --- Top and bottom padding.
    py: number?,
    --- Top padding.
    pt: number?,
    --- Right padding.
    pr: number?,
    --- Bottom padding.
    pb: number?,
    --- Left padding.
    pl: number?,
    --- Margin on every side.
    m: number?,
    --- Left and right margin.
    mx: number?,
    --- Top and bottom margin.
    my: number?,
    --- Top margin.
    mt: number?,
    --- Right margin.
    mr: number?,
    --- Bottom margin.
    mb: number?,
    --- Left margin.
    ml: number?,
    --- Takes the node out of flow, at these offsets from its parent's edges.
    pin: UiPin?,
    --- 0 to 1.
    opacity: number?,
    --- The accessibility role.
    role: Role?,
    --- The accessible name.
    label: string?,
    --- A tooltip.
    help: string?,
    --- Where the tooltip shows. Defaults to below.
    helpSide: Side?,
    --- Keeps the tooltip showing while true, whatever the pointer does, as Desktop's `Tooltip forceOpen`: a Copy button's "Copied" for a moment after the press.
    helpOpen: boolean?,
    --- Which way the subtree reads, as HTML's `dir` does: `ltr` or `rtl` keeps a figure or a path in that order inside a window running the other way, `auto` takes a text's first strong direction (a name). On `ui.field` it is the value's direction, as on an `input`: `auto` starts an Arabic name at the right, `ltr` keeps an address or a host left to right. Defaults to the window's.
    dir: ("ltr" | "rtl" | "auto")?,
    --- The pointer's shape over the node and over what is inside it that sets none, by its CSS `cursor` name. Desktop shows the arrow on controls; set this only where its style sheet names another cursor, as the browser's address field does (`text`).
    cursor: ("default" | "pointer" | "text" | "grab" | "grabbing" | "not-allowed" | "move" | "col-resize" | "row-resize" | "ew-resize" | "ns-resize" | "nwse-resize" | "nesw-resize" | "crosshair" | "cell" | "zoom-in" | "zoom-out")?,
    --- The icon.
    icon: IconName,
    --- The look. Defaults to `ghost`.
    variant: ButtonVariant?,
    --- The size.
    size: ControlSize?,
    --- Takes no presses.
    disabled: boolean?,
    --- A toggle button's pressed state.
    pressed: boolean?,
    --- The icon's color, instead of the variant's.
    color: Color?,
    --- A published command id whose shortcut its tooltip shows after the `help` text, as keycaps.
    keybind: string?,
    --- Runs on a press.
    onPress: UiHandler?,
    --- Its `onPress` opens a menu, as Kobalte's `Menu.Trigger` does: a mouse runs it as the button goes down. A key, a screen reader's Invoke or a tap run it on the click. Contract 1.1.
    menuTrigger: boolean?,
    --- Takes focus as the dialog it is in opens, as HTML's `autofocus` in a dialog: a confirmation's Cancel, say. Without one the dialog focuses its first text field or Tab stop.
    autofocus: boolean?,
    --- The key of the element that takes focus when this one leaves while it has focus, as a page's `element.focus()` after a view changes: a drill-in's Back hands focus to the row that opened it. That element, or its first focusable child; nothing happens when none shows.
    focusReturn: string?,
}

--- `ui.switch` props: the app's own switch.
export type UiSwitchProps = {
    --- Identity among siblings across renders, and the anchor key for popovers and menus.
    key: string?,
    --- Width.
    w: UiSize?,
    --- Height.
    h: UiSize?,
    --- Minimum width.
    minW: number?,
    --- Minimum height.
    minH: number?,
    --- Maximum width.
    maxW: number?,
    --- Maximum height.
    maxH: number?,
    --- Share of a stack's free space.
    grow: number?,
    --- May shrink below its content size.
    shrink: boolean?,
    --- Padding on every side.
    pad: number?,
    --- Left and right padding.
    px: number?,
    --- Top and bottom padding.
    py: number?,
    --- Top padding.
    pt: number?,
    --- Right padding.
    pr: number?,
    --- Bottom padding.
    pb: number?,
    --- Left padding.
    pl: number?,
    --- Margin on every side.
    m: number?,
    --- Left and right margin.
    mx: number?,
    --- Top and bottom margin.
    my: number?,
    --- Top margin.
    mt: number?,
    --- Right margin.
    mr: number?,
    --- Bottom margin.
    mb: number?,
    --- Left margin.
    ml: number?,
    --- Takes the node out of flow, at these offsets from its parent's edges.
    pin: UiPin?,
    --- 0 to 1.
    opacity: number?,
    --- The accessibility role.
    role: Role?,
    --- The accessible name.
    label: string?,
    --- A tooltip.
    help: string?,
    --- Where the tooltip shows. Defaults to below.
    helpSide: Side?,
    --- Keeps the tooltip showing while true, whatever the pointer does, as Desktop's `Tooltip forceOpen`: a Copy button's "Copied" for a moment after the press.
    helpOpen: boolean?,
    --- Which way the subtree reads, as HTML's `dir` does: `ltr` or `rtl` keeps a figure or a path in that order inside a window running the other way, `auto` takes a text's first strong direction (a name). On `ui.field` it is the value's direction, as on an `input`: `auto` starts an Arabic name at the right, `ltr` keeps an address or a host left to right. Defaults to the window's.
    dir: ("ltr" | "rtl" | "auto")?,
    --- The pointer's shape over the node and over what is inside it that sets none, by its CSS `cursor` name. Desktop shows the arrow on controls; set this only where its style sheet names another cursor, as the browser's address field does (`text`).
    cursor: ("default" | "pointer" | "text" | "grab" | "grabbing" | "not-allowed" | "move" | "col-resize" | "row-resize" | "ew-resize" | "ns-resize" | "nwse-resize" | "nesw-resize" | "crosshair" | "cell" | "zoom-in" | "zoom-out")?,
    --- On.
    checked: boolean,
    --- Cannot change.
    disabled: boolean?,
    --- Runs with the new state in `event.checked`.
    onChange: UiHandler?,
}

--- `ui.checkbox` props: Desktop's compact checkbox (`Checkbox appearance="compact"`) with its label.
export type UiCheckboxProps = {
    --- Identity among siblings across renders, and the anchor key for popovers and menus.
    key: string?,
    --- Width.
    w: UiSize?,
    --- Height.
    h: UiSize?,
    --- Minimum width.
    minW: number?,
    --- Minimum height.
    minH: number?,
    --- Maximum width.
    maxW: number?,
    --- Maximum height.
    maxH: number?,
    --- Share of a stack's free space.
    grow: number?,
    --- May shrink below its content size.
    shrink: boolean?,
    --- Padding on every side.
    pad: number?,
    --- Left and right padding.
    px: number?,
    --- Top and bottom padding.
    py: number?,
    --- Top padding.
    pt: number?,
    --- Right padding.
    pr: number?,
    --- Bottom padding.
    pb: number?,
    --- Left padding.
    pl: number?,
    --- Margin on every side.
    m: number?,
    --- Left and right margin.
    mx: number?,
    --- Top and bottom margin.
    my: number?,
    --- Top margin.
    mt: number?,
    --- Right margin.
    mr: number?,
    --- Bottom margin.
    mb: number?,
    --- Left margin.
    ml: number?,
    --- Takes the node out of flow, at these offsets from its parent's edges.
    pin: UiPin?,
    --- 0 to 1.
    opacity: number?,
    --- The accessibility role.
    role: Role?,
    --- The accessible name.
    label: string?,
    --- A tooltip.
    help: string?,
    --- Where the tooltip shows. Defaults to below.
    helpSide: Side?,
    --- Keeps the tooltip showing while true, whatever the pointer does, as Desktop's `Tooltip forceOpen`: a Copy button's "Copied" for a moment after the press.
    helpOpen: boolean?,
    --- Which way the subtree reads, as HTML's `dir` does: `ltr` or `rtl` keeps a figure or a path in that order inside a window running the other way, `auto` takes a text's first strong direction (a name). On `ui.field` it is the value's direction, as on an `input`: `auto` starts an Arabic name at the right, `ltr` keeps an address or a host left to right. Defaults to the window's.
    dir: ("ltr" | "rtl" | "auto")?,
    --- The pointer's shape over the node and over what is inside it that sets none, by its CSS `cursor` name. Desktop shows the arrow on controls; set this only where its style sheet names another cursor, as the browser's address field does (`text`).
    cursor: ("default" | "pointer" | "text" | "grab" | "grabbing" | "not-allowed" | "move" | "col-resize" | "row-resize" | "ew-resize" | "ns-resize" | "nwse-resize" | "nesw-resize" | "crosshair" | "cell" | "zoom-in" | "zoom-out")?,
    --- The label, beside the box.
    text: string,
    --- A line under the label.
    description: string?,
    --- Ticked.
    checked: boolean,
    --- Some of what it stands for is ticked: the box shows a dash, as Desktop's `indeterminate`. A press still reports the opposite of `checked`.
    indeterminate: boolean?,
    --- The background the box sits on: `base` (the default) or `layer-1`, as Desktop's `surface`; an unticked box takes its fill.
    surface: ("base" | "layer-1")?,
    --- Draws only the checkbox box visually with no role, focus or click handler, for use inside an `option` row (Desktop's `Checkbox presentation`).
    presentation: boolean?,
    --- Cannot change; drawn at half opacity.
    disabled: boolean?,
    --- Runs with the new state in `event.checked`.
    onChange: UiHandler?,
}

--- `ui.radio` props: a radio row with its label and description.
export type UiRadioProps = {
    --- Identity among siblings across renders, and the anchor key for popovers and menus.
    key: string?,
    --- Width.
    w: UiSize?,
    --- Height.
    h: UiSize?,
    --- Minimum width.
    minW: number?,
    --- Minimum height.
    minH: number?,
    --- Maximum width.
    maxW: number?,
    --- Maximum height.
    maxH: number?,
    --- Share of a stack's free space.
    grow: number?,
    --- May shrink below its content size.
    shrink: boolean?,
    --- Padding on every side.
    pad: number?,
    --- Left and right padding.
    px: number?,
    --- Top and bottom padding.
    py: number?,
    --- Top padding.
    pt: number?,
    --- Right padding.
    pr: number?,
    --- Bottom padding.
    pb: number?,
    --- Left padding.
    pl: number?,
    --- Margin on every side.
    m: number?,
    --- Left and right margin.
    mx: number?,
    --- Top and bottom margin.
    my: number?,
    --- Top margin.
    mt: number?,
    --- Right margin.
    mr: number?,
    --- Bottom margin.
    mb: number?,
    --- Left margin.
    ml: number?,
    --- Takes the node out of flow, at these offsets from its parent's edges.
    pin: UiPin?,
    --- 0 to 1.
    opacity: number?,
    --- The accessibility role.
    role: Role?,
    --- The accessible name.
    label: string?,
    --- A tooltip.
    help: string?,
    --- Where the tooltip shows. Defaults to below.
    helpSide: Side?,
    --- Keeps the tooltip showing while true, whatever the pointer does, as Desktop's `Tooltip forceOpen`: a Copy button's "Copied" for a moment after the press.
    helpOpen: boolean?,
    --- Which way the subtree reads, as HTML's `dir` does: `ltr` or `rtl` keeps a figure or a path in that order inside a window running the other way, `auto` takes a text's first strong direction (a name). On `ui.field` it is the value's direction, as on an `input`: `auto` starts an Arabic name at the right, `ltr` keeps an address or a host left to right. Defaults to the window's.
    dir: ("ltr" | "rtl" | "auto")?,
    --- The pointer's shape over the node and over what is inside it that sets none, by its CSS `cursor` name. Desktop shows the arrow on controls; set this only where its style sheet names another cursor, as the browser's address field does (`text`).
    cursor: ("default" | "pointer" | "text" | "grab" | "grabbing" | "not-allowed" | "move" | "col-resize" | "row-resize" | "ew-resize" | "ns-resize" | "nwse-resize" | "nesw-resize" | "crosshair" | "cell" | "zoom-in" | "zoom-out")?,
    --- The row's label.
    text: string,
    --- A line under the label.
    description: string?,
    --- Selected.
    checked: boolean,
    --- Colors the description, as a status line.
    tone: Tone?,
    --- The label's line height: 20 by default, as Desktop's radio label; 16 for a list row whose label sets its own.
    lineHeight: number?,
    --- Cannot change.
    disabled: boolean?,
    --- Runs as the user picks the row.
    onChange: UiHandler?,
}

--- `ui.field` props: the app's own text field over an `Editor`.
export type UiFieldProps = {
    --- Identity among siblings across renders, and the anchor key for popovers and menus.
    key: string?,
    --- Width.
    w: UiSize?,
    --- Height.
    h: UiSize?,
    --- Minimum width.
    minW: number?,
    --- Minimum height.
    minH: number?,
    --- Maximum width.
    maxW: number?,
    --- Maximum height.
    maxH: number?,
    --- Share of a stack's free space.
    grow: number?,
    --- May shrink below its content size.
    shrink: boolean?,
    --- Padding on every side.
    pad: number?,
    --- Left and right padding.
    px: number?,
    --- Top and bottom padding.
    py: number?,
    --- Top padding.
    pt: number?,
    --- Right padding.
    pr: number?,
    --- Bottom padding.
    pb: number?,
    --- Left padding.
    pl: number?,
    --- Margin on every side.
    m: number?,
    --- Left and right margin.
    mx: number?,
    --- Top and bottom margin.
    my: number?,
    --- Top margin.
    mt: number?,
    --- Right margin.
    mr: number?,
    --- Bottom margin.
    mb: number?,
    --- Left margin.
    ml: number?,
    --- Takes the node out of flow, at these offsets from its parent's edges.
    pin: UiPin?,
    --- 0 to 1.
    opacity: number?,
    --- The accessibility role.
    role: Role?,
    --- The accessible name.
    label: string?,
    --- A tooltip.
    help: string?,
    --- Where the tooltip shows. Defaults to below.
    helpSide: Side?,
    --- Keeps the tooltip showing while true, whatever the pointer does, as Desktop's `Tooltip forceOpen`: a Copy button's "Copied" for a moment after the press.
    helpOpen: boolean?,
    --- Which way the subtree reads, as HTML's `dir` does: `ltr` or `rtl` keeps a figure or a path in that order inside a window running the other way, `auto` takes a text's first strong direction (a name). On `ui.field` it is the value's direction, as on an `input`: `auto` starts an Arabic name at the right, `ltr` keeps an address or a host left to right. Defaults to the window's.
    dir: ("ltr" | "rtl" | "auto")?,
    --- The pointer's shape over the node and over what is inside it that sets none, by its CSS `cursor` name. Desktop shows the arrow on controls; set this only where its style sheet names another cursor, as the browser's address field does (`text`).
    cursor: ("default" | "pointer" | "text" | "grab" | "grabbing" | "not-allowed" | "move" | "col-resize" | "row-resize" | "ew-resize" | "ns-resize" | "nwse-resize" | "nesw-resize" | "crosshair" | "cell" | "zoom-in" | "zoom-out")?,
    --- The field's content, from `ctx:CreateEditor`.
    editor: Editor,
    --- Shows while the field is empty.
    placeholder: string?,
    --- The size.
    size: ControlSize?,
    --- Drawn as invalid.
    invalid: boolean?,
    --- Hides the text, for a password.
    secure: boolean?,
    --- Takes no typing.
    disabled: boolean?,
    --- Takes no typing but looks as usual, such as while what was typed is sent.
    readOnly: boolean?,
    --- Grows to more lines. Enter inserts a line break; Mod+Enter submits.
    multiline: boolean?,
    --- The look. Defaults to `default`.
    variant: FieldVariant?,
    --- An icon in the box before the text, as Desktop's `TextInput` `leadingIcon`: a search field's magnifying glass. The `default` variant only.
    icon: IconName?,
    --- A Clear button at the box's end that empties the field, as Desktop's `TextInput` `showClearButton`: set it while the field has text. The `default` variant only.
    clearable: boolean?,
    --- The key of the element Down moves the keyboard to from this field, or that element's first focusable control: a list under its search field, as official's site chooser does. The `default` variant only.
    focusDown: string?,
    --- Keys the field hands to its editor's `KeyPressed` instead of handling them: any of `ArrowUp`, `ArrowDown` and `Escape`.
    keys: { string }?,
    --- Runs on Enter with the text in `event.text`.
    onSubmit: UiHandler?,
}

--- One item of `ui.segmented`.
export type UiSegmentedItem = {
    --- What `onChange` receives.
    id: string,
    --- Its icon.
    icon: IconName,
    --- Its accessible name and tooltip.
    label: string,
    --- Its element's key. Defaults to `id`.
    key: string?,
}

--- `ui.listbox` props: a listbox container (`role="listbox"`) holding `ui.option` rows.
export type UiListboxProps = {
    --- Identity among siblings across renders, and the anchor key for popovers and menus.
    key: string?,
    --- Width.
    w: UiSize?,
    --- Height.
    h: UiSize?,
    --- Minimum width.
    minW: number?,
    --- Minimum height.
    minH: number?,
    --- Maximum width.
    maxW: number?,
    --- Maximum height.
    maxH: number?,
    --- Share of a stack's free space.
    grow: number?,
    --- May shrink below its content size.
    shrink: boolean?,
    --- Padding on every side.
    pad: number?,
    --- Left and right padding.
    px: number?,
    --- Top and bottom padding.
    py: number?,
    --- Top padding.
    pt: number?,
    --- Right padding.
    pr: number?,
    --- Bottom padding.
    pb: number?,
    --- Left padding.
    pl: number?,
    --- Margin on every side.
    m: number?,
    --- Left and right margin.
    mx: number?,
    --- Top and bottom margin.
    my: number?,
    --- Top margin.
    mt: number?,
    --- Right margin.
    mr: number?,
    --- Bottom margin.
    mb: number?,
    --- Left margin.
    ml: number?,
    --- Takes the node out of flow, at these offsets from its parent's edges.
    pin: UiPin?,
    --- 0 to 1.
    opacity: number?,
    --- The accessibility role.
    role: Role?,
    --- The accessible name.
    label: string?,
    --- A tooltip.
    help: string?,
    --- Where the tooltip shows. Defaults to below.
    helpSide: Side?,
    --- Keeps the tooltip showing while true, whatever the pointer does, as Desktop's `Tooltip forceOpen`: a Copy button's "Copied" for a moment after the press.
    helpOpen: boolean?,
    --- Which way the subtree reads, as HTML's `dir` does: `ltr` or `rtl` keeps a figure or a path in that order inside a window running the other way, `auto` takes a text's first strong direction (a name). On `ui.field` it is the value's direction, as on an `input`: `auto` starts an Arabic name at the right, `ltr` keeps an address or a host left to right. Defaults to the window's.
    dir: ("ltr" | "rtl" | "auto")?,
    --- The pointer's shape over the node and over what is inside it that sets none, by its CSS `cursor` name. Desktop shows the arrow on controls; set this only where its style sheet names another cursor, as the browser's address field does (`text`).
    cursor: ("default" | "pointer" | "text" | "grab" | "grabbing" | "not-allowed" | "move" | "col-resize" | "row-resize" | "ew-resize" | "ns-resize" | "nwse-resize" | "nesw-resize" | "crosshair" | "cell" | "zoom-in" | "zoom-out")?,
    --- Space between children.
    gap: number?,
    --- How children sit across the axis.
    align: UiAlign?,
    --- How children sit along the axis.
    justify: UiJustify?,
    --- Centers children on both axes.
    center: boolean?,
    --- Children wrap onto more lines.
    wrap: boolean?,
    --- Background.
    fill: Color?,
    --- Background under the pointer.
    hover: Color?,
    --- Background while pressed.
    press: Color?,
    --- Draws the `surface-selected` fill and reports the selected state to assistive technology.
    selected: boolean?,
    --- Border color.
    border: Color?,
    --- Border width. Defaults to 1 when `border` is set.
    borderWidth: number?,
    --- Corner radius.
    radius: number?,
    --- Per-corner radii: top-left, top-right, bottom-right, bottom-left.
    corners: { number }?,
    --- A shadow from the app's elevation scale.
    shadow: Elevation?,
    --- Clips children to the box.
    clip: boolean?,
    --- Scrolls vertical overflow.
    scrollY: boolean?,
    --- Scrolls horizontal overflow.
    scrollX: boolean?,
    --- With `scrollY`: stays scrolled to the bottom as content grows, as a log or a chat does, until the reader scrolls up.
    stickBottom: boolean?,
    --- Takes keyboard focus.
    focusable: boolean?,
    --- Takes no presses.
    disabled: boolean?,
    --- Runs on a press.
    onPress: UiHandler?,
    --- Its `onPress` opens a menu, as Kobalte's `Menu.Trigger` does: a mouse runs it as the button goes down, and that press's release runs nothing. A key, a screen reader's Invoke or a tap run it on the click. Contract 1.1; a 1.0 host ignores it with one warning and opens on the click.
    menuTrigger: boolean?,
    --- Runs on a middle click, as a browser's `auxclick`: a tab closes, a row opens in the background. A `middle-press` event, with the modifier keys.
    onMiddlePress: UiHandler?,
    --- Runs as the pointer enters and leaves.
    onHover: UiHandler?,
    --- How hover and press fills change.
    hoverTransition: HoverTransition?,
    --- Starts a hover group: descendants with `hoverGroupColor` or `revealOnHover` react while this box is hovered.
    hoverGroup: boolean?,
    --- The text or icon color while the nearest hover group is hovered.
    hoverGroupColor: Color?,
    --- Shows only while the nearest hover group is hovered or focused.
    revealOnHover: boolean?,
    --- The default text and icon color for descendants.
    color: Color?,
    --- Stack direction: `"col"` (default), `"row"` or `"box"`.
    direction: UiDirection?,
    --- Per-side border widths in points: `{ top, right, bottom, left }`. Overrides `borderWidth`.
    borderSides: { number }?,
    --- The key of the element that takes focus when this frame leaves while focus is on it or inside it: one view of a popover handing focus to the control that leads back to it. That element, or its first focusable child; nothing happens when none shows.
    focusReturn: string?,
    --- One Tab stop for the focusable controls inside, which the arrow keys move between, as a roving `tabindex` does: Up and Down with `role` `list`, `listbox`, `menu` or `tree`, Left and Right as well otherwise. Home and End go to the ends; a `list` or `tree` stops at its ends, the others wrap. In a `list`, `listbox`, `menu`, `tree` or `tablist`, typing moves to the first control whose label starts with it. Space presses the focused control, and in a list Return does too.
    focusGroup: boolean?,
    --- Keys `onKey` takes while focus is on the frame or inside it, outside a text field, pressed with no modifier: any of `Enter`, `Escape`, `ArrowUp`, `ArrowDown`, `ArrowLeft`, `ArrowRight`, `Home`, `End`, `PageUp`, `PageDown`, `Delete` and `Backspace`. A key it takes does nothing else there: Return in a `focusGroup` list then no longer presses the focused control.
    keys: { string }?,
    --- Runs as a key `keys` names is pressed, with its name in `event.keyName`.
    onKey: UiHandler?,
}

--- `ui.option` props: one selectable option row (`role="option"`) inside a `ui.listbox`.
export type UiOptionProps = {
    --- Identity among siblings across renders, and the anchor key for popovers and menus.
    key: string?,
    --- Width.
    w: UiSize?,
    --- Height.
    h: UiSize?,
    --- Minimum width.
    minW: number?,
    --- Minimum height.
    minH: number?,
    --- Maximum width.
    maxW: number?,
    --- Maximum height.
    maxH: number?,
    --- Share of a stack's free space.
    grow: number?,
    --- May shrink below its content size.
    shrink: boolean?,
    --- Padding on every side.
    pad: number?,
    --- Left and right padding.
    px: number?,
    --- Top and bottom padding.
    py: number?,
    --- Top padding.
    pt: number?,
    --- Right padding.
    pr: number?,
    --- Bottom padding.
    pb: number?,
    --- Left padding.
    pl: number?,
    --- Margin on every side.
    m: number?,
    --- Left and right margin.
    mx: number?,
    --- Top and bottom margin.
    my: number?,
    --- Top margin.
    mt: number?,
    --- Right margin.
    mr: number?,
    --- Bottom margin.
    mb: number?,
    --- Left margin.
    ml: number?,
    --- Takes the node out of flow, at these offsets from its parent's edges.
    pin: UiPin?,
    --- 0 to 1.
    opacity: number?,
    --- The accessibility role.
    role: Role?,
    --- The accessible name.
    label: string?,
    --- A tooltip.
    help: string?,
    --- Where the tooltip shows. Defaults to below.
    helpSide: Side?,
    --- Keeps the tooltip showing while true, whatever the pointer does, as Desktop's `Tooltip forceOpen`: a Copy button's "Copied" for a moment after the press.
    helpOpen: boolean?,
    --- Which way the subtree reads, as HTML's `dir` does: `ltr` or `rtl` keeps a figure or a path in that order inside a window running the other way, `auto` takes a text's first strong direction (a name). On `ui.field` it is the value's direction, as on an `input`: `auto` starts an Arabic name at the right, `ltr` keeps an address or a host left to right. Defaults to the window's.
    dir: ("ltr" | "rtl" | "auto")?,
    --- The pointer's shape over the node and over what is inside it that sets none, by its CSS `cursor` name. Desktop shows the arrow on controls; set this only where its style sheet names another cursor, as the browser's address field does (`text`).
    cursor: ("default" | "pointer" | "text" | "grab" | "grabbing" | "not-allowed" | "move" | "col-resize" | "row-resize" | "ew-resize" | "ns-resize" | "nwse-resize" | "nesw-resize" | "crosshair" | "cell" | "zoom-in" | "zoom-out")?,
    --- Space between children.
    gap: number?,
    --- How children sit across the axis.
    align: UiAlign?,
    --- How children sit along the axis.
    justify: UiJustify?,
    --- Centers children on both axes.
    center: boolean?,
    --- Children wrap onto more lines.
    wrap: boolean?,
    --- Background.
    fill: Color?,
    --- Background under the pointer.
    hover: Color?,
    --- Background while pressed.
    press: Color?,
    --- Whether this option is selected (`aria-selected`).
    selected: boolean?,
    --- Border color.
    border: Color?,
    --- Border width. Defaults to 1 when `border` is set.
    borderWidth: number?,
    --- Corner radius.
    radius: number?,
    --- Per-corner radii: top-left, top-right, bottom-right, bottom-left.
    corners: { number }?,
    --- A shadow from the app's elevation scale.
    shadow: Elevation?,
    --- Clips children to the box.
    clip: boolean?,
    --- Scrolls vertical overflow.
    scrollY: boolean?,
    --- Scrolls horizontal overflow.
    scrollX: boolean?,
    --- With `scrollY`: stays scrolled to the bottom as content grows, as a log or a chat does, until the reader scrolls up.
    stickBottom: boolean?,
    --- Takes keyboard focus.
    focusable: boolean?,
    --- Takes no presses.
    disabled: boolean?,
    --- Runs on a press.
    onPress: UiHandler?,
    --- Its `onPress` opens a menu, as Kobalte's `Menu.Trigger` does: a mouse runs it as the button goes down, and that press's release runs nothing. A key, a screen reader's Invoke or a tap run it on the click. Contract 1.1; a 1.0 host ignores it with one warning and opens on the click.
    menuTrigger: boolean?,
    --- Runs on a middle click, as a browser's `auxclick`: a tab closes, a row opens in the background. A `middle-press` event, with the modifier keys.
    onMiddlePress: UiHandler?,
    --- Runs as the pointer enters and leaves.
    onHover: UiHandler?,
    --- How hover and press fills change.
    hoverTransition: HoverTransition?,
    --- Starts a hover group: descendants with `hoverGroupColor` or `revealOnHover` react while this box is hovered.
    hoverGroup: boolean?,
    --- The text or icon color while the nearest hover group is hovered.
    hoverGroupColor: Color?,
    --- Shows only while the nearest hover group is hovered or focused.
    revealOnHover: boolean?,
    --- The default text and icon color for descendants.
    color: Color?,
    --- Stack direction: `"col"` (default), `"row"` or `"box"`.
    direction: UiDirection?,
    --- Per-side border widths in points: `{ top, right, bottom, left }`. Overrides `borderWidth`.
    borderSides: { number }?,
    --- The key of the element that takes focus when this frame leaves while focus is on it or inside it: one view of a popover handing focus to the control that leads back to it. That element, or its first focusable child; nothing happens when none shows.
    focusReturn: string?,
    --- One Tab stop for the focusable controls inside, which the arrow keys move between, as a roving `tabindex` does: Up and Down with `role` `list`, `listbox`, `menu` or `tree`, Left and Right as well otherwise. Home and End go to the ends; a `list` or `tree` stops at its ends, the others wrap. In a `list`, `listbox`, `menu`, `tree` or `tablist`, typing moves to the first control whose label starts with it. Space presses the focused control, and in a list Return does too.
    focusGroup: boolean?,
    --- Keys `onKey` takes while focus is on the frame or inside it, outside a text field, pressed with no modifier: any of `Enter`, `Escape`, `ArrowUp`, `ArrowDown`, `ArrowLeft`, `ArrowRight`, `Home`, `End`, `PageUp`, `PageDown`, `Delete` and `Backspace`. A key it takes does nothing else there: Return in a `focusGroup` list then no longer presses the focused control.
    keys: { string }?,
    --- Runs as a key `keys` names is pressed, with its name in `event.keyName`.
    onKey: UiHandler?,
}

--- One option of `ui.select`.
export type UiSelectOption = {
    --- What `onChange` receives in `event.value`.
    value: string,
    --- The option's visible text.
    label: string,
}

--- `ui.select` props: Desktop's `Select` dropdown control (`select-v2`). Desktop's plugin API calls it `Select`.
export type UiSelectProps = {
    --- Identity among siblings across renders, and the anchor key for popovers and menus.
    key: string?,
    --- Width.
    w: UiSize?,
    --- Height.
    h: UiSize?,
    --- Minimum width.
    minW: number?,
    --- Minimum height.
    minH: number?,
    --- Maximum width.
    maxW: number?,
    --- Maximum height.
    maxH: number?,
    --- Share of a stack's free space.
    grow: number?,
    --- May shrink below its content size.
    shrink: boolean?,
    --- Padding on every side.
    pad: number?,
    --- Left and right padding.
    px: number?,
    --- Top and bottom padding.
    py: number?,
    --- Top padding.
    pt: number?,
    --- Right padding.
    pr: number?,
    --- Bottom padding.
    pb: number?,
    --- Left padding.
    pl: number?,
    --- Margin on every side.
    m: number?,
    --- Left and right margin.
    mx: number?,
    --- Top and bottom margin.
    my: number?,
    --- Top margin.
    mt: number?,
    --- Right margin.
    mr: number?,
    --- Bottom margin.
    mb: number?,
    --- Left margin.
    ml: number?,
    --- Takes the node out of flow, at these offsets from its parent's edges.
    pin: UiPin?,
    --- 0 to 1.
    opacity: number?,
    --- The accessibility role.
    role: Role?,
    --- The accessible name.
    label: string?,
    --- A tooltip.
    help: string?,
    --- Where the tooltip shows. Defaults to below.
    helpSide: Side?,
    --- Keeps the tooltip showing while true, whatever the pointer does, as Desktop's `Tooltip forceOpen`: a Copy button's "Copied" for a moment after the press.
    helpOpen: boolean?,
    --- Which way the subtree reads, as HTML's `dir` does: `ltr` or `rtl` keeps a figure or a path in that order inside a window running the other way, `auto` takes a text's first strong direction (a name). On `ui.field` it is the value's direction, as on an `input`: `auto` starts an Arabic name at the right, `ltr` keeps an address or a host left to right. Defaults to the window's.
    dir: ("ltr" | "rtl" | "auto")?,
    --- The pointer's shape over the node and over what is inside it that sets none, by its CSS `cursor` name. Desktop shows the arrow on controls; set this only where its style sheet names another cursor, as the browser's address field does (`text`).
    cursor: ("default" | "pointer" | "text" | "grab" | "grabbing" | "not-allowed" | "move" | "col-resize" | "row-resize" | "ew-resize" | "ns-resize" | "nwse-resize" | "nesw-resize" | "crosshair" | "cell" | "zoom-in" | "zoom-out")?,
    --- The selected option's `value`.
    value: string?,
    --- Text shown when no option matches `value`.
    placeholder: string?,
    --- The options, in order.
    options: { UiSelectOption },
    --- Cannot open; drawn at half opacity.
    disabled: boolean?,
    --- Runs when an option is picked, with its value in `event.value`.
    onChange: UiHandler?,
}

--- `ui.segmented` props: the app's segmented control of icon items.
export type UiSegmentedProps = {
    --- Identity among siblings across renders, and the anchor key for popovers and menus.
    key: string?,
    --- Width.
    w: UiSize?,
    --- Height.
    h: UiSize?,
    --- Minimum width.
    minW: number?,
    --- Minimum height.
    minH: number?,
    --- Maximum width.
    maxW: number?,
    --- Maximum height.
    maxH: number?,
    --- Share of a stack's free space.
    grow: number?,
    --- May shrink below its content size.
    shrink: boolean?,
    --- Padding on every side.
    pad: number?,
    --- Left and right padding.
    px: number?,
    --- Top and bottom padding.
    py: number?,
    --- Top padding.
    pt: number?,
    --- Right padding.
    pr: number?,
    --- Bottom padding.
    pb: number?,
    --- Left padding.
    pl: number?,
    --- Margin on every side.
    m: number?,
    --- Left and right margin.
    mx: number?,
    --- Top and bottom margin.
    my: number?,
    --- Top margin.
    mt: number?,
    --- Right margin.
    mr: number?,
    --- Bottom margin.
    mb: number?,
    --- Left margin.
    ml: number?,
    --- Takes the node out of flow, at these offsets from its parent's edges.
    pin: UiPin?,
    --- 0 to 1.
    opacity: number?,
    --- The accessibility role.
    role: Role?,
    --- The accessible name.
    label: string?,
    --- A tooltip.
    help: string?,
    --- Where the tooltip shows. Defaults to below.
    helpSide: Side?,
    --- Keeps the tooltip showing while true, whatever the pointer does, as Desktop's `Tooltip forceOpen`: a Copy button's "Copied" for a moment after the press.
    helpOpen: boolean?,
    --- Which way the subtree reads, as HTML's `dir` does: `ltr` or `rtl` keeps a figure or a path in that order inside a window running the other way, `auto` takes a text's first strong direction (a name). On `ui.field` it is the value's direction, as on an `input`: `auto` starts an Arabic name at the right, `ltr` keeps an address or a host left to right. Defaults to the window's.
    dir: ("ltr" | "rtl" | "auto")?,
    --- The pointer's shape over the node and over what is inside it that sets none, by its CSS `cursor` name. Desktop shows the arrow on controls; set this only where its style sheet names another cursor, as the browser's address field does (`text`).
    cursor: ("default" | "pointer" | "text" | "grab" | "grabbing" | "not-allowed" | "move" | "col-resize" | "row-resize" | "ew-resize" | "ns-resize" | "nwse-resize" | "nesw-resize" | "crosshair" | "cell" | "zoom-in" | "zoom-out")?,
    --- Its items, in order.
    items: { UiSegmentedItem },
    --- The picked item's id.
    value: string,
    --- Runs as the user picks another, with its id in `event.value`.
    onChange: UiHandler?,
}

--- `ui.loader` props: the app's spinner.
export type UiLoaderProps = {
    --- Identity among siblings across renders, and the anchor key for popovers and menus.
    key: string?,
    --- Width.
    w: UiSize?,
    --- Height.
    h: UiSize?,
    --- Minimum width.
    minW: number?,
    --- Minimum height.
    minH: number?,
    --- Maximum width.
    maxW: number?,
    --- Maximum height.
    maxH: number?,
    --- Share of a stack's free space.
    grow: number?,
    --- May shrink below its content size.
    shrink: boolean?,
    --- Padding on every side.
    pad: number?,
    --- Left and right padding.
    px: number?,
    --- Top and bottom padding.
    py: number?,
    --- Top padding.
    pt: number?,
    --- Right padding.
    pr: number?,
    --- Bottom padding.
    pb: number?,
    --- Left padding.
    pl: number?,
    --- Margin on every side.
    m: number?,
    --- Left and right margin.
    mx: number?,
    --- Top and bottom margin.
    my: number?,
    --- Top margin.
    mt: number?,
    --- Right margin.
    mr: number?,
    --- Bottom margin.
    mb: number?,
    --- Left margin.
    ml: number?,
    --- Takes the node out of flow, at these offsets from its parent's edges.
    pin: UiPin?,
    --- 0 to 1.
    opacity: number?,
    --- The accessibility role.
    role: Role?,
    --- The accessible name.
    label: string?,
    --- A tooltip.
    help: string?,
    --- Where the tooltip shows. Defaults to below.
    helpSide: Side?,
    --- Keeps the tooltip showing while true, whatever the pointer does, as Desktop's `Tooltip forceOpen`: a Copy button's "Copied" for a moment after the press.
    helpOpen: boolean?,
    --- Which way the subtree reads, as HTML's `dir` does: `ltr` or `rtl` keeps a figure or a path in that order inside a window running the other way, `auto` takes a text's first strong direction (a name). On `ui.field` it is the value's direction, as on an `input`: `auto` starts an Arabic name at the right, `ltr` keeps an address or a host left to right. Defaults to the window's.
    dir: ("ltr" | "rtl" | "auto")?,
    --- The pointer's shape over the node and over what is inside it that sets none, by its CSS `cursor` name. Desktop shows the arrow on controls; set this only where its style sheet names another cursor, as the browser's address field does (`text`).
    cursor: ("default" | "pointer" | "text" | "grab" | "grabbing" | "not-allowed" | "move" | "col-resize" | "row-resize" | "ew-resize" | "ns-resize" | "nwse-resize" | "nesw-resize" | "crosshair" | "cell" | "zoom-in" | "zoom-out")?,
    --- Width and height in points. Defaults to 16.
    size: number?,
}

--- `ui.serverIndicator` props: a server's health as Settings > Servers shows it (Desktop's `ServerRow.Indicator`).
export type UiServerIndicatorProps = {
    --- Identity among siblings across renders, and the anchor key for popovers and menus.
    key: string?,
    --- Width.
    w: UiSize?,
    --- Height.
    h: UiSize?,
    --- Minimum width.
    minW: number?,
    --- Minimum height.
    minH: number?,
    --- Maximum width.
    maxW: number?,
    --- Maximum height.
    maxH: number?,
    --- Share of a stack's free space.
    grow: number?,
    --- May shrink below its content size.
    shrink: boolean?,
    --- Padding on every side.
    pad: number?,
    --- Left and right padding.
    px: number?,
    --- Top and bottom padding.
    py: number?,
    --- Top padding.
    pt: number?,
    --- Right padding.
    pr: number?,
    --- Bottom padding.
    pb: number?,
    --- Left padding.
    pl: number?,
    --- Margin on every side.
    m: number?,
    --- Left and right margin.
    mx: number?,
    --- Top and bottom margin.
    my: number?,
    --- Top margin.
    mt: number?,
    --- Right margin.
    mr: number?,
    --- Bottom margin.
    mb: number?,
    --- Left margin.
    ml: number?,
    --- Takes the node out of flow, at these offsets from its parent's edges.
    pin: UiPin?,
    --- 0 to 1.
    opacity: number?,
    --- The accessibility role.
    role: Role?,
    --- The accessible name.
    label: string?,
    --- A tooltip.
    help: string?,
    --- Where the tooltip shows. Defaults to below.
    helpSide: Side?,
    --- Keeps the tooltip showing while true, whatever the pointer does, as Desktop's `Tooltip forceOpen`: a Copy button's "Copied" for a moment after the press.
    helpOpen: boolean?,
    --- Which way the subtree reads, as HTML's `dir` does: `ltr` or `rtl` keeps a figure or a path in that order inside a window running the other way, `auto` takes a text's first strong direction (a name). On `ui.field` it is the value's direction, as on an `input`: `auto` starts an Arabic name at the right, `ltr` keeps an address or a host left to right. Defaults to the window's.
    dir: ("ltr" | "rtl" | "auto")?,
    --- The pointer's shape over the node and over what is inside it that sets none, by its CSS `cursor` name. Desktop shows the arrow on controls; set this only where its style sheet names another cursor, as the browser's address field does (`text`).
    cursor: ("default" | "pointer" | "text" | "grab" | "grabbing" | "not-allowed" | "move" | "col-resize" | "row-resize" | "ew-resize" | "ns-resize" | "nwse-resize" | "nesw-resize" | "crosshair" | "cell" | "zoom-in" | "zoom-out")?,
    --- Its latest health check: a green dot once it answered, red when it didn't, a faint one when unknown.
    health: ServerHealth?,
    --- It waits for the user to sign in: a lock.
    auth: boolean?,
    --- It is starting or connecting: a spinner.
    connecting: boolean?,
}

--- `ui.pill` props: a status capsule like the composer's pills.
export type UiPillProps = {
    --- Identity among siblings across renders, and the anchor key for popovers and menus.
    key: string?,
    --- Width.
    w: UiSize?,
    --- Height.
    h: UiSize?,
    --- Minimum width.
    minW: number?,
    --- Minimum height.
    minH: number?,
    --- Maximum width.
    maxW: number?,
    --- Maximum height.
    maxH: number?,
    --- Share of a stack's free space.
    grow: number?,
    --- May shrink below its content size.
    shrink: boolean?,
    --- Padding on every side.
    pad: number?,
    --- Left and right padding.
    px: number?,
    --- Top and bottom padding.
    py: number?,
    --- Top padding.
    pt: number?,
    --- Right padding.
    pr: number?,
    --- Bottom padding.
    pb: number?,
    --- Left padding.
    pl: number?,
    --- Margin on every side.
    m: number?,
    --- Left and right margin.
    mx: number?,
    --- Top and bottom margin.
    my: number?,
    --- Top margin.
    mt: number?,
    --- Right margin.
    mr: number?,
    --- Bottom margin.
    mb: number?,
    --- Left margin.
    ml: number?,
    --- Takes the node out of flow, at these offsets from its parent's edges.
    pin: UiPin?,
    --- 0 to 1.
    opacity: number?,
    --- The accessibility role.
    role: Role?,
    --- The accessible name.
    label: string?,
    --- A tooltip.
    help: string?,
    --- Where the tooltip shows. Defaults to below.
    helpSide: Side?,
    --- Keeps the tooltip showing while true, whatever the pointer does, as Desktop's `Tooltip forceOpen`: a Copy button's "Copied" for a moment after the press.
    helpOpen: boolean?,
    --- Which way the subtree reads, as HTML's `dir` does: `ltr` or `rtl` keeps a figure or a path in that order inside a window running the other way, `auto` takes a text's first strong direction (a name). On `ui.field` it is the value's direction, as on an `input`: `auto` starts an Arabic name at the right, `ltr` keeps an address or a host left to right. Defaults to the window's.
    dir: ("ltr" | "rtl" | "auto")?,
    --- The pointer's shape over the node and over what is inside it that sets none, by its CSS `cursor` name. Desktop shows the arrow on controls; set this only where its style sheet names another cursor, as the browser's address field does (`text`).
    cursor: ("default" | "pointer" | "text" | "grab" | "grabbing" | "not-allowed" | "move" | "col-resize" | "row-resize" | "ew-resize" | "ns-resize" | "nwse-resize" | "nesw-resize" | "crosshair" | "cell" | "zoom-in" | "zoom-out")?,
    --- The capsule's label.
    text: string,
    --- A leading icon.
    icon: IconName?,
    --- Colors the icon.
    tone: Tone?,
    --- Shows the capsule pressed.
    pressed: boolean?,
    --- Runs on a press.
    onPress: UiHandler?,
}

--- `ui.keybind` props: a shortcut chip.
export type UiKeybindProps = {
    --- Identity among siblings across renders, and the anchor key for popovers and menus.
    key: string?,
    --- Width.
    w: UiSize?,
    --- Height.
    h: UiSize?,
    --- Minimum width.
    minW: number?,
    --- Minimum height.
    minH: number?,
    --- Maximum width.
    maxW: number?,
    --- Maximum height.
    maxH: number?,
    --- Share of a stack's free space.
    grow: number?,
    --- May shrink below its content size.
    shrink: boolean?,
    --- Padding on every side.
    pad: number?,
    --- Left and right padding.
    px: number?,
    --- Top and bottom padding.
    py: number?,
    --- Top padding.
    pt: number?,
    --- Right padding.
    pr: number?,
    --- Bottom padding.
    pb: number?,
    --- Left padding.
    pl: number?,
    --- Margin on every side.
    m: number?,
    --- Left and right margin.
    mx: number?,
    --- Top and bottom margin.
    my: number?,
    --- Top margin.
    mt: number?,
    --- Right margin.
    mr: number?,
    --- Bottom margin.
    mb: number?,
    --- Left margin.
    ml: number?,
    --- Takes the node out of flow, at these offsets from its parent's edges.
    pin: UiPin?,
    --- 0 to 1.
    opacity: number?,
    --- The accessibility role.
    role: Role?,
    --- The accessible name.
    label: string?,
    --- A tooltip.
    help: string?,
    --- Where the tooltip shows. Defaults to below.
    helpSide: Side?,
    --- Keeps the tooltip showing while true, whatever the pointer does, as Desktop's `Tooltip forceOpen`: a Copy button's "Copied" for a moment after the press.
    helpOpen: boolean?,
    --- Which way the subtree reads, as HTML's `dir` does: `ltr` or `rtl` keeps a figure or a path in that order inside a window running the other way, `auto` takes a text's first strong direction (a name). On `ui.field` it is the value's direction, as on an `input`: `auto` starts an Arabic name at the right, `ltr` keeps an address or a host left to right. Defaults to the window's.
    dir: ("ltr" | "rtl" | "auto")?,
    --- The pointer's shape over the node and over what is inside it that sets none, by its CSS `cursor` name. Desktop shows the arrow on controls; set this only where its style sheet names another cursor, as the browser's address field does (`text`).
    cursor: ("default" | "pointer" | "text" | "grab" | "grabbing" | "not-allowed" | "move" | "col-resize" | "row-resize" | "ew-resize" | "ns-resize" | "nwse-resize" | "nesw-resize" | "crosshair" | "cell" | "zoom-in" | "zoom-out")?,
    --- A published command id. The chip shows the user's current binding.
    command: string?,
    --- A literal binding, such as `"mod+shift+t"`, when there is no command.
    keys: string?,
}

--- `ui.settingsRow` props: one settings row, as the app's own.
export type UiSettingsRowProps = {
    --- Identity among siblings across renders, and the anchor key for popovers and menus.
    key: string?,
    --- Width.
    w: UiSize?,
    --- Height.
    h: UiSize?,
    --- Minimum width.
    minW: number?,
    --- Minimum height.
    minH: number?,
    --- Maximum width.
    maxW: number?,
    --- Maximum height.
    maxH: number?,
    --- Share of a stack's free space.
    grow: number?,
    --- May shrink below its content size.
    shrink: boolean?,
    --- Padding on every side.
    pad: number?,
    --- Left and right padding.
    px: number?,
    --- Top and bottom padding.
    py: number?,
    --- Top padding.
    pt: number?,
    --- Right padding.
    pr: number?,
    --- Bottom padding.
    pb: number?,
    --- Left padding.
    pl: number?,
    --- Margin on every side.
    m: number?,
    --- Left and right margin.
    mx: number?,
    --- Top and bottom margin.
    my: number?,
    --- Top margin.
    mt: number?,
    --- Right margin.
    mr: number?,
    --- Bottom margin.
    mb: number?,
    --- Left margin.
    ml: number?,
    --- Takes the node out of flow, at these offsets from its parent's edges.
    pin: UiPin?,
    --- 0 to 1.
    opacity: number?,
    --- The accessibility role.
    role: Role?,
    --- The accessible name.
    label: string?,
    --- A tooltip.
    help: string?,
    --- Where the tooltip shows. Defaults to below.
    helpSide: Side?,
    --- Keeps the tooltip showing while true, whatever the pointer does, as Desktop's `Tooltip forceOpen`: a Copy button's "Copied" for a moment after the press.
    helpOpen: boolean?,
    --- Which way the subtree reads, as HTML's `dir` does: `ltr` or `rtl` keeps a figure or a path in that order inside a window running the other way, `auto` takes a text's first strong direction (a name). On `ui.field` it is the value's direction, as on an `input`: `auto` starts an Arabic name at the right, `ltr` keeps an address or a host left to right. Defaults to the window's.
    dir: ("ltr" | "rtl" | "auto")?,
    --- The pointer's shape over the node and over what is inside it that sets none, by its CSS `cursor` name. Desktop shows the arrow on controls; set this only where its style sheet names another cursor, as the browser's address field does (`text`).
    cursor: ("default" | "pointer" | "text" | "grab" | "grabbing" | "not-allowed" | "move" | "col-resize" | "row-resize" | "ew-resize" | "ns-resize" | "nwse-resize" | "nesw-resize" | "crosshair" | "cell" | "zoom-in" | "zoom-out")?,
    --- The row's search id: settings search scrolls to it.
    id: string,
    --- The row's title.
    title: string,
    --- A line under the title.
    description: string?,
}

--- `ui.settingsList` props: a card of settings rows split by hairlines.
export type UiSettingsListProps = {
    --- Identity among siblings across renders, and the anchor key for popovers and menus.
    key: string?,
    --- Width.
    w: UiSize?,
    --- Height.
    h: UiSize?,
    --- Minimum width.
    minW: number?,
    --- Minimum height.
    minH: number?,
    --- Maximum width.
    maxW: number?,
    --- Maximum height.
    maxH: number?,
    --- Share of a stack's free space.
    grow: number?,
    --- May shrink below its content size.
    shrink: boolean?,
    --- Padding on every side.
    pad: number?,
    --- Left and right padding.
    px: number?,
    --- Top and bottom padding.
    py: number?,
    --- Top padding.
    pt: number?,
    --- Right padding.
    pr: number?,
    --- Bottom padding.
    pb: number?,
    --- Left padding.
    pl: number?,
    --- Margin on every side.
    m: number?,
    --- Left and right margin.
    mx: number?,
    --- Top and bottom margin.
    my: number?,
    --- Top margin.
    mt: number?,
    --- Right margin.
    mr: number?,
    --- Bottom margin.
    mb: number?,
    --- Left margin.
    ml: number?,
    --- Takes the node out of flow, at these offsets from its parent's edges.
    pin: UiPin?,
    --- 0 to 1.
    opacity: number?,
    --- The accessibility role.
    role: Role?,
    --- The accessible name.
    label: string?,
    --- A tooltip.
    help: string?,
    --- Where the tooltip shows. Defaults to below.
    helpSide: Side?,
    --- Keeps the tooltip showing while true, whatever the pointer does, as Desktop's `Tooltip forceOpen`: a Copy button's "Copied" for a moment after the press.
    helpOpen: boolean?,
    --- Which way the subtree reads, as HTML's `dir` does: `ltr` or `rtl` keeps a figure or a path in that order inside a window running the other way, `auto` takes a text's first strong direction (a name). On `ui.field` it is the value's direction, as on an `input`: `auto` starts an Arabic name at the right, `ltr` keeps an address or a host left to right. Defaults to the window's.
    dir: ("ltr" | "rtl" | "auto")?,
    --- The pointer's shape over the node and over what is inside it that sets none, by its CSS `cursor` name. Desktop shows the arrow on controls; set this only where its style sheet names another cursor, as the browser's address field does (`text`).
    cursor: ("default" | "pointer" | "text" | "grab" | "grabbing" | "not-allowed" | "move" | "col-resize" | "row-resize" | "ew-resize" | "ns-resize" | "nwse-resize" | "nesw-resize" | "crosshair" | "cell" | "zoom-in" | "zoom-out")?,
    --- A section title above the card.
    title: string?,
    --- Desktop's catalog list (`data-variant="catalog"`): rows 16 in from the card's sides, for lists of entries rather than controls.
    catalog: boolean?,
}

--- `ui.flyoutRow` props: a popover row that opens a nested popover beside it. The host owns placement, hover intent and dismissal.
export type UiFlyoutRowProps = {
    --- Identity among siblings across renders, and the anchor key for popovers and menus.
    key: string?,
    --- Width.
    w: UiSize?,
    --- Height.
    h: UiSize?,
    --- Minimum width.
    minW: number?,
    --- Minimum height.
    minH: number?,
    --- Maximum width.
    maxW: number?,
    --- Maximum height.
    maxH: number?,
    --- Share of a stack's free space.
    grow: number?,
    --- May shrink below its content size.
    shrink: boolean?,
    --- Padding on every side.
    pad: number?,
    --- Left and right padding.
    px: number?,
    --- Top and bottom padding.
    py: number?,
    --- Top padding.
    pt: number?,
    --- Right padding.
    pr: number?,
    --- Bottom padding.
    pb: number?,
    --- Left padding.
    pl: number?,
    --- Margin on every side.
    m: number?,
    --- Left and right margin.
    mx: number?,
    --- Top and bottom margin.
    my: number?,
    --- Top margin.
    mt: number?,
    --- Right margin.
    mr: number?,
    --- Bottom margin.
    mb: number?,
    --- Left margin.
    ml: number?,
    --- Takes the node out of flow, at these offsets from its parent's edges.
    pin: UiPin?,
    --- 0 to 1.
    opacity: number?,
    --- The accessibility role.
    role: Role?,
    --- The accessible name.
    label: string?,
    --- A tooltip.
    help: string?,
    --- Where the tooltip shows. Defaults to below.
    helpSide: Side?,
    --- Keeps the tooltip showing while true, whatever the pointer does, as Desktop's `Tooltip forceOpen`: a Copy button's "Copied" for a moment after the press.
    helpOpen: boolean?,
    --- Which way the subtree reads, as HTML's `dir` does: `ltr` or `rtl` keeps a figure or a path in that order inside a window running the other way, `auto` takes a text's first strong direction (a name). On `ui.field` it is the value's direction, as on an `input`: `auto` starts an Arabic name at the right, `ltr` keeps an address or a host left to right. Defaults to the window's.
    dir: ("ltr" | "rtl" | "auto")?,
    --- The pointer's shape over the node and over what is inside it that sets none, by its CSS `cursor` name. Desktop shows the arrow on controls; set this only where its style sheet names another cursor, as the browser's address field does (`text`).
    cursor: ("default" | "pointer" | "text" | "grab" | "grabbing" | "not-allowed" | "move" | "col-resize" | "row-resize" | "ew-resize" | "ns-resize" | "nwse-resize" | "nesw-resize" | "crosshair" | "cell" | "zoom-in" | "zoom-out")?,
    --- The row's label.
    text: string,
    --- A leading icon.
    icon: IconName?,
    --- Muted text at the row's end, such as a count.
    detail: string?,
    --- The id of this plugin's popover the row opens.
    popover: string,
    --- Passed to the nested popover's render as `props.data`.
    props: { [string]: any }?,
    --- Shows a danger dot before the detail: something in the nested list failed.
    alert: boolean?,
    --- The side of the row the nested popover opens on. Defaults to `right`; a popover at the window's right edge opens its rows' lists on the `left`.
    side: Side?,
    --- Corner radius of the row's hover and open fill. Defaults to 4.
    radius: number?,
}

--- `ui.badge` props: Desktop's compact badge, a short muted label in a 16-point tag. Desktop's plugin API calls it `Badge`.
export type UiBadgeProps = {
    --- Identity among siblings across renders, and the anchor key for popovers and menus.
    key: string?,
    --- Width.
    w: UiSize?,
    --- Height.
    h: UiSize?,
    --- Minimum width.
    minW: number?,
    --- Minimum height.
    minH: number?,
    --- Maximum width.
    maxW: number?,
    --- Maximum height.
    maxH: number?,
    --- Share of a stack's free space.
    grow: number?,
    --- May shrink below its content size.
    shrink: boolean?,
    --- Padding on every side.
    pad: number?,
    --- Left and right padding.
    px: number?,
    --- Top and bottom padding.
    py: number?,
    --- Top padding.
    pt: number?,
    --- Right padding.
    pr: number?,
    --- Bottom padding.
    pb: number?,
    --- Left padding.
    pl: number?,
    --- Margin on every side.
    m: number?,
    --- Left and right margin.
    mx: number?,
    --- Top and bottom margin.
    my: number?,
    --- Top margin.
    mt: number?,
    --- Right margin.
    mr: number?,
    --- Bottom margin.
    mb: number?,
    --- Left margin.
    ml: number?,
    --- Takes the node out of flow, at these offsets from its parent's edges.
    pin: UiPin?,
    --- 0 to 1.
    opacity: number?,
    --- The accessibility role.
    role: Role?,
    --- The accessible name.
    label: string?,
    --- A tooltip.
    help: string?,
    --- Where the tooltip shows. Defaults to below.
    helpSide: Side?,
    --- Keeps the tooltip showing while true, whatever the pointer does, as Desktop's `Tooltip forceOpen`: a Copy button's "Copied" for a moment after the press.
    helpOpen: boolean?,
    --- Which way the subtree reads, as HTML's `dir` does: `ltr` or `rtl` keeps a figure or a path in that order inside a window running the other way, `auto` takes a text's first strong direction (a name). On `ui.field` it is the value's direction, as on an `input`: `auto` starts an Arabic name at the right, `ltr` keeps an address or a host left to right. Defaults to the window's.
    dir: ("ltr" | "rtl" | "auto")?,
    --- The pointer's shape over the node and over what is inside it that sets none, by its CSS `cursor` name. Desktop shows the arrow on controls; set this only where its style sheet names another cursor, as the browser's address field does (`text`).
    cursor: ("default" | "pointer" | "text" | "grab" | "grabbing" | "not-allowed" | "move" | "col-resize" | "row-resize" | "ew-resize" | "ns-resize" | "nwse-resize" | "nesw-resize" | "crosshair" | "cell" | "zoom-in" | "zoom-out")?,
    --- Its label, on one line; cut short where it has no room.
    text: string,
}

--- `ui.dialogHeader` props: the app's dialog header (`DialogHeader` with a `DialogTitleGroup`), 16 in from the frame, with its close button. Desktop's plugin API calls it `DialogHeader`.
export type UiDialogHeaderProps = {
    --- Identity among siblings across renders, and the anchor key for popovers and menus.
    key: string?,
    --- Width.
    w: UiSize?,
    --- Height.
    h: UiSize?,
    --- Minimum width.
    minW: number?,
    --- Minimum height.
    minH: number?,
    --- Maximum width.
    maxW: number?,
    --- Maximum height.
    maxH: number?,
    --- Share of a stack's free space.
    grow: number?,
    --- May shrink below its content size.
    shrink: boolean?,
    --- Padding on every side.
    pad: number?,
    --- Left and right padding.
    px: number?,
    --- Top and bottom padding.
    py: number?,
    --- Top padding.
    pt: number?,
    --- Right padding.
    pr: number?,
    --- Bottom padding.
    pb: number?,
    --- Left padding.
    pl: number?,
    --- Margin on every side.
    m: number?,
    --- Left and right margin.
    mx: number?,
    --- Top and bottom margin.
    my: number?,
    --- Top margin.
    mt: number?,
    --- Right margin.
    mr: number?,
    --- Bottom margin.
    mb: number?,
    --- Left margin.
    ml: number?,
    --- Takes the node out of flow, at these offsets from its parent's edges.
    pin: UiPin?,
    --- 0 to 1.
    opacity: number?,
    --- The accessibility role.
    role: Role?,
    --- The accessible name.
    label: string?,
    --- A tooltip.
    help: string?,
    --- Where the tooltip shows. Defaults to below.
    helpSide: Side?,
    --- Keeps the tooltip showing while true, whatever the pointer does, as Desktop's `Tooltip forceOpen`: a Copy button's "Copied" for a moment after the press.
    helpOpen: boolean?,
    --- Which way the subtree reads, as HTML's `dir` does: `ltr` or `rtl` keeps a figure or a path in that order inside a window running the other way, `auto` takes a text's first strong direction (a name). On `ui.field` it is the value's direction, as on an `input`: `auto` starts an Arabic name at the right, `ltr` keeps an address or a host left to right. Defaults to the window's.
    dir: ("ltr" | "rtl" | "auto")?,
    --- The pointer's shape over the node and over what is inside it that sets none, by its CSS `cursor` name. Desktop shows the arrow on controls; set this only where its style sheet names another cursor, as the browser's address field does (`text`).
    cursor: ("default" | "pointer" | "text" | "grab" | "grabbing" | "not-allowed" | "move" | "col-resize" | "row-resize" | "ew-resize" | "ns-resize" | "nwse-resize" | "nesw-resize" | "crosshair" | "cell" | "zoom-in" | "zoom-out")?,
    --- The dialog's title: 15 pt, wrapping anywhere unless `wrapTitle` is false.
    title: string,
    --- A muted line under the title.
    description: string?,
    --- A badge after the title on its line, such as a profile's name. The title then stays on one line, and each is cut short where the line has no room for both.
    badge: string?,
    --- Leaves out the close button.
    hideClose: boolean?,
    --- The close button's accessible name. Defaults to Close.
    closeLabel: string?,
    --- Whether the title wraps across lines (breaking anywhere for long identifiers). Defaults to true; set false for short single-line dialog titles.
    wrapTitle: boolean?,
}

--- `ui.markdown` props. Native: parsing, shaping, code fences and Mermaid diagrams stay in the host.
export type UiMarkdownProps = {
    --- Identity among siblings across renders, and the anchor key for popovers and menus.
    key: string?,
    --- Width.
    w: UiSize?,
    --- Height.
    h: UiSize?,
    --- Minimum width.
    minW: number?,
    --- Minimum height.
    minH: number?,
    --- Maximum width.
    maxW: number?,
    --- Maximum height.
    maxH: number?,
    --- Share of a stack's free space.
    grow: number?,
    --- May shrink below its content size.
    shrink: boolean?,
    --- Padding on every side.
    pad: number?,
    --- Left and right padding.
    px: number?,
    --- Top and bottom padding.
    py: number?,
    --- Top padding.
    pt: number?,
    --- Right padding.
    pr: number?,
    --- Bottom padding.
    pb: number?,
    --- Left padding.
    pl: number?,
    --- Margin on every side.
    m: number?,
    --- Left and right margin.
    mx: number?,
    --- Top and bottom margin.
    my: number?,
    --- Top margin.
    mt: number?,
    --- Right margin.
    mr: number?,
    --- Bottom margin.
    mb: number?,
    --- Left margin.
    ml: number?,
    --- Takes the node out of flow, at these offsets from its parent's edges.
    pin: UiPin?,
    --- 0 to 1.
    opacity: number?,
    --- The accessibility role.
    role: Role?,
    --- The accessible name.
    label: string?,
    --- A tooltip.
    help: string?,
    --- Where the tooltip shows. Defaults to below.
    helpSide: Side?,
    --- Keeps the tooltip showing while true, whatever the pointer does, as Desktop's `Tooltip forceOpen`: a Copy button's "Copied" for a moment after the press.
    helpOpen: boolean?,
    --- Which way the subtree reads, as HTML's `dir` does: `ltr` or `rtl` keeps a figure or a path in that order inside a window running the other way, `auto` takes a text's first strong direction (a name). On `ui.field` it is the value's direction, as on an `input`: `auto` starts an Arabic name at the right, `ltr` keeps an address or a host left to right. Defaults to the window's.
    dir: ("ltr" | "rtl" | "auto")?,
    --- The pointer's shape over the node and over what is inside it that sets none, by its CSS `cursor` name. Desktop shows the arrow on controls; set this only where its style sheet names another cursor, as the browser's address field does (`text`).
    cursor: ("default" | "pointer" | "text" | "grab" | "grabbing" | "not-allowed" | "move" | "col-resize" | "row-resize" | "ew-resize" | "ns-resize" | "nwse-resize" | "nesw-resize" | "crosshair" | "cell" | "zoom-in" | "zoom-out")?,
    --- The Markdown.
    source: string,
    --- Body font size in points. Defaults to 14.
    size: number?,
    --- Body line height in points.
    lineHeight: number?,
    --- Words to highlight, such as search matches.
    highlights: { string }?,
    --- The user can select and copy it. Defaults to true.
    selectable: boolean?,
    --- The source is still growing: the host keeps the last block open.
    streaming: boolean?,
}

--- `ui.virtualList` props. Native: windowing, the height cache and scroll anchoring stay in the host; Luau builds only the rows that come into view.
export type UiVirtualListProps = {
    --- Identity among siblings across renders, and the anchor key for popovers and menus.
    key: string?,
    --- Width.
    w: UiSize?,
    --- Height.
    h: UiSize?,
    --- Minimum width.
    minW: number?,
    --- Minimum height.
    minH: number?,
    --- Maximum width.
    maxW: number?,
    --- Maximum height.
    maxH: number?,
    --- Share of a stack's free space.
    grow: number?,
    --- May shrink below its content size.
    shrink: boolean?,
    --- Padding on every side.
    pad: number?,
    --- Left and right padding.
    px: number?,
    --- Top and bottom padding.
    py: number?,
    --- Top padding.
    pt: number?,
    --- Right padding.
    pr: number?,
    --- Bottom padding.
    pb: number?,
    --- Left padding.
    pl: number?,
    --- Margin on every side.
    m: number?,
    --- Left and right margin.
    mx: number?,
    --- Top and bottom margin.
    my: number?,
    --- Top margin.
    mt: number?,
    --- Right margin.
    mr: number?,
    --- Bottom margin.
    mb: number?,
    --- Left margin.
    ml: number?,
    --- Takes the node out of flow, at these offsets from its parent's edges.
    pin: UiPin?,
    --- 0 to 1.
    opacity: number?,
    --- The accessibility role.
    role: Role?,
    --- The accessible name.
    label: string?,
    --- A tooltip.
    help: string?,
    --- Where the tooltip shows. Defaults to below.
    helpSide: Side?,
    --- Keeps the tooltip showing while true, whatever the pointer does, as Desktop's `Tooltip forceOpen`: a Copy button's "Copied" for a moment after the press.
    helpOpen: boolean?,
    --- Which way the subtree reads, as HTML's `dir` does: `ltr` or `rtl` keeps a figure or a path in that order inside a window running the other way, `auto` takes a text's first strong direction (a name). On `ui.field` it is the value's direction, as on an `input`: `auto` starts an Arabic name at the right, `ltr` keeps an address or a host left to right. Defaults to the window's.
    dir: ("ltr" | "rtl" | "auto")?,
    --- The pointer's shape over the node and over what is inside it that sets none, by its CSS `cursor` name. Desktop shows the arrow on controls; set this only where its style sheet names another cursor, as the browser's address field does (`text`).
    cursor: ("default" | "pointer" | "text" | "grab" | "grabbing" | "not-allowed" | "move" | "col-resize" | "row-resize" | "ew-resize" | "ns-resize" | "nwse-resize" | "nesw-resize" | "crosshair" | "cell" | "zoom-in" | "zoom-out")?,
    --- How many rows.
    count: number,
    --- A row's height before it is measured.
    estimatedRowHeight: number,
    --- Change it when rows change. While it stays the same, scrolling builds only rows that enter the view.
    version: MemoVersion,
    --- Each row's own version, beside `version`: a row is built again when `version` or its own changes, so opening one row rebuilds that row alone.
    rowVersions: { MemoVersion }?,
    --- `false`: the list has no scroll of its own. It sits in a scrolling column (`scrollY = true` on an ancestor), under or among other content, and builds the rows that column shows. Defaults to `true`: the list is its own scroll.
    scrollY: boolean?,
    --- Builds row `index`, from 1. It must not yield.
    renderRow: (index: number) -> UiNode,
    --- A stable identity for row `index`, so rows keep their state as the list changes. Defaults to the index.
    rowKey: ((index: number) -> string)?,
    --- Space between rows.
    gap: number?,
    --- Runs as the visible rows change, with `event.first` and `event.last`.
    onVisibleRows: UiHandler?,
}

--- `ui.terminalSurface` props. Native: the VT parser, scrollback, glyph grid, IME and PTY stream stay in the host.
export type UiTerminalSurfaceProps = {
    --- Identity among siblings across renders, and the anchor key for popovers and menus.
    key: string?,
    --- Width.
    w: UiSize?,
    --- Height.
    h: UiSize?,
    --- Minimum width.
    minW: number?,
    --- Minimum height.
    minH: number?,
    --- Maximum width.
    maxW: number?,
    --- Maximum height.
    maxH: number?,
    --- Share of a stack's free space.
    grow: number?,
    --- May shrink below its content size.
    shrink: boolean?,
    --- Padding on every side.
    pad: number?,
    --- Left and right padding.
    px: number?,
    --- Top and bottom padding.
    py: number?,
    --- Top padding.
    pt: number?,
    --- Right padding.
    pr: number?,
    --- Bottom padding.
    pb: number?,
    --- Left padding.
    pl: number?,
    --- Margin on every side.
    m: number?,
    --- Left and right margin.
    mx: number?,
    --- Top and bottom margin.
    my: number?,
    --- Top margin.
    mt: number?,
    --- Right margin.
    mr: number?,
    --- Bottom margin.
    mb: number?,
    --- Left margin.
    ml: number?,
    --- Takes the node out of flow, at these offsets from its parent's edges.
    pin: UiPin?,
    --- 0 to 1.
    opacity: number?,
    --- The accessibility role.
    role: Role?,
    --- The accessible name.
    label: string?,
    --- A tooltip.
    help: string?,
    --- Where the tooltip shows. Defaults to below.
    helpSide: Side?,
    --- Keeps the tooltip showing while true, whatever the pointer does, as Desktop's `Tooltip forceOpen`: a Copy button's "Copied" for a moment after the press.
    helpOpen: boolean?,
    --- Which way the subtree reads, as HTML's `dir` does: `ltr` or `rtl` keeps a figure or a path in that order inside a window running the other way, `auto` takes a text's first strong direction (a name). On `ui.field` it is the value's direction, as on an `input`: `auto` starts an Arabic name at the right, `ltr` keeps an address or a host left to right. Defaults to the window's.
    dir: ("ltr" | "rtl" | "auto")?,
    --- The pointer's shape over the node and over what is inside it that sets none, by its CSS `cursor` name. Desktop shows the arrow on controls; set this only where its style sheet names another cursor, as the browser's address field does (`text`).
    cursor: ("default" | "pointer" | "text" | "grab" | "grabbing" | "not-allowed" | "move" | "col-resize" | "row-resize" | "ew-resize" | "ns-resize" | "nwse-resize" | "nesw-resize" | "crosshair" | "cell" | "zoom-in" | "zoom-out")?,
    --- The terminal session, from the `Terminal` service.
    session: PtySession?,
    --- A font family. Defaults to the user's code font.
    fontFamily: string?,
    --- Font size in points.
    fontSize: number?,
    --- Takes keyboard focus.
    focused: boolean?,
    --- The pane's width in points (`PanelProps.width`): the grid's columns fit it.
    width: number?,
    --- The pane's height in points: the grid's rows fit it.
    height: number?,
    --- The narrow window's Terminal view of session `sessionId`: its terminals' tabs over the active one.
    narrow: boolean?,
    --- The session a `narrow` view shows.
    sessionId: string?,
}

--- `ui.terminalTabLabel` props.
export type UiTerminalTabLabelProps = {
    --- Identity among siblings across renders, and the anchor key for popovers and menus.
    key: string?,
    --- Width.
    w: UiSize?,
    --- Height.
    h: UiSize?,
    --- Minimum width.
    minW: number?,
    --- Minimum height.
    minH: number?,
    --- Maximum width.
    maxW: number?,
    --- Maximum height.
    maxH: number?,
    --- Share of a stack's free space.
    grow: number?,
    --- May shrink below its content size.
    shrink: boolean?,
    --- Padding on every side.
    pad: number?,
    --- Left and right padding.
    px: number?,
    --- Top and bottom padding.
    py: number?,
    --- Top padding.
    pt: number?,
    --- Right padding.
    pr: number?,
    --- Bottom padding.
    pb: number?,
    --- Left padding.
    pl: number?,
    --- Margin on every side.
    m: number?,
    --- Left and right margin.
    mx: number?,
    --- Top and bottom margin.
    my: number?,
    --- Top margin.
    mt: number?,
    --- Right margin.
    mr: number?,
    --- Bottom margin.
    mb: number?,
    --- Left margin.
    ml: number?,
    --- Takes the node out of flow, at these offsets from its parent's edges.
    pin: UiPin?,
    --- 0 to 1.
    opacity: number?,
    --- The accessibility role.
    role: Role?,
    --- The accessible name.
    label: string?,
    --- A tooltip.
    help: string?,
    --- Where the tooltip shows. Defaults to below.
    helpSide: Side?,
    --- Keeps the tooltip showing while true, whatever the pointer does, as Desktop's `Tooltip forceOpen`: a Copy button's "Copied" for a moment after the press.
    helpOpen: boolean?,
    --- Which way the subtree reads, as HTML's `dir` does: `ltr` or `rtl` keeps a figure or a path in that order inside a window running the other way, `auto` takes a text's first strong direction (a name). On `ui.field` it is the value's direction, as on an `input`: `auto` starts an Arabic name at the right, `ltr` keeps an address or a host left to right. Defaults to the window's.
    dir: ("ltr" | "rtl" | "auto")?,
    --- The pointer's shape over the node and over what is inside it that sets none, by its CSS `cursor` name. Desktop shows the arrow on controls; set this only where its style sheet names another cursor, as the browser's address field does (`text`).
    cursor: ("default" | "pointer" | "text" | "grab" | "grabbing" | "not-allowed" | "move" | "col-resize" | "row-resize" | "ew-resize" | "ns-resize" | "nwse-resize" | "nesw-resize" | "crosshair" | "cell" | "zoom-in" | "zoom-out")?,
    --- The terminal the tab shows.
    session: PtySession,
    --- Its label.
    title: string,
    --- The tab is selected.
    selected: boolean?,
}

--- `ui.browserSurface` props. Native: the browser engine, its compositor, and input forwarding stay in the host.
export type UiBrowserSurfaceProps = {
    --- Identity among siblings across renders, and the anchor key for popovers and menus.
    key: string?,
    --- Width.
    w: UiSize?,
    --- Height.
    h: UiSize?,
    --- Minimum width.
    minW: number?,
    --- Minimum height.
    minH: number?,
    --- Maximum width.
    maxW: number?,
    --- Maximum height.
    maxH: number?,
    --- Share of a stack's free space.
    grow: number?,
    --- May shrink below its content size.
    shrink: boolean?,
    --- Padding on every side.
    pad: number?,
    --- Left and right padding.
    px: number?,
    --- Top and bottom padding.
    py: number?,
    --- Top padding.
    pt: number?,
    --- Right padding.
    pr: number?,
    --- Bottom padding.
    pb: number?,
    --- Left padding.
    pl: number?,
    --- Margin on every side.
    m: number?,
    --- Left and right margin.
    mx: number?,
    --- Top and bottom margin.
    my: number?,
    --- Top margin.
    mt: number?,
    --- Right margin.
    mr: number?,
    --- Bottom margin.
    mb: number?,
    --- Left margin.
    ml: number?,
    --- Takes the node out of flow, at these offsets from its parent's edges.
    pin: UiPin?,
    --- 0 to 1.
    opacity: number?,
    --- The accessibility role.
    role: Role?,
    --- The accessible name.
    label: string?,
    --- A tooltip.
    help: string?,
    --- Where the tooltip shows. Defaults to below.
    helpSide: Side?,
    --- Keeps the tooltip showing while true, whatever the pointer does, as Desktop's `Tooltip forceOpen`: a Copy button's "Copied" for a moment after the press.
    helpOpen: boolean?,
    --- Which way the subtree reads, as HTML's `dir` does: `ltr` or `rtl` keeps a figure or a path in that order inside a window running the other way, `auto` takes a text's first strong direction (a name). On `ui.field` it is the value's direction, as on an `input`: `auto` starts an Arabic name at the right, `ltr` keeps an address or a host left to right. Defaults to the window's.
    dir: ("ltr" | "rtl" | "auto")?,
    --- The pointer's shape over the node and over what is inside it that sets none, by its CSS `cursor` name. Desktop shows the arrow on controls; set this only where its style sheet names another cursor, as the browser's address field does (`text`).
    cursor: ("default" | "pointer" | "text" | "grab" | "grabbing" | "not-allowed" | "move" | "col-resize" | "row-resize" | "ew-resize" | "ns-resize" | "nwse-resize" | "nesw-resize" | "crosshair" | "cell" | "zoom-in" | "zoom-out")?,
    --- Available width of the device preview area.
    areaWidth: number?,
    --- Available height of the device preview area.
    areaHeight: number?,
    --- The page, from the `Browser` service. Until the engine runs, the surface shows the engine's state instead: its download, the download's progress, or why it failed.
    tab: BrowserTab,
    --- Takes keyboard focus.
    focused: boolean?,
}

--- Device dimensions in CSS pixels and optional display scale.
export type BrowserViewport = {
    --- Width from 1 to 4096.
    width: number,
    --- Height from 1 to 4096.
    height: number,
    --- desktop, phone, tablet or windows_tablet.
    profile: string,
    --- Device preset id.
    preset: string?,
    --- Display factor, or nil to fit the pane.
    scale: number?,
}

--- `ui.codeView` props. Native: highlighting, the line buffer, the gutter, selection and find stay in the host.
export type UiCodeViewProps = {
    --- Identity among siblings across renders, and the anchor key for popovers and menus.
    key: string?,
    --- Width.
    w: UiSize?,
    --- Height.
    h: UiSize?,
    --- Minimum width.
    minW: number?,
    --- Minimum height.
    minH: number?,
    --- Maximum width.
    maxW: number?,
    --- Maximum height.
    maxH: number?,
    --- Share of a stack's free space.
    grow: number?,
    --- May shrink below its content size.
    shrink: boolean?,
    --- Padding on every side.
    pad: number?,
    --- Left and right padding.
    px: number?,
    --- Top and bottom padding.
    py: number?,
    --- Top padding.
    pt: number?,
    --- Right padding.
    pr: number?,
    --- Bottom padding.
    pb: number?,
    --- Left padding.
    pl: number?,
    --- Margin on every side.
    m: number?,
    --- Left and right margin.
    mx: number?,
    --- Top and bottom margin.
    my: number?,
    --- Top margin.
    mt: number?,
    --- Right margin.
    mr: number?,
    --- Bottom margin.
    mb: number?,
    --- Left margin.
    ml: number?,
    --- Takes the node out of flow, at these offsets from its parent's edges.
    pin: UiPin?,
    --- 0 to 1.
    opacity: number?,
    --- The accessibility role.
    role: Role?,
    --- The accessible name.
    label: string?,
    --- A tooltip.
    help: string?,
    --- Where the tooltip shows. Defaults to below.
    helpSide: Side?,
    --- Keeps the tooltip showing while true, whatever the pointer does, as Desktop's `Tooltip forceOpen`: a Copy button's "Copied" for a moment after the press.
    helpOpen: boolean?,
    --- Which way the subtree reads, as HTML's `dir` does: `ltr` or `rtl` keeps a figure or a path in that order inside a window running the other way, `auto` takes a text's first strong direction (a name). On `ui.field` it is the value's direction, as on an `input`: `auto` starts an Arabic name at the right, `ltr` keeps an address or a host left to right. Defaults to the window's.
    dir: ("ltr" | "rtl" | "auto")?,
    --- The pointer's shape over the node and over what is inside it that sets none, by its CSS `cursor` name. Desktop shows the arrow on controls; set this only where its style sheet names another cursor, as the browser's address field does (`text`).
    cursor: ("default" | "pointer" | "text" | "grab" | "grabbing" | "not-allowed" | "move" | "col-resize" | "row-resize" | "ew-resize" | "ns-resize" | "nwse-resize" | "nesw-resize" | "crosshair" | "cell" | "zoom-in" | "zoom-out")?,
    --- The file, from `Workspace:OpenDocument`.
    doc: CodeDocument,
    --- Wraps long lines.
    lineWrap: boolean?,
    --- Shows the gutter. Defaults to true.
    showLineNumbers: boolean?,
    --- Scrolls to this line, from 1, and highlights it.
    revealLine: number?,
}

--- `ui.diffView` props. Native: hunk layout, intraline diffing, highlighting and comment anchors stay in the host.
export type UiDiffViewProps = {
    --- Identity among siblings across renders, and the anchor key for popovers and menus.
    key: string?,
    --- Width.
    w: UiSize?,
    --- Height.
    h: UiSize?,
    --- Minimum width.
    minW: number?,
    --- Minimum height.
    minH: number?,
    --- Maximum width.
    maxW: number?,
    --- Maximum height.
    maxH: number?,
    --- Share of a stack's free space.
    grow: number?,
    --- May shrink below its content size.
    shrink: boolean?,
    --- Padding on every side.
    pad: number?,
    --- Left and right padding.
    px: number?,
    --- Top and bottom padding.
    py: number?,
    --- Top padding.
    pt: number?,
    --- Right padding.
    pr: number?,
    --- Bottom padding.
    pb: number?,
    --- Left padding.
    pl: number?,
    --- Margin on every side.
    m: number?,
    --- Left and right margin.
    mx: number?,
    --- Top and bottom margin.
    my: number?,
    --- Top margin.
    mt: number?,
    --- Right margin.
    mr: number?,
    --- Bottom margin.
    mb: number?,
    --- Left margin.
    ml: number?,
    --- Takes the node out of flow, at these offsets from its parent's edges.
    pin: UiPin?,
    --- 0 to 1.
    opacity: number?,
    --- The accessibility role.
    role: Role?,
    --- The accessible name.
    label: string?,
    --- A tooltip.
    help: string?,
    --- Where the tooltip shows. Defaults to below.
    helpSide: Side?,
    --- Keeps the tooltip showing while true, whatever the pointer does, as Desktop's `Tooltip forceOpen`: a Copy button's "Copied" for a moment after the press.
    helpOpen: boolean?,
    --- Which way the subtree reads, as HTML's `dir` does: `ltr` or `rtl` keeps a figure or a path in that order inside a window running the other way, `auto` takes a text's first strong direction (a name). On `ui.field` it is the value's direction, as on an `input`: `auto` starts an Arabic name at the right, `ltr` keeps an address or a host left to right. Defaults to the window's.
    dir: ("ltr" | "rtl" | "auto")?,
    --- The pointer's shape over the node and over what is inside it that sets none, by its CSS `cursor` name. Desktop shows the arrow on controls; set this only where its style sheet names another cursor, as the browser's address field does (`text`).
    cursor: ("default" | "pointer" | "text" | "grab" | "grabbing" | "not-allowed" | "move" | "col-resize" | "row-resize" | "ew-resize" | "ns-resize" | "nwse-resize" | "nesw-resize" | "crosshair" | "cell" | "zoom-in" | "zoom-out")?,
    --- The changes, from `Diff:Open`.
    diff: DiffDocument,
    --- Split or unified.
    mode: DiffMode,
    --- Shows unchanged lines in full.
    expand: boolean?,
    --- Shows only this file's changes, under its header.
    file: string?,
    --- Scrolls to this line of the new side, from 1, once it shows.
    revealLine: number?,
    --- Every file as a row that opens to its unified diff, under a header with the totals and Expand all: the narrow Changes view. Its children sit at the header's start.
    list: boolean?,
}

--- `ui.fileTree` props. Native: tree flattening, file icons and keyboard navigation stay in the host. A secondary click on a row offers the apps that open it.
export type UiFileTreeProps = {
    --- Identity among siblings across renders, and the anchor key for popovers and menus.
    key: string?,
    --- Width.
    w: UiSize?,
    --- Height.
    h: UiSize?,
    --- Minimum width.
    minW: number?,
    --- Minimum height.
    minH: number?,
    --- Maximum width.
    maxW: number?,
    --- Maximum height.
    maxH: number?,
    --- Share of a stack's free space.
    grow: number?,
    --- May shrink below its content size.
    shrink: boolean?,
    --- Padding on every side.
    pad: number?,
    --- Left and right padding.
    px: number?,
    --- Top and bottom padding.
    py: number?,
    --- Top padding.
    pt: number?,
    --- Right padding.
    pr: number?,
    --- Bottom padding.
    pb: number?,
    --- Left padding.
    pl: number?,
    --- Margin on every side.
    m: number?,
    --- Left and right margin.
    mx: number?,
    --- Top and bottom margin.
    my: number?,
    --- Top margin.
    mt: number?,
    --- Right margin.
    mr: number?,
    --- Bottom margin.
    mb: number?,
    --- Left margin.
    ml: number?,
    --- Takes the node out of flow, at these offsets from its parent's edges.
    pin: UiPin?,
    --- 0 to 1.
    opacity: number?,
    --- The accessibility role.
    role: Role?,
    --- The accessible name.
    label: string?,
    --- A tooltip.
    help: string?,
    --- Where the tooltip shows. Defaults to below.
    helpSide: Side?,
    --- Keeps the tooltip showing while true, whatever the pointer does, as Desktop's `Tooltip forceOpen`: a Copy button's "Copied" for a moment after the press.
    helpOpen: boolean?,
    --- Which way the subtree reads, as HTML's `dir` does: `ltr` or `rtl` keeps a figure or a path in that order inside a window running the other way, `auto` takes a text's first strong direction (a name). On `ui.field` it is the value's direction, as on an `input`: `auto` starts an Arabic name at the right, `ltr` keeps an address or a host left to right. Defaults to the window's.
    dir: ("ltr" | "rtl" | "auto")?,
    --- The pointer's shape over the node and over what is inside it that sets none, by its CSS `cursor` name. Desktop shows the arrow on controls; set this only where its style sheet names another cursor, as the browser's address field does (`text`).
    cursor: ("default" | "pointer" | "text" | "grab" | "grabbing" | "not-allowed" | "move" | "col-resize" | "row-resize" | "ew-resize" | "ns-resize" | "nwse-resize" | "nesw-resize" | "crosshair" | "cell" | "zoom-in" | "zoom-out")?,
    --- The tree, from `Workspace:OpenTree`.
    tree: FileTreeModel,
    --- Shows only these workspace paths and the folders above them, all known at once: a change list rather than the folder's listing.
    allowed: { string }?,
    --- A change letter at the end of these paths' rows: `A`, `D` or `M`.
    kinds: { [string]: ChangeKind }?,
    --- Lists only the files whose path holds this text, flat, with their counts, instead of the tree. The first is highlighted.
    filter: string?,
    --- The selected row.
    selectedPath: string?,
    --- A row to highlight, such as the open file.
    highlightedPath: string?,
    --- Runs as the user picks a file, with `event.path`.
    onSelect: UiHandler?,
    --- Runs on a secondary click, with `event.path`, instead of the host's menu.
    onContextMenu: UiHandler?,
    --- A workspace tree as the narrow Files view: All files and the open files' tabs over the tree, or the open file.
    narrow: boolean?,
    --- The narrow Files view's width.
    width: number?,
}

--- `ui.panelSidebar(props, { sidebar, main })` props: a panel's frame with the file sidebar at its start or end, resized by its inner edge (a double click resets it), and the sidebar toggle pinned at the panel's top corner on the sidebar's side. Whether the sidebar shows, its width and the edge it sits on are one preference every panel shares (`Layout:PanelSidebar`); the toggle and Mod+\ flip it, and a right-click on the sidebar or the toggle offers Sidebar position.
export type UiPanelSidebarProps = {
    --- Identity among siblings across renders, and the anchor key for popovers and menus.
    key: string?,
    --- Width.
    w: UiSize?,
    --- Height.
    h: UiSize?,
    --- Minimum width.
    minW: number?,
    --- Minimum height.
    minH: number?,
    --- Maximum width.
    maxW: number?,
    --- Maximum height.
    maxH: number?,
    --- Share of a stack's free space.
    grow: number?,
    --- May shrink below its content size.
    shrink: boolean?,
    --- Padding on every side.
    pad: number?,
    --- Left and right padding.
    px: number?,
    --- Top and bottom padding.
    py: number?,
    --- Top padding.
    pt: number?,
    --- Right padding.
    pr: number?,
    --- Bottom padding.
    pb: number?,
    --- Left padding.
    pl: number?,
    --- Margin on every side.
    m: number?,
    --- Left and right margin.
    mx: number?,
    --- Top and bottom margin.
    my: number?,
    --- Top margin.
    mt: number?,
    --- Right margin.
    mr: number?,
    --- Bottom margin.
    mb: number?,
    --- Left margin.
    ml: number?,
    --- Takes the node out of flow, at these offsets from its parent's edges.
    pin: UiPin?,
    --- 0 to 1.
    opacity: number?,
    --- The accessibility role.
    role: Role?,
    --- The accessible name.
    label: string?,
    --- A tooltip.
    help: string?,
    --- Where the tooltip shows. Defaults to below.
    helpSide: Side?,
    --- Keeps the tooltip showing while true, whatever the pointer does, as Desktop's `Tooltip forceOpen`: a Copy button's "Copied" for a moment after the press.
    helpOpen: boolean?,
    --- Which way the subtree reads, as HTML's `dir` does: `ltr` or `rtl` keeps a figure or a path in that order inside a window running the other way, `auto` takes a text's first strong direction (a name). On `ui.field` it is the value's direction, as on an `input`: `auto` starts an Arabic name at the right, `ltr` keeps an address or a host left to right. Defaults to the window's.
    dir: ("ltr" | "rtl" | "auto")?,
    --- The pointer's shape over the node and over what is inside it that sets none, by its CSS `cursor` name. Desktop shows the arrow on controls; set this only where its style sheet names another cursor, as the browser's address field does (`text`).
    cursor: ("default" | "pointer" | "text" | "grab" | "grabbing" | "not-allowed" | "move" | "col-resize" | "row-resize" | "ew-resize" | "ns-resize" | "nwse-resize" | "nesw-resize" | "crosshair" | "cell" | "zoom-in" | "zoom-out")?,
    --- Prefixes its parts' keys: `<id>-sidebar`, `<id>-sidebar-resize`, `<id>-tree-toggle`.
    id: string,
    --- Its width until the user drags it. Defaults to 240.
    width: number?,
    --- The narrowest it gets. Defaults to 200.
    minWidth: number?,
    --- The widest it gets. Defaults to 480; give 60% of the panel's width when that is less.
    maxWidth: number?,
    --- Shows the sidebar however the user left it, its toggle disabled (Files' Open file launcher).
    locked: boolean?,
}

--- The panels' shared file sidebar.
export type PanelSidebarInfo = {
    --- It shows.
    opened: boolean,
    --- Its width in points.
    width: number,
    --- The edge of the panel it sits on. With `end`, the toggle is at the panel's top end corner: leave it room at the end of a header or toolbar under it, not at the start.
    side: PanelSidebarSide,
}

--- `ui.openInButton` props: the app's Open in split button. Its mark opens the path in the app the user chose last; its chevron lists the installed apps and Copy path. Nothing where the server's files aren't on this machine.
export type UiOpenInButtonProps = {
    --- Identity among siblings across renders, and the anchor key for popovers and menus.
    key: string?,
    --- Width.
    w: UiSize?,
    --- Height.
    h: UiSize?,
    --- Minimum width.
    minW: number?,
    --- Minimum height.
    minH: number?,
    --- Maximum width.
    maxW: number?,
    --- Maximum height.
    maxH: number?,
    --- Share of a stack's free space.
    grow: number?,
    --- May shrink below its content size.
    shrink: boolean?,
    --- Padding on every side.
    pad: number?,
    --- Left and right padding.
    px: number?,
    --- Top and bottom padding.
    py: number?,
    --- Top padding.
    pt: number?,
    --- Right padding.
    pr: number?,
    --- Bottom padding.
    pb: number?,
    --- Left padding.
    pl: number?,
    --- Margin on every side.
    m: number?,
    --- Left and right margin.
    mx: number?,
    --- Top and bottom margin.
    my: number?,
    --- Top margin.
    mt: number?,
    --- Right margin.
    mr: number?,
    --- Bottom margin.
    mb: number?,
    --- Left margin.
    ml: number?,
    --- Takes the node out of flow, at these offsets from its parent's edges.
    pin: UiPin?,
    --- 0 to 1.
    opacity: number?,
    --- The accessibility role.
    role: Role?,
    --- The accessible name.
    label: string?,
    --- A tooltip.
    help: string?,
    --- Where the tooltip shows. Defaults to below.
    helpSide: Side?,
    --- Keeps the tooltip showing while true, whatever the pointer does, as Desktop's `Tooltip forceOpen`: a Copy button's "Copied" for a moment after the press.
    helpOpen: boolean?,
    --- Which way the subtree reads, as HTML's `dir` does: `ltr` or `rtl` keeps a figure or a path in that order inside a window running the other way, `auto` takes a text's first strong direction (a name). On `ui.field` it is the value's direction, as on an `input`: `auto` starts an Arabic name at the right, `ltr` keeps an address or a host left to right. Defaults to the window's.
    dir: ("ltr" | "rtl" | "auto")?,
    --- The pointer's shape over the node and over what is inside it that sets none, by its CSS `cursor` name. Desktop shows the arrow on controls; set this only where its style sheet names another cursor, as the browser's address field does (`text`).
    cursor: ("default" | "pointer" | "text" | "grab" | "grabbing" | "not-allowed" | "move" | "col-resize" | "row-resize" | "ew-resize" | "ns-resize" | "nwse-resize" | "nesw-resize" | "crosshair" | "cell" | "zoom-in" | "zoom-out")?,
    --- A workspace path, or empty for the workspace folder.
    path: string,
}

--- `ui.officePages` props. Native: the page column, its scenes, near-viewport loading and the zoom stay in the host.
export type UiOfficePagesProps = {
    --- Identity among siblings across renders, and the anchor key for popovers and menus.
    key: string?,
    --- Width.
    w: UiSize?,
    --- Height.
    h: UiSize?,
    --- Minimum width.
    minW: number?,
    --- Minimum height.
    minH: number?,
    --- Maximum width.
    maxW: number?,
    --- Maximum height.
    maxH: number?,
    --- Share of a stack's free space.
    grow: number?,
    --- May shrink below its content size.
    shrink: boolean?,
    --- Padding on every side.
    pad: number?,
    --- Left and right padding.
    px: number?,
    --- Top and bottom padding.
    py: number?,
    --- Top padding.
    pt: number?,
    --- Right padding.
    pr: number?,
    --- Bottom padding.
    pb: number?,
    --- Left padding.
    pl: number?,
    --- Margin on every side.
    m: number?,
    --- Left and right margin.
    mx: number?,
    --- Top and bottom margin.
    my: number?,
    --- Top margin.
    mt: number?,
    --- Right margin.
    mr: number?,
    --- Bottom margin.
    mb: number?,
    --- Left margin.
    ml: number?,
    --- Takes the node out of flow, at these offsets from its parent's edges.
    pin: UiPin?,
    --- 0 to 1.
    opacity: number?,
    --- The accessibility role.
    role: Role?,
    --- The accessible name.
    label: string?,
    --- A tooltip.
    help: string?,
    --- Where the tooltip shows. Defaults to below.
    helpSide: Side?,
    --- Keeps the tooltip showing while true, whatever the pointer does, as Desktop's `Tooltip forceOpen`: a Copy button's "Copied" for a moment after the press.
    helpOpen: boolean?,
    --- Which way the subtree reads, as HTML's `dir` does: `ltr` or `rtl` keeps a figure or a path in that order inside a window running the other way, `auto` takes a text's first strong direction (a name). On `ui.field` it is the value's direction, as on an `input`: `auto` starts an Arabic name at the right, `ltr` keeps an address or a host left to right. Defaults to the window's.
    dir: ("ltr" | "rtl" | "auto")?,
    --- The pointer's shape over the node and over what is inside it that sets none, by its CSS `cursor` name. Desktop shows the arrow on controls; set this only where its style sheet names another cursor, as the browser's address field does (`text`).
    cursor: ("default" | "pointer" | "text" | "grab" | "grabbing" | "not-allowed" | "move" | "col-resize" | "row-resize" | "ew-resize" | "ns-resize" | "nwse-resize" | "nesw-resize" | "crosshair" | "cell" | "zoom-in" | "zoom-out")?,
    --- A Word document or a deck, from `Office:OpenAsync`.
    document: OfficeDocument,
    --- The words it shows.
    labels: OfficePagesLabels,
}

--- `ui.officeSheet` props. Native: frames of the sheet as it scrolls, its headers, the selection and copying stay in the host.
export type UiOfficeSheetProps = {
    --- Identity among siblings across renders, and the anchor key for popovers and menus.
    key: string?,
    --- Width.
    w: UiSize?,
    --- Height.
    h: UiSize?,
    --- Minimum width.
    minW: number?,
    --- Minimum height.
    minH: number?,
    --- Maximum width.
    maxW: number?,
    --- Maximum height.
    maxH: number?,
    --- Share of a stack's free space.
    grow: number?,
    --- May shrink below its content size.
    shrink: boolean?,
    --- Padding on every side.
    pad: number?,
    --- Left and right padding.
    px: number?,
    --- Top and bottom padding.
    py: number?,
    --- Top padding.
    pt: number?,
    --- Right padding.
    pr: number?,
    --- Bottom padding.
    pb: number?,
    --- Left padding.
    pl: number?,
    --- Margin on every side.
    m: number?,
    --- Left and right margin.
    mx: number?,
    --- Top and bottom margin.
    my: number?,
    --- Top margin.
    mt: number?,
    --- Right margin.
    mr: number?,
    --- Bottom margin.
    mb: number?,
    --- Left margin.
    ml: number?,
    --- Takes the node out of flow, at these offsets from its parent's edges.
    pin: UiPin?,
    --- 0 to 1.
    opacity: number?,
    --- The accessibility role.
    role: Role?,
    --- The accessible name.
    label: string?,
    --- A tooltip.
    help: string?,
    --- Where the tooltip shows. Defaults to below.
    helpSide: Side?,
    --- Keeps the tooltip showing while true, whatever the pointer does, as Desktop's `Tooltip forceOpen`: a Copy button's "Copied" for a moment after the press.
    helpOpen: boolean?,
    --- Which way the subtree reads, as HTML's `dir` does: `ltr` or `rtl` keeps a figure or a path in that order inside a window running the other way, `auto` takes a text's first strong direction (a name). On `ui.field` it is the value's direction, as on an `input`: `auto` starts an Arabic name at the right, `ltr` keeps an address or a host left to right. Defaults to the window's.
    dir: ("ltr" | "rtl" | "auto")?,
    --- The pointer's shape over the node and over what is inside it that sets none, by its CSS `cursor` name. Desktop shows the arrow on controls; set this only where its style sheet names another cursor, as the browser's address field does (`text`).
    cursor: ("default" | "pointer" | "text" | "grab" | "grabbing" | "not-allowed" | "move" | "col-resize" | "row-resize" | "ew-resize" | "ns-resize" | "nwse-resize" | "nesw-resize" | "crosshair" | "cell" | "zoom-in" | "zoom-out")?,
    --- A workbook, from `Office:OpenAsync`; it shows the sheet `Office:SetSheet` chose.
    document: OfficeDocument,
    --- The words the grid says, in the plugin's language.
    labels: OfficeSheetLabels?,
}

--- `ui.genUi` props. Native: the outline parser and its animated widgets stay in the host.
export type UiGenUiProps = {
    --- Identity among siblings across renders, and the anchor key for popovers and menus.
    key: string?,
    --- Width.
    w: UiSize?,
    --- Height.
    h: UiSize?,
    --- Minimum width.
    minW: number?,
    --- Minimum height.
    minH: number?,
    --- Maximum width.
    maxW: number?,
    --- Maximum height.
    maxH: number?,
    --- Share of a stack's free space.
    grow: number?,
    --- May shrink below its content size.
    shrink: boolean?,
    --- Padding on every side.
    pad: number?,
    --- Left and right padding.
    px: number?,
    --- Top and bottom padding.
    py: number?,
    --- Top padding.
    pt: number?,
    --- Right padding.
    pr: number?,
    --- Bottom padding.
    pb: number?,
    --- Left padding.
    pl: number?,
    --- Margin on every side.
    m: number?,
    --- Left and right margin.
    mx: number?,
    --- Top and bottom margin.
    my: number?,
    --- Top margin.
    mt: number?,
    --- Right margin.
    mr: number?,
    --- Bottom margin.
    mb: number?,
    --- Left margin.
    ml: number?,
    --- Takes the node out of flow, at these offsets from its parent's edges.
    pin: UiPin?,
    --- 0 to 1.
    opacity: number?,
    --- The accessibility role.
    role: Role?,
    --- The accessible name.
    label: string?,
    --- A tooltip.
    help: string?,
    --- Where the tooltip shows. Defaults to below.
    helpSide: Side?,
    --- Keeps the tooltip showing while true, whatever the pointer does, as Desktop's `Tooltip forceOpen`: a Copy button's "Copied" for a moment after the press.
    helpOpen: boolean?,
    --- Which way the subtree reads, as HTML's `dir` does: `ltr` or `rtl` keeps a figure or a path in that order inside a window running the other way, `auto` takes a text's first strong direction (a name). On `ui.field` it is the value's direction, as on an `input`: `auto` starts an Arabic name at the right, `ltr` keeps an address or a host left to right. Defaults to the window's.
    dir: ("ltr" | "rtl" | "auto")?,
    --- The pointer's shape over the node and over what is inside it that sets none, by its CSS `cursor` name. Desktop shows the arrow on controls; set this only where its style sheet names another cursor, as the browser's address field does (`text`).
    cursor: ("default" | "pointer" | "text" | "grab" | "grabbing" | "not-allowed" | "move" | "col-resize" | "row-resize" | "ew-resize" | "ns-resize" | "nwse-resize" | "nesw-resize" | "crosshair" | "cell" | "zoom-in" | "zoom-out")?,
    --- The ```` ```ui ```` outline source.
    source: string,
    --- The source is still growing.
    streaming: boolean?,
}

--- `ui.progressCircle` props: the app's progress ring.
export type UiProgressCircleProps = {
    --- Identity among siblings across renders, and the anchor key for popovers and menus.
    key: string?,
    --- Width.
    w: UiSize?,
    --- Height.
    h: UiSize?,
    --- Minimum width.
    minW: number?,
    --- Minimum height.
    minH: number?,
    --- Maximum width.
    maxW: number?,
    --- Maximum height.
    maxH: number?,
    --- Share of a stack's free space.
    grow: number?,
    --- May shrink below its content size.
    shrink: boolean?,
    --- Padding on every side.
    pad: number?,
    --- Left and right padding.
    px: number?,
    --- Top and bottom padding.
    py: number?,
    --- Top padding.
    pt: number?,
    --- Right padding.
    pr: number?,
    --- Bottom padding.
    pb: number?,
    --- Left padding.
    pl: number?,
    --- Margin on every side.
    m: number?,
    --- Left and right margin.
    mx: number?,
    --- Top and bottom margin.
    my: number?,
    --- Top margin.
    mt: number?,
    --- Right margin.
    mr: number?,
    --- Bottom margin.
    mb: number?,
    --- Left margin.
    ml: number?,
    --- Takes the node out of flow, at these offsets from its parent's edges.
    pin: UiPin?,
    --- 0 to 1.
    opacity: number?,
    --- The accessibility role.
    role: Role?,
    --- The accessible name.
    label: string?,
    --- A tooltip.
    help: string?,
    --- Where the tooltip shows. Defaults to below.
    helpSide: Side?,
    --- Keeps the tooltip showing while true, whatever the pointer does, as Desktop's `Tooltip forceOpen`: a Copy button's "Copied" for a moment after the press.
    helpOpen: boolean?,
    --- Which way the subtree reads, as HTML's `dir` does: `ltr` or `rtl` keeps a figure or a path in that order inside a window running the other way, `auto` takes a text's first strong direction (a name). On `ui.field` it is the value's direction, as on an `input`: `auto` starts an Arabic name at the right, `ltr` keeps an address or a host left to right. Defaults to the window's.
    dir: ("ltr" | "rtl" | "auto")?,
    --- The pointer's shape over the node and over what is inside it that sets none, by its CSS `cursor` name. Desktop shows the arrow on controls; set this only where its style sheet names another cursor, as the browser's address field does (`text`).
    cursor: ("default" | "pointer" | "text" | "grab" | "grabbing" | "not-allowed" | "move" | "col-resize" | "row-resize" | "ew-resize" | "ns-resize" | "nwse-resize" | "nesw-resize" | "crosshair" | "cell" | "zoom-in" | "zoom-out")?,
    --- How full, 0 to 100. The arc runs clockwise from twelve o'clock.
    percentage: number,
    --- Its look. Defaults to `compact`.
    appearance: ProgressAppearance?,
    --- Width and height in points. Defaults to the appearance's own: 14 or 16.
    size: number?,
    --- The stroke in the ring's own units. Defaults to 1.5 for `compact`, 3 for `indicator`.
    strokeWidth: number?,
    --- The track's color. Defaults to `surface-4`.
    track: Color?,
    --- `indicator` only: a color over the track. Defaults to none.
    overlay: Color?,
    --- The arc's color. Defaults to `icon-base`.
    progress: Color?,
}

--- The interface language.
export type LocaleInfo = {
    --- The app's id for it, such as `"de"` or `"zht"`: the name of its catalog file, `locales/<id>.json`.
    id: string,
    --- Its BCP 47 tag, such as `"de-DE"` or `"zh-Hant"`.
    tag: string,
    --- Its name in itself, such as `"Deutsch"`.
    name: string,
    --- Which way it reads.
    direction: TextDirection,
}

--- How `I18n:Number` writes a number.
export type NumberFormat = {
    --- Defaults to `decimal`.
    style: NumberStyle?,
    --- Defaults to `standard`.
    notation: NumberNotation?,
    --- An ISO 4217 code for `currency`. Defaults to `"USD"`.
    currency: string?,
    --- The most fraction digits. Defaults to 3 for `decimal`, 2 for `currency`, 0 for `percent` and `compact`.
    maxFraction: number?,
    --- For `currency`, formats with the interface language's localized currency pattern (such as `1.234,50 $` or `1,84 USD`) instead of prefixing the narrow currency symbol. Defaults to false.
    localePattern: boolean?,
}

--- A session's token counts at its last step.
export type SessionUsage = {
    --- Input tokens.
    input: number,
    --- Output tokens.
    output: number,
    --- Reasoning tokens.
    reasoning: number,
    --- Tokens read from the cache.
    cacheRead: number,
    --- Tokens written to the cache.
    cacheWrite: number,
}

--- A session as the app holds it.
export type Session = {
    --- The session id.
    id: string,
    --- The key of the server that owns it: a `ServerInfo.key`, `local` for the app's own server.
    serverId: string,
    --- Its workspace folder.
    directory: string,
    --- Its title.
    title: string,
    --- The parent session of a subagent's session.
    parentId: string?,
    --- The project it belongs to. Nil for a folder outside any project.
    projectId: string?,
    --- The agent it runs with, such as `build`.
    agent: string?,
    --- The model variant it runs with, such as `high`. Nil for the model's default.
    variant: string?,
    --- A turn is running.
    busy: boolean,
    --- Changes whenever anything here does: put it in a memo's version.
    version: number,
    --- Changes when its newest messages or its inbox's waiting input do, not as older ones are read or the reader scrolls: the version for a read of its newest messages.
    messagesVersion: number,
    --- What it cost in USD, its subagents' sessions included.
    cost: number,
    --- Token counts at the last step with tokens.
    usage: SessionUsage?,
    --- The provider of the step `usage` came from.
    usageProviderId: string?,
    --- The model of the step `usage` came from.
    usageModelId: string?,
    --- The id of the message `usage` came from: the last step with tokens after the last finished compaction and before a staged revert.
    usageMessageId: string?,
    --- The oldest message Desktop's timeline would hold: its first read on opening the session, a page of 20 per approach of the top since, and every message after. `usage` and the Context tab's counts come from these messages, as Desktop's do. Empty while that is not known yet; nil before this window opened the session (every message counts).
    windowStart: string?,
    --- When the message `usage` came from was created, in milliseconds since the Unix epoch.
    usageCreated: number?,
    --- The provider of the model the session runs with.
    providerId: string?,
    --- The model the session runs with.
    modelId: string?,
    --- When it was created, in milliseconds since the Unix epoch.
    created: number?,
    --- When it last changed, in milliseconds since the Unix epoch.
    updated: number?,
    --- How many messages the app holds for it. Nil before it has read them.
    messages: number?,
    --- The HTTP status code of the session's last assistant message failure, when it failed with one (such as 402 or 429).
    lastErrorStatus: number?,
}

--- A model as the server's catalog names it.
export type ModelInfo = {
    --- The provider's name. Nil when the catalog does not list the provider.
    providerName: string?,
    --- The model's name. Nil when the catalog does not list it.
    modelName: string?,
    --- Its context window in tokens.
    contextLimit: number?,
    --- Its output limit in tokens.
    outputLimit: number?,
    --- Any of its token prices is above zero.
    paid: boolean?,
}

--- An entry in the host's model catalog (`Models:List`).
export type ModelEntry = {
    --- The provider id.
    providerId: string,
    --- The model id.
    modelId: string,
    --- Its display name.
    name: string,
    --- The provider's display name.
    providerName: string?,
    --- The provider's integration id, when connected through an integration.
    integration: string?,
    --- Whether the model is marked free.
    free: boolean,
    --- Whether any of its token prices is above zero.
    paid: boolean,
}

--- A short inline annotation a plugin attaches to a model row in the model picker (`Models:SetBadges`).
export type ModelBadge = {
    --- The provider id.
    providerId: string,
    --- The model id.
    modelId: string,
    --- Short text shown beside the model name and in its effort flyout.
    label: string,
    --- Emphasizes the annotation in danger ink (for example, a reached limit or blocked model).
    critical: boolean?,
}

--- The model selected in the composer.
export type ComposerModel = {
    --- The provider id.
    providerId: string,
    --- The model id.
    modelId: string,
    --- Its display name.
    name: string,
}

--- A cached server read.
export type Resource = {
    --- Its state.
    status: ResourceStatus,
    --- The answer as JSON, when it is JSON and the read didn't ask for `items`.
    json: any?,
    --- For a read that asked for them, the items its answer lists (`{ "data": [...] }` or `[...]`), split off the UI thread; `json` is the rest of the answer then.
    items: {ResourceItem}?,
    --- The answer as text.
    text: string?,
    --- Why it failed.
    error: string?,
    --- Changes with every new answer: put it in a memo's version.
    generation: number,
}

--- One item a read's answer lists.
export type ResourceItem = {
    --- Its `id`.
    id: string,
    --- Its `type`.
    kind: string,
    --- Its `time.created`.
    created: number?,
    --- Its JSON.
    text: string,
}

--- A server's answer.
export type ServerResponse = {
    --- A 2xx status.
    ok: boolean,
    --- The HTTP status. 0 when nothing answered.
    status: number,
    --- The body as JSON, when it is JSON.
    json: any?,
    --- The body as text.
    text: string,
    --- Why it failed.
    error: string?,
}

--- One event from the server's event stream.
export type ServerEvent = {
    --- The event type, such as `"session.updated"`.
    type: string,
    --- The event's payload.
    data: any,
    --- The key of the server that sent it: a `ServerInfo.key`, `local` for the app's own server.
    serverId: string,
    --- The workspace folder it concerns.
    directory: string?,
}

--- A server the app lists.
export type ServerInfo = {
    --- Its key: `local` for the app's own server, `<plugin>:<id>` for a contributed server, or the address of a server added by address.
    key: string,
    --- Its display name.
    name: string,
    --- Its badge, such as `"SSH"`.
    label: string?,
    --- Where it stands.
    state: ServerState,
    --- It runs on this machine.
    isLocal: boolean,
    --- This window works with it now: the server of the session or draft it shows, or of its Home.
    current: boolean,
    --- The version it reported.
    version: string?,
}

--- A server's latest health check.
export type ServerHealth = {
    --- It answered.
    healthy: boolean,
    --- The version it reported.
    version: string?,
    --- This app does not support that version.
    incompatible: boolean?,
    --- A check is in flight.
    checking: boolean?,
}

--- How to reach a ready server.
export type ServerEndpoint = {
    --- The base URL.
    url: string,
    --- The HTTP basic password. The user name is always `opencode`.
    password: string?,
}

--- One server a server source lists. Its key is `<plugin>:<id>`.
export type ServerEntry = {
    --- The entry's id within the source.
    id: string,
    --- The display name.
    name: string,
    --- A short badge after the name, such as `"WSL"`.
    label: string?,
    --- Where it stands.
    state: ServerState,
    --- False keeps it out of the app's server list. Settings still shows it. Defaults to true.
    listed: boolean?,
    --- The plugin keeps it reachable, as an SSH tunnel does: while the app checks a new endpoint, it shows a connecting spinner, not an unknown dot. Defaults to false.
    managed: boolean?,
    --- Its endpoint, once `ready`.
    http: ServerEndpoint?,
}

--- HTTP basic credentials.
export type BasicAuth = {
    --- The user name.
    user: string,
    --- The password.
    password: string,
}

--- An HTTP request.
export type HttpRequest = {
    --- The URL.
    url: string,
    --- Defaults to GET.
    method: HttpMethod?,
    --- Request headers.
    headers: { [string]: string }?,
    --- A text body.
    body: string?,
    --- A JSON body, instead of `body`. Sets the content type.
    json: any?,
    --- Sends `Authorization: Basic`.
    basicAuth: BasicAuth?,
    --- Seconds before it gives up. Defaults to 30.
    timeout: number?,
}

--- An HTTP answer.
export type HttpResponse = {
    --- A 2xx status.
    ok: boolean,
    --- The HTTP status. 0 when nothing answered.
    status: number,
    --- Response headers, with lowercase names.
    headers: { [string]: string },
    --- The body as text.
    text: string,
    --- The body as JSON, when it is JSON.
    json: any?,
    --- Why it failed.
    error: string?,
}

--- A process to start.
export type ProcessSpec = {
    --- The program, as a path or a name on PATH.
    program: string,
    --- Its arguments, passed as they are, without a shell.
    args: { string }?,
    --- Variables to add to the app's environment.
    env: { [string]: string }?,
    --- Its working folder.
    cwd: string?,
    --- Written to its stdin first. Stdin then closes unless `keepStdin` is set.
    stdin: string?,
    --- Keeps stdin open for `Write`.
    keepStdin: boolean?,
    --- Seconds before the host kills it.
    timeout: number?,
    --- How it encodes its output. Defaults to UTF-8, or UTF-16LE when its first output reads as that.
    encoding: TextEncoding?,
    --- Sends stderr to a file rather than a pipe, read back as one `stderr` chunk as it exits: for programs that never exit after writing to a stderr pipe, as Windows' OpenSSH does after a failure.
    stderrFile: boolean?,
}

--- How a process ended.
export type ProcessExit = {
    --- Its exit code, when it has one.
    code: number?,
    --- Why it could not start.
    error: string?,
    --- Its timeout killed it.
    timedOut: boolean,
    --- `Kill`, or the plugin going away, ended it.
    killed: boolean,
}

--- A finished process and all its output.
export type ProcessOutput = {
    --- It ran and exited with 0.
    ok: boolean,
    --- Its exit code.
    code: number?,
    --- Everything it wrote to stdout.
    stdout: string,
    --- Everything it wrote to stderr.
    stderr: string,
    --- Why it could not start.
    error: string?,
    --- Its timeout killed it.
    timedOut: boolean,
}

--- A process the host supervises. The host kills it when the plugin half that started it goes away.
export type ProcessHandle = {
    --- The host's id for it.
    Id: number,
    --- It has not exited.
    Running: (self: ProcessHandle) -> boolean,
    --- Writes to its stdin, then closes stdin when `close` is true. False when stdin is closed.
    Write: (self: ProcessHandle, text: string, close: boolean?) -> boolean,
    --- Kills it. `Exited` follows.
    Kill: (self: ProcessHandle) -> (),
    --- Yields until it exits. Yields: call it from a handler, a task or a signal callback.
    WaitAsync: (self: ProcessHandle) -> ProcessExit,
    --- Fires with each chunk of output, line-buffered.
    Output: Signal<ProcessStream, string>,
    --- Fires once as it exits.
    Exited: Signal<ProcessExit>,
}

--- A value the host saves for this plugin with the app's state.
export type StoreValue<T> = {
    --- The value, or the default.
    Get: (self: StoreValue<T>) -> T,
    --- Saves a new value. It must be plain data: nil, booleans, numbers, strings and tables of them. Setting the value it already holds writes nothing.
    Set: (self: StoreValue<T>, value: T) -> (),
    --- Changes with every `Set`: put it in a memo's version.
    Version: (self: StoreValue<T>) -> number,
    --- Fires with the new value, also when another window sets it.
    Changed: Signal<T>,
}

--- A value the host saves for this plugin per session. Each keeps the 50 sessions used last (read or set): a 51st session's value replaces the one used longest ago.
export type SessionStoreValue<T> = {
    --- The session's value, or the default.
    Get: (self: SessionStoreValue<T>, sessionId: string) -> T,
    --- Saves a new value for the session. It must be plain data. Setting the value it already holds writes nothing.
    Set: (self: SessionStoreValue<T>, sessionId: string, value: T) -> (),
    --- Changes with every `Set` for any session: put it in a memo's version.
    Version: (self: SessionStoreValue<T>) -> number,
    --- Fires with the session id and its new value.
    Changed: Signal<string, T>,
}

--- A button on a toast.
export type ToastAction = {
    --- The button's label.
    label: string,
    --- Shows this path in the system's file manager.
    revealPath: string?,
    --- Opens this URL in the system's browser.
    openUrl: string?,
    --- Runs when the user clicks it. The toast then closes.
    run: (() -> ())?,
    --- Its look: `primary` (the default) or `secondary`.
    variant: ToastActionVariant?,
}

--- What a toast shows.
export type ToastSpec = {
    --- The headline.
    title: string,
    --- More detail under the title.
    description: string?,
    --- The tone. Defaults to `default`.
    variant: ToastVariant?,
    --- An icon before the title.
    icon: IconName?,
    --- Seconds before it closes. Ignored when `persistent`. Defaults to 5.
    duration: number?,
    --- Stays until the user or `Dismiss` closes it.
    persistent: boolean?,
    --- Buttons, in order.
    actions: { ToastAction }?,
}

--- One toast the plugin showed. It closes when the plugin goes away.
export type ToastHandle = {
    --- Closes it. Does nothing once it closed.
    Dismiss: (self: ToastHandle) -> (),
    --- Replaces what it shows, in place.
    Update: (self: ToastHandle, spec: ToastSpec) -> (),
}

--- Where to open one of this plugin's popovers.
export type PopoverOpenSpec = {
    --- The popover's id, from `ctx:AddPopover`.
    id: string,
    --- The `key` of the element it opens beside.
    anchorKey: string,
    --- The `key` of the button that opens it, which takes focus back when it closes (Kobalte's `onCloseAutoFocus`), however it was opened. Defaults to what had focus when it opened.
    triggerKey: string?,
    --- The anchor's side. Defaults to below.
    side: Side?,
    --- Alignment along that side. Defaults to start.
    align: Align?,
    --- Points between the anchor and the popover.
    gap: number?,
    --- Passed to the popover's render as `props.data`.
    data: { [string]: any }?,
    --- How it appears.
    transition: Transition?,
}

--- One entry of a menu.
export type MenuEntry = {
    --- Defaults to `item`.
    kind: MenuEntryKind?,
    --- An item's id, returned when the user picks it.
    id: string?,
    --- An item's label, or a group's title.
    title: string?,
    --- An item's icon.
    icon: IconName?,
    --- Muted text at an item's end.
    detail: string?,
    --- A published command id whose shortcut the item shows.
    keybind: string?,
    --- A literal shortcut the item shows, such as `"mod+plus"`, when no command has it.
    keys: string?,
    --- False shows the item disabled. Defaults to true.
    enabled: boolean?,
    --- A destructive item.
    danger: boolean?,
    --- Shows a check mark.
    checked: boolean?,
}

--- A menu to open beside an element.
export type MenuOpenSpec = {
    --- The `key` of the element it opens beside.
    anchorKey: string,
    --- Its entries.
    items: { MenuEntry },
    --- Its accessible name.
    label: string?,
    --- The anchor's side. Defaults to below.
    side: Side?,
    --- Alignment along that side. Defaults to start.
    align: Align?,
    --- Its minimum width in points.
    minWidth: number?,
    --- Its width in points.
    width: number?,
}

--- A new session's draft, opened with `Layout:OpenDraft`.
export type DraftSpec = {
    --- The server it runs on: a `ServerInfo.key`. Defaults to the server this window works with.
    serverKey: string?,
    --- Its workspace folder on that server.
    directory: string,
    --- Text typed into its composer.
    prompt: string?,
}

--- How to open a panel tab.
export type OpenPanelOptions = {
    --- The session. Defaults to the routed session.
    sessionId: string?,
    --- The tab of an instance panel.
    instance: string?,
    --- Opens the tab without selecting it or switching the narrow-screen view.
    background: boolean?,
    --- How the side region opens.
    transition: Transition?,
}

--- A local link the app routes to a link handler.
export type Link = {
    --- The target: a path, a URL or a `file://` link.
    href: string,
    --- The plugin that produced the linked item.
    origin: string?,
    --- The path is a known workspace file, not a guess from text.
    exact: boolean?,
    --- Must not switch the narrow-screen view.
    background: boolean?,
    --- The workspace-relative folder the link was written in.
    base: string?,
    --- The session the link belongs to.
    sessionId: string?,
}

--- A system notification. It follows the user's notification settings.
export type NotificationSpec = {
    --- The title.
    title: string,
    --- The text under it.
    body: string?,
    --- Clicking it opens this session.
    sessionId: string?,
    --- Plays the user's notification sound.
    sound: boolean?,
}

--- The window's theme.
export type ThemeInfo = {
    --- The theme's key.
    id: string,
    --- Its display name.
    name: string,
    --- The color scheme in use.
    scheme: ColorScheme,
}

--- Where a file was saved.
export type SaveResult = {
    --- Its path.
    path: string?,
    --- Why it could not be saved.
    error: string?,
}

--- A file's text.
export type ReadResult = {
    --- Its text, as UTF-8.
    text: string?,
    --- Why it could not be read.
    error: string?,
}

--- An app that can open a path, such as an editor or a file manager.
export type OpenInApp = {
    --- Its id, for `Workspace:OpenIn`.
    id: string,
    --- Its display name.
    name: string,
    --- Its icon.
    icon: ImageId?,
}

--- A file open for `ui.codeView`. The host closes it when the plugin half goes away.
export type CodeDocument = {
    --- Its path.
    Path: string,
    --- Changes when its content does: put it in a memo's version.
    Version: (self: CodeDocument) -> number,
    --- The file no longer exists.
    Missing: (self: CodeDocument) -> boolean,
    --- The file is not text.
    Binary: (self: CodeDocument) -> boolean,
    --- Reads it again.
    Reload: (self: CodeDocument) -> (),
    --- Releases it.
    Close: (self: CodeDocument) -> (),
    --- Fires when its content changes on disk.
    Changed: Signal<()>,
}

--- A folder's tree for `ui.fileTree`.
export type FileTreeModel = {
    --- The root folder.
    Root: string,
    --- Expands a folder.
    Expand: (self: FileTreeModel, path: string) -> (),
    --- Collapses a folder.
    Collapse: (self: FileTreeModel, path: string) -> (),
    --- Shows only paths that match a fuzzy query. An empty query shows all.
    SetFilter: (self: FileTreeModel, query: string) -> (),
    --- The files whose path holds `query`, best first: what `ui.fileTree`'s `filter` lists for it.
    Find: (self: FileTreeModel, query: string) -> { string },
    --- Expands the folders above a path and scrolls its row into view.
    Reveal: (self: FileTreeModel, path: string) -> (),
    --- Reads the folder again.
    Refresh: (self: FileTreeModel) -> (),
    --- Changes when the tree does.
    Version: (self: FileTreeModel) -> number,
    --- Releases it.
    Close: (self: FileTreeModel) -> (),
    --- Fires when the tree changes.
    Changed: Signal<()>,
}

--- A diff's size.
export type DiffTotals = {
    --- Changed files.
    files: number,
    --- Added lines.
    added: number,
    --- Removed lines.
    removed: number,
}

--- One changed file.
export type DiffFile = {
    --- Its path.
    path: string,
    --- `added`, `modified`, `deleted` or `renamed`.
    status: string,
    --- Added lines.
    added: number,
    --- Removed lines.
    removed: number,
}

--- Which changes to open.
export type DiffOpenSpec = {
    --- The session.
    sessionId: string,
    --- What to compare.
    source: DiffSource,
    --- A `turn` source's user message.
    messageId: string?,
}

--- A set of changes for `ui.diffView`.
export type DiffDocument = {
    --- Its size.
    Totals: (self: DiffDocument) -> DiffTotals,
    --- Its files.
    Files: (self: DiffDocument) -> { DiffFile },
    --- Compares something else.
    SetSource: (self: DiffDocument, source: DiffSource, messageId: string?) -> (),
    --- What it compares.
    Source: (self: DiffDocument) -> DiffSource,
    --- What it can compare in this folder: `branch` only on a branch other than the default one.
    Sources: (self: DiffDocument) -> { DiffSource },
    --- How the read went.
    Status: (self: DiffDocument) -> DiffStatus,
    --- Reads again after a failure.
    Retry: (self: DiffDocument) -> (),
    --- Why the read failed, in the app's words, while `Status` is `error` or `offline`.
    Error: (self: DiffDocument) -> string?,
    --- The file the preview shows: the one focused last while it still changed, else the first the server lists.
    Active: (self: DiffDocument) -> string?,
    --- Shows a changed file in the preview, and selects it in the tree.
    Focus: (self: DiffDocument, path: string) -> (),
    --- Its changed files as a tree for `ui.fileTree`, every folder open until the user shuts it.
    Tree: (self: DiffDocument) -> FileTreeModel,
    --- A read is in flight.
    Loading: (self: DiffDocument) -> boolean,
    --- Changes when the changes do.
    Version: (self: DiffDocument) -> number,
    --- Releases it.
    Close: (self: DiffDocument) -> (),
    --- Fires when the changes change.
    Changed: Signal<()>,
}

--- A terminal session to start on the server.
export type TerminalSpec = {
    --- The workspace folder.
    directory: string,
    --- The shell's working folder. Defaults to `directory`.
    cwd: string?,
    --- Its initial title.
    title: string?,
    --- A program to run instead of the user's shell.
    command: string?,
    --- Its arguments.
    args: { string }?,
}

--- A terminal session for `ui.terminalSurface`.
export type PtySession = {
    --- The server's id for it.
    Id: string,
    --- Its title, as the shell sets it.
    Title: (self: PtySession) -> string,
    --- Its process has not exited.
    Running: (self: PtySession) -> boolean,
    --- Types text into it.
    Write: (self: PtySession, text: string) -> (),
    --- Ends it.
    Close: (self: PtySession) -> (),
    --- Fires with the exit code as its process exits.
    Exited: Signal<number?>,
    --- Fires with the new title.
    TitleChanged: Signal<string>,
}

--- One terminal of a session, as its tab lists it.
export type TerminalInfo = {
    --- The server's id for it.
    id: string,
    --- Its title, as the shell or a rename set it. Empty before the server names it.
    title: string,
    --- Its place among the session's terminals, from 1: what a default title says.
    number: number,
    --- Listed only to hold a restored tab until the server lists the session's terminals.
    hidden: boolean,
}

--- How `Workspace:OpenFile` opens a file.
export type OpenFileOptions = {
    --- Opens it in the preview tab, which the next preview replaces, instead of a kept one.
    preview: boolean?,
    --- The first line to select and scroll into view, from 1.
    startLine: number?,
    --- The last line to select. Defaults to `startLine`.
    endLine: number?,
}

--- The terminal tab `Terminal:OpenAsync` showed.
export type TerminalOpened = {
    --- The terminal's id; nil when the session has none and can't start one.
    id: string?,
    --- Where a session's first terminal opens (Settings > Terminal): `side` or `bottom`.
    placement: string,
}

--- A page to open.
export type BrowserOpenSpec = {
    --- The URL, or a local `:port`. Defaults to `about:blank`.
    url: string,
    --- The session whose side region gets the page's tab. Defaults to the routed session.
    sessionId: string?,
    --- Opens the tab without moving the keyboard: focus stays where it is.
    background: boolean?,
    --- The side-region pane (`side`, `split`, or `bottom`) to open the page's tab in.
    pane: string?,
}

--- Where to show a page's tab.
export type BrowserShowOptions = {
    --- The session whose side region shows it. Defaults to the routed session.
    sessionId: string?,
    --- Selects the tab without moving the keyboard.
    background: boolean?,
}

--- One page of a session, as its tab lists it.
export type BrowserTabInfo = {
    --- The host's id for it.
    id: string,
    --- Its address; empty for a blank page.
    url: string,
    --- Its title; empty before the page names itself.
    title: string,
    --- The page exists. A stored tab the engine never opened in this run is listed, not open.
    open: boolean,
}

--- Where the browser engine stands.
export type BrowserEngine = {
    --- Its state.
    state: BrowserEngineState,
    --- Why it failed, for `failed`.
    message: string?,
    --- The engine that runs (or last ran) keeps a page's downloads for `BrowserTab:Files`; false before one has run, and for an engine from before downloads.
    downloads: boolean,
    --- The engine that runs (or last ran) confines frame and sub-resource requests to the page's URL policy; false before one has run, and for an engine from before URL policy.
    policy: boolean,
}

--- An element the user picked in a page with the element picker.
export type BrowserPick = {
    --- The page.
    tabId: string,
    --- The element's `@eN` reference.
    ref: string,
    --- A CSS selector for it.
    selector: string,
}

--- A page in the app's browser, for `ui.browserSurface`.
export type BrowserTab = {
    --- The host's id for it.
    Id: string,
    --- The page exists: it hasn't closed.
    IsOpen: (self: BrowserTab) -> boolean,
    --- The current URL.
    Url: (self: BrowserTab) -> string,
    --- The page title.
    Title: (self: BrowserTab) -> string,
    --- A load is in flight.
    Loading: (self: BrowserTab) -> boolean,
    --- History has an earlier page.
    CanGoBack: (self: BrowserTab) -> boolean,
    --- History has a later page.
    CanGoForward: (self: BrowserTab) -> boolean,
    --- The page's zoom: 1 at 100%.
    Zoom: (self: BrowserTab) -> number,
    --- The element picker waits for a click in the page.
    Inspecting: (self: BrowserTab) -> boolean,
    --- What the page downloaded, oldest first: each file in the engine's private downloads folder, with no dialog, until its tab closes or the engine ends; one over 5 MiB is cancelled, and a tab keeps at most 64. Empty with an engine that doesn't keep downloads (`Browser:Engine().downloads`).
    Files: (self: BrowserTab) -> { BrowserFile },
    --- Why the page shows no document: the engine's error for a load that failed (`net::ERR_CONNECTION_REFUSED`), or `page_crashed` when its renderer went away; nil while it shows one. A reload or another address loads it again.
    LoadError: (self: BrowserTab) -> string?,
    --- Why the last navigation was refused while keeping the page (`browser.address.credentials`, `browser.address.workspace`, `browser.address.web`, or `ERR_BLOCKED_BY_CLIENT`); nil once an allowed navigation starts.
    Refusal: (self: BrowserTab) -> string?,
    --- Count of refused navigations on this page, so a repeated refusal still ends the address field's hold.
    Rejected: (self: BrowserTab) -> number,
    --- Loads a URL, or a local `:port`.
    Navigate: (self: BrowserTab, url: string) -> (),
    --- Goes back.
    Back: (self: BrowserTab) -> (),
    --- Goes forward.
    Forward: (self: BrowserTab) -> (),
    --- Reloads.
    Reload: (self: BrowserTab) -> (),
    --- Stops loading.
    Stop: (self: BrowserTab) -> (),
    --- Zooms the page in or out a step, or back to 100%. The site keeps its zoom.
    SetZoom: (self: BrowserTab, direction: BrowserZoom) -> (),
    --- Starts or stops the element picker; without an argument, toggles it. A pick fires `Browser.Picked`.
    Inspect: (self: BrowserTab, enabled: boolean?) -> (),
    --- The fixed CSS viewport, or nil when the page follows the pane.
    Viewport: (self: BrowserTab) -> BrowserViewport?,
    --- Apply a device viewport in a local or dev build, or nil to restore the page.
    SetViewport: (self: BrowserTab, viewport: BrowserViewport?) -> (),
    --- Outlines the DevTools backend node `nodeId` in the page, or clears the outline.
    Highlight: (self: BrowserTab, nodeId: number?) -> (),
    --- Selects its tab in a session's side region, opening the region and adding the tab when it isn't there.
    Show: (self: BrowserTab, options: BrowserShowOptions?) -> (),
    --- Closes the page and its tab.
    Close: (self: BrowserTab) -> (),
    --- Sends one allow-listed DevTools protocol command and returns its result. Raises when the method or parameters are not permitted, when the engine fails, or after 10 seconds. Yields: call it from a handler, a task or a signal callback.
    DevtoolsAsync: (self: BrowserTab, method: string, params: { [string]: any }?, targetSessionId: string?) -> any,
    --- Counts the cookies in use for the page's HTTP(S) address; 0 for blank or workspace file pages. Yields: call it from a handler, a task or a signal callback.
    SiteCookiesAsync: (self: BrowserTab) -> number,
    --- Deletes cookies and origin storage for the page's HTTP(S) address and reloads the page. Yields: call it from a handler, a task or a signal callback.
    ClearSiteAsync: (self: BrowserTab) -> (),
    --- Reads a completed download of the page: its text when it is UTF-8, else its bytes in base64. Raises when the file is unknown, still downloading, failed, or over 5 MiB. Yields: call it from a handler, a task or a signal callback.
    ReadFileAsync: (self: BrowserTab, fileId: string) -> BrowserFileContents,
}

--- A file a page downloaded (`BrowserTab:Files`).
export type BrowserFile = {
    --- `file_<n>`, for `BrowserTab:ReadFileAsync`.
    id: string,
    --- The name the page gave it.
    name: string,
    --- Its MIME type.
    mime: string,
    --- Bytes received so far.
    bytes: number,
    --- `pending`, `completed` or `failed`.
    state: string,
    --- Why it failed: `too_large`, `canceled`, or Chromium's interrupt reason (`network_failed`, `file_no_space`, ...).
    error: string?,
}

--- A downloaded file's contents (`BrowserTab:ReadFileAsync`).
export type BrowserFileContents = {
    --- The name the page gave it.
    name: string,
    --- Its MIME type.
    mime: string,
    --- Its size.
    bytes: number,
    --- Its text, when it is UTF-8.
    text: string?,
    --- Its bytes in base64, when it isn't.
    base64: string?,
}

--- An Office file the host opened, laid out on a thread of its own. `ui.officePages` and `ui.officeSheet` draw it; `Office:Close` releases it.
export type OfficeDocument = {
    --- The host's id for it.
    id: number,
    --- Its format.
    kind: OfficeKind,
    --- Pages or slides; 0 for a workbook.
    pages: number,
    --- A workbook's sheet names, in order.
    sheets: { string },
}

--- What `Office:OpenAsync` found: the file's size and first bytes, and the document when they are an Office Open XML package that opened.
export type OfficeFile = {
    --- The file's size in bytes.
    size: number,
    --- What its first bytes are.
    signature: OfficeSignature,
    --- The opened document; nil when the bytes are no package or the engine failed.
    document: OfficeDocument?,
    --- Why it did not open.
    error: string?,
}

--- The words `ui.officePages` shows, in the plugin's language.
export type OfficePagesLabels = {
    --- A page's label, `{{number}}` its number from 1: "Page {{number}}" or "Slide {{number}}".
    page: string,
    --- The badge under a hidden slide.
    hiddenSlide: string?,
    --- Over a page that could not be drawn.
    failed: string,
    --- The label of a slide's notes.
    speakerNotes: string?,
    --- Under the last page when the rest could not be laid out.
    layoutFailed: string?,
}

--- The words `ui.officeSheet` says: the grid's name for screen readers, and the notices a copy shows.
export type OfficeSheetLabels = {
    --- The grid's accessible name: the sheet's name.
    name: string?,
    --- The toast when a copy holds more than 200,000 cells.
    copyTooLarge: string?,
    --- The toast when the cells could not be read.
    copyFailed: string?,
}

--- Where the app updater stands.
export type UpdaterState = {
    --- The phase.
    phase: UpdaterPhase,
    --- The available version.
    version: string?,
    --- Download progress, 0 to 1.
    progress: number?,
    --- The release notes, as Markdown.
    notes: string?,
    --- Why it failed.
    error: string?,
}

--- Keeps the display awake while held.
export type PowerLock = {
    --- Lets the display sleep again.
    Release: (self: PowerLock) -> (),
}

--- A QR code's module matrix.
export type QrCode = {
    --- Modules per side.
    size: number,
    --- `size * size` modules, row by row: true is dark.
    modules: { boolean },
    --- Its dark modules inside a 4-module quiet zone, as an icon `ui.icon` draws crisp at any size in its color.
    icon: IconName,
}

--- Where an image stands, from `Images:State`.
export type ImageState = {
    --- Loading, loaded or failed.
    status: ImageStatus,
    --- Its width in pixels, once loaded.
    width: number?,
    --- Its height in pixels, once loaded.
    height: number?,
    --- Why it failed.
    error: string?,
}

--- A credential prompt from a child process.
export type AskpassPrompt = {
    --- Its id within the listener, as `Cleared` names it.
    Id: string,
    --- What the process asks.
    Prompt: string,
    --- The answer is a password. False for a yes/no question, such as trusting a host's key.
    Secret: boolean,
    --- Answers it. Nil cancels.
    Respond: (self: AskpassPrompt, answer: string?) -> (),
}

--- A loopback endpoint that turns `SSH_ASKPASS` prompts into signals.
export type AskpassListener = {
    --- Environment variables to give the child process.
    Env: { [string]: string },
    --- Fires with each prompt, one at a time.
    Prompted: Signal<AskpassPrompt>,
    --- Fires with a prompt's id once it ends: answered, cancelled, or its process stopped asking.
    Cleared: Signal<string>,
    --- Stops listening.
    Close: (self: AskpassListener) -> (),
}

--- One line of the Studio console.
export type ConsoleLine = {
    --- The plugin that wrote it.
    plugin: string,
    --- How serious it is.
    level: ConsoleLevel,
    --- What it says. An error's traceback follows its first line.
    message: string,
    --- The `.luau` file the message names, such as `init.luau`.
    file: string?,
    --- The line in `file`.
    line: number?,
    --- A `file://` link to that line, when the plugin runs from a folder.
    href: string?,
    --- How many times it was written in a row.
    count: number,
}

--- What `Console:Eval` gave back.
export type ConsoleEvalResult = {
    --- It ran without an error.
    ok: boolean,
    --- Its value as text.
    value: string?,
    --- The error it raised.
    error: string?,
}

--- A plugin's live costs.
export type PluginMetrics = {
    --- Luau heap in KB.
    memoryKb: number,
    --- The last render's cost in microseconds.
    renderUs: number,
    --- Share of memo lookups that reused their subtree, 0 to 1.
    memoHitRate: number,
}

--- A command's slash form in the composer.
export type CommandSlash = {
    --- The name after the slash, such as `open` for `/open`.
    name: string,
    --- The text after the name goes to `run`.
    arguments: boolean?,
    --- Lists the entry right after the slash entry with this name.
    after: string?,
}

--- A command in the palette, with an optional keybind and slash command. Published as `<plugin>.<id>`. Desktop's plugin API calls it `Command`.
export type CommandSpec = {
    --- Its id within the plugin.
    id: string,
    --- The palette label.
    title: string,
    --- The palette's second line.
    description: string?,
    --- More words the palette matches, separated by spaces.
    keywords: string?,
    --- The palette category.
    group: string?,
    --- Palette order, ascending. Commands without an order are listed after ordered commands.
    order: number?,
    --- The Settings › Shortcuts section. Defaults to `general`.
    section: CommandSection?,
    --- The default keybind, such as `"mod+shift+t"`.
    bind: string?,
    --- The id of the command whose keybind this one shares, such as `"tab.close"`: Settings > Shortcuts lists that one and leaves this one out, unless the user has bound this one itself (Desktop's `follows`).
    follows: string?,
    --- Also offers it as a slash command.
    slash: CommandSlash?,
    --- Keeps it out of the palette. The keybind still works. A function makes it reactive: the host calls it as it lists commands, and again only once something it read changes. Keep it cheap, and don't yield.
    hidden: (boolean | (() -> boolean))?,
    --- Also lists it under Suggested.
    suggested: boolean?,
    --- Lists it when the palette opens without a query.
    featured: boolean?,
    --- False takes it out of the palette and the slash list and turns its keybind off, and `Commands:Run` declines it. Defaults to true. A function makes it reactive, as Desktop's command accessor is: the host calls it as it lists commands or matches a key, and again only once something it read changes (`Layout:Narrow`, the routed session, a store value). Keep it cheap, and don't yield.
    enabled: (boolean | (() -> boolean))?,
    --- The keybind also fires while a text field has focus.
    editable: boolean?,
    --- Offered only while a session is open.
    requiresSession: boolean?,
    --- Runs it, as a coroutine. `input` is the slash command's arguments. Return `false` before the first yield to decline the key, so it reaches its target; any other return handles it.
    run: (input: string?) -> ...any,
}

--- What a panel's render receives.
export type PanelProps = {
    --- The routed session.
    sessionId: string,
    --- Its workspace folder.
    directory: string,
    --- An instance panel's tab.
    instance: string?,
    --- The tab is selected.
    active: boolean,
    --- Where it renders.
    placement: "side" | "mobile",
    --- In `onFocus`: the selection came back with a restored strip rather than from the user.
    restored: boolean?,
    --- The pane's width in points: lay out for it as a container query would.
    width: number,
    --- The pane's height in points.
    height: number,
    --- In a narrow window's More drawer, the nested view this panel opened there with `Layout:OpenDrawerView`: its `data`. Nil while the drawer shows the panel's own view.
    drawer: { [string]: any }?,
}

--- What a panel's `label` render receives.
export type PanelTabLabelProps = {
    --- The routed session.
    sessionId: string,
    --- An instance panel's tab.
    instance: string?,
    --- The tab is selected.
    active: boolean,
    --- The tab is the replaceable preview tab.
    preview: boolean,
}

--- One tab an instance panel lists.
export type PanelTab = {
    --- The instance id.
    instance: string,
    --- The accessible name, and the label without a `label` render.
    title: string,
    --- Listed before every other tab, never closed or dragged.
    pinned: boolean?,
    --- Struck through, such as a deleted file.
    missing: boolean?,
    --- The workspace file the tab shows.
    file: string?,
    --- Its icon, instead of the panel's. A tab showing `file` defaults to the file's type icon.
    icon: IconName?,
    --- A launcher tab: the next preview replaces it, and closing it keeps nothing in Recently closed.
    transient: boolean?,
    --- Drags between panes. Defaults to true.
    draggable: boolean?,
    --- Selected when the selected tab closes.
    fallback: boolean?,
    --- Listed but not drawn in the strip: it holds a restored selection until the panel can show it.
    hidden: boolean?,
}

--- What Recently closed lists for a panel's closed tab.
export type ClosedTab = {
    --- Its title, instead of the tab's.
    title: string?,
    --- Its icon.
    icon: IconName?,
    --- Muted text after its title, such as a page's host.
    detail: string?,
    --- What `restore` gets back, such as a page's address.
    data: string?,
}

--- A panel's view in a narrow window (below 768 points), where the side region folds into a Session / More switcher. Its render gets `placement` `"mobile"`. Desktop's plugin API calls it `MobileView`.
export type PanelMobileView = {
    --- Its label in the switcher or the More drawer.
    title: string,
    --- Views list in ascending order.
    order: number,
    --- Where it is offered: a switcher tab, a More row that replaces the conversation, or a More row that opens in the drawer.
    kind: PanelMobileKind,
    --- Its icon in the More drawer.
    icon: IconName?,
    --- Once shown, it stays mounted while another view shows. Defaults to false: it mounts each time it is shown.
    cached: boolean?,
}

--- A panel: tabs in a session's side region. Desktop's plugin API calls it `Panel`.
export type PanelSpec = {
    --- Its id within the plugin.
    id: string,
    --- Its tab and launcher title.
    title: string,
    --- The launcher card's second line.
    description: string?,
    --- Its icon.
    icon: IconName?,
    --- Launcher order, ascending.
    order: number?,
    --- Asks for the wider session minimum while open.
    wide: boolean?,
    --- A width of its own for the side region, in points.
    width: number?,
    --- Its tabs are not restored.
    transient: boolean?,
    --- Offered in the + menu and the New tab view. Defaults to true.
    launcher: boolean?,
    --- Its tab sits at the front of the strip.
    first: boolean?,
    --- Its tab is selected when the selected tab closes.
    fallback: boolean?,
    --- Its view in a narrow window: a switcher tab or a More drawer row. Without one, a narrow window doesn't offer it.
    mobile: PanelMobileView?,
    --- False withdraws the panel: no tab, launcher or narrow view. Defaults to true. A function offers it per session, as Desktop's panel accessor does (`session.project` for Session details): the host calls it with the routed session as it lists panels, and again only once something it read changes. Keep it cheap, and don't yield.
    enabled: (boolean | ((sessionId: string) -> boolean))?,
    --- The pane a new tab opens in beside none of the plugin's others. Defaults to `side`.
    pane: PanelPane?,
    --- What Recently closed lists for its closed tab.
    closed: ClosedTab?,
    --- Asks for the wider session minimum while it returns true. Read on every layout: keep it cheap, and don't yield.
    isWide: (() -> boolean)?,
    --- Its tab's title in a session, instead of `title`, such as a count that changes. Keep it cheap, and don't yield.
    tabTitle: ((sessionId: string) -> string)?,
    --- Makes it an instance panel: lists the session's tabs from the stored instance ids in `open`. A tab's key is `<plugin>:<instance>`; a panel without `list` has one tab, `<plugin>:<id>`.
    list: ((sessionId: string, open: { string }) -> { PanelTab })?,
    --- Draws a tab's content.
    render: (view: View, props: PanelProps) -> UiNode?,
    --- Draws the tab's label in the strip, instead of its icon and title.
    label: ((view: View, props: PanelTabLabelProps) -> UiNode?)?,
    --- Runs after the tab leaves the strip.
    onClose: ((props: PanelProps) -> ())?,
    --- Runs as the tab becomes selected.
    onFocus: ((props: PanelProps) -> ())?,
    --- What Recently closed keeps of a tab as it closes, instead of `closed`; nil keeps nothing. Keep it cheap, and don't yield.
    closedTab: ((props: PanelProps) -> ClosedTab?)?,
    --- Brings a tab back from Recently closed with its `ClosedTab.data`. Without it, the tab's key opens again.
    restore: ((props: PanelProps, data: string?) -> ())?,
}

--- A row in Settings › Plugins, under this plugin.
export type SettingSpec = {
    --- Its search id.
    id: string,
    --- The row's title.
    title: string,
    --- A line under the title.
    description: string?,
    --- More words settings search matches.
    keywords: string?,
    --- Draws the row's control.
    render: (view: View) -> UiNode?,
}

--- A settings search entry, indexed without drawing the page.
export type SettingEntry = {
    --- The `ui.settingsRow` id search reveals.
    id: string,
    --- The title in search results.
    title: string,
    --- Its second line.
    description: string?,
    --- More words search matches.
    keywords: string?,
}

--- What a settings page's render receives.
export type SettingsPageProps = {
    --- The entry id settings search is revealing.
    target: string?,
}

--- A settings page, or a section on a host page. Desktop's plugin API calls it `SettingsPage`.
export type SettingsPageSpec = {
    --- The page's id, also for `Layout:OpenSettings`.
    id: string,
    --- Its navigation label.
    title: string,
    --- The line under the page's title. A section on a host page has none.
    description: string?,
    --- Its navigation icon.
    icon: IconName?,
    --- Adds a section to this host page instead of a page of its own.
    page: HostSettingsPage?,
    --- Lists the page in the About group, such as release notes.
    about: boolean?,
    --- Search entries.
    entries: { SettingEntry }?,
    --- Draws the page or section.
    render: (view: View, props: SettingsPageProps) -> UiNode?,
    --- Draws controls at the end of the page's header, such as a link out.
    renderActions: ((view: View) -> UiNode?)?,
}

--- An item of a host menu. Desktop's plugin API calls it `MenuItem`.
export type MenuItemSpec = {
    --- The menu.
    menu: MenuName,
    --- Its id within the plugin.
    id: string,
    --- Its label.
    title: string,
    --- Ascending order.
    order: number?,
    --- `session.panel`: its icon.
    icon: IconName?,
    --- `session.panel`: a published command id whose shortcut it shows.
    keybind: string?,
    --- `session.panel`: muted detail in launcher lists.
    description: string?,
    --- `session.panel`: + runs this item directly.
    primary: boolean?,
    --- `server.row`: lists it only for rows where it returns true. `session.panel`: lists it only where it returns true for the session id.
    when: ((target: string) -> boolean)?,
    --- `session.panel`: what it opens is already in the session's strip, so the New tab view leaves it out.
    opened: ((sessionId: string) -> boolean)?,
    --- `session.panel`: muted detail for the session, instead of `description`.
    detail: ((sessionId: string) -> string?)?,
    --- `server.row`: shows it disabled while it returns false.
    enabled: ((target: string) -> boolean)?,
    --- `session.panel`: runs once when the pointer comes onto its row in the + menu or the New tab view, before any press: the moment to get ready for it, such as `Browser:Prepare`.
    hover: ((sessionId: string) -> ())?,
    --- Runs it. `server.row` passes the row's server key.
    run: (target: string?) -> (),
}

--- An item of the app menu bar. Desktop's plugin API calls it `MenubarItem`.
export type MenubarItemSpec = {
    --- The submenu.
    menu: MenubarMenu,
    --- Its id within the plugin.
    id: string,
    --- Its label.
    label: string,
    --- Ascending order.
    order: number?,
    --- Its accelerator, such as `"mod+,"`.
    bind: string?,
    --- False shows it disabled.
    enabled: boolean?,
    --- Runs it.
    run: () -> (),
}

--- Opens local links it matches. The matching handler with the highest priority wins. Desktop's plugin API calls it `LinkHandler`.
export type LinkHandlerSpec = {
    --- Its id within the plugin.
    id: string,
    --- Higher wins. Defaults to 0.
    priority: number?,
    --- Whether it opens the link. It must not yield.
    match: (link: Link) -> boolean,
    --- Opens a link `match` accepted.
    open: (link: Link) -> (),
    --- Whether the target exists, so text renders as a link. It must not read file contents.
    exists: ((link: Link) -> boolean)?,
}

--- What a titlebar item shows.
export type TitlebarItemView = {
    --- The visible label.
    label: string,
    --- The accessible name, when it differs from the label.
    title: string?,
    --- Its icon. Defaults to a download arrow.
    icon: IconName?,
    --- Shows a spinner and disables it.
    busy: boolean?,
    --- Shows it pressed.
    pressed: boolean?,
}

--- A pill at the titlebar's end. Desktop's plugin API calls it `TitlebarItem`.
export type TitlebarItemSpec = {
    --- Its id within the plugin.
    id: string,
    --- What it shows, or nil to hide it. Called again after `Contribution:Refresh` and after any signal the plugin fires.
    show: () -> TitlebarItemView?,
    --- Runs when the user clicks it.
    run: (() -> ())?,
}

--- What a composer pill receives.
export type ComposerPillProps = {
    --- The session whose composer it sits above.
    sessionId: string,
    --- Its workspace folder.
    directory: string,
}

--- One row of the menu a composer pill opens.
export type ComposerPillRow = {
    --- Its id within the pill.
    id: string,
    --- Its text.
    label: string,
    --- Its icon.
    icon: IconName?,
    --- Colors its icon.
    tone: Tone?,
    --- Muted text at its end.
    detail: string?,
    --- Runs when the user picks it.
    run: (() -> ())?,
}

--- What a composer pill shows above one session's composer.
export type ComposerPillView = {
    --- The capsule's text.
    label: string,
    --- The capsule's icon.
    icon: IconName?,
    --- Colors the icon.
    tone: Tone?,
    --- The accessible name, when it differs from `label`. It names the popover `renderMenu` fills, which has no heading of the host's.
    title: string?,
    --- The menu it opens, when the pill has no `renderMenu`.
    rows: { ComposerPillRow }?,
}

--- A pill in the row above a session's composer. Desktop's plugin API calls it `ComposerPill`.
export type ComposerPillSpec = {
    --- Its id within the plugin.
    id: string,
    --- Ascending order among plugin pills.
    order: number?,
    --- What it shows above this session's composer, or nil for nothing. It must not yield.
    show: (props: ComposerPillProps) -> ComposerPillView?,
    --- Content for the popover the pill opens. The host owns the capsule and the popover's frame (its width, a 4 pt inset, placement, focus and dismissal); everything inside, any heading included, is yours.
    renderMenu: ((view: View, props: ComposerPillProps) -> UiNode?)?,
    --- Runs on a click when the pill has no menu.
    run: ((props: ComposerPillProps) -> ())?,
}

--- One complete fenced code block.
export type CodeBlockProps = {
    --- The source between the fences.
    code: string,
    --- The fence's language, lowercased. Empty when it has none.
    language: string,
    --- The session that shows it.
    sessionId: string?,
    --- The reply it is in still streams: the block may grow, and may not be closed yet.
    streaming: boolean?,
}

--- Renders the fenced code blocks of some languages as your own UI. Desktop's plugin API calls it `CodeBlock`.
export type CodeBlockSpec = {
    --- Its id within the plugin.
    id: string,
    --- The languages it renders. The host resolves no aliases.
    languages: { string },
    --- Draws a block. Return nil to keep the highlighted code.
    render: (view: View, props: CodeBlockProps) -> UiNode?,
}

--- A button beside a code block's Copy button. Desktop's plugin API calls it `CodeBlockAction`.
export type CodeBlockActionSpec = {
    --- Its id within the plugin.
    id: string,
    --- The languages whose blocks show it.
    languages: { string },
    --- Its icon.
    icon: IconName,
    --- Its accessible name and tooltip.
    title: string,
    --- Ascending order, before Copy.
    order: number?,
    --- Runs on a click.
    run: (props: CodeBlockProps) -> (),
}

--- What a slot's render receives.
export type SlotProps = {
    --- Session slots: the session.
    sessionId: string?,
    --- Session slots: its workspace folder.
    directory: string?,
    --- `session.header`: the timeline is on screen.
    active: boolean?,
    --- `session.header`: the title needs the room, so draw the compact form.
    compact: boolean?,
    --- `session.location.menu`: the menu is open.
    shown: boolean?,
    --- New tab slots: the pane (`side`, `split`, or `bottom`).
    pane: string?,
    --- New tab slots: the pane's available width in points.
    width: number?,
    --- New tab slots: incremented when the user explicitly asked for this New tab, so its first field takes focus once.
    focusToken: number?,
}

--- Content for one of the shell's slots. Desktop's plugin API calls it `Slot`.
export type SlotSpec = {
    --- Its id within the plugin.
    id: string,
    --- The slot.
    at: SlotName,
    --- Ascending order.
    order: number?,
    --- Draws its content, or nil for nothing.
    render: (view: View, props: SlotProps) -> UiNode?,
}

--- The seed colors a theme variant generates its scales from, as `#RRGGBB` strings.
export type ThemeSeeds = {
    --- The gray scale's base.
    neutral: string,
    --- The brand and accent color.
    primary: string,
    --- Success states.
    success: string,
    --- Warning states.
    warning: string,
    --- Error states.
    error: string,
    --- Informational states.
    info: string,
    --- Links and interactive elements.
    interactive: string,
    --- Added lines.
    diffAdd: string,
    --- Deleted lines.
    diffDelete: string,
}

--- One color scheme of a theme.
export type ThemeVariant = {
    --- The seed colors.
    seeds: ThemeSeeds,
    --- Exact values for single tokens of the app's theme file format.
    overrides: { [string]: string }?,
}

--- A color theme, listed in Settings › Appearance as `<plugin>:<id>`. Desktop's plugin API calls it `Theme`.
export type ThemeSpec = {
    --- Its id within the plugin.
    id: string,
    --- The name the theme list shows.
    name: string,
    --- The light scheme.
    light: ThemeVariant,
    --- The dark scheme.
    dark: ThemeVariant,
}

--- What a file viewer's render receives.
export type FileViewerProps = {
    --- The file.
    path: string,
    --- The session that opened it.
    sessionId: string?,
    --- Its workspace folder.
    directory: string,
}

--- Shows files with some extensions in the file panel. Desktop's plugin API calls it `FileViewer`.
export type FileViewerSpec = {
    --- Its id within the plugin.
    id: string,
    --- Its name, such as `"Word document"`.
    title: string,
    --- The extensions it shows, without the dot.
    extensions: { string },
    --- Draws the file.
    render: (view: View, props: FileViewerProps) -> UiNode?,
}

--- What a server source's row render receives.
export type ServerRowProps = {
    --- The entry's id.
    id: string,
    --- Its key: `<plugin>:<id>`.
    key: string,
    --- Its latest health check.
    health: ServerHealth?,
}

--- What a server source's cover render receives.
export type ServerCoverProps = {
    --- The entry's id.
    id: string,
    --- Its key: `<plugin>:<id>`.
    key: string,
    --- The tab it covers: `<server>:<sessionId>` for a session, the draft's id for a draft.
    tab: string,
    --- The window's page visit: it changes whenever the window shows another page, so a cover can offer something once per visit, as Desktop's SSH cover offers its sign-in.
    visit: number,
    --- A dialog shows over the window. A cover offers nothing that opens a dialog while one shows.
    dialog: boolean,
}

--- A source of servers, such as SSH hosts or WSL distributions. Add it in the app half to list entries, and in each window half to draw them: the host runs a window half's `row`, `cover`, `connect`, `reconnect` and `remove`, else the app half's. Desktop's plugin API calls it `Server`.
export type ServerSourceSpec = {
    --- Its id within the plugin.
    id: string,
    --- Its name in Settings › Servers.
    title: string,
    --- Ascending order among sources.
    order: number?,
    --- Draws an entry's connection row in Settings › Servers. Window halves only.
    row: ((view: View, props: ServerRowProps) -> UiNode?)?,
    --- Draws over a session or draft tab of an entry that is not `ready`, such as a disconnected SSH host's Reconnect panel. The tab stays mounted under it. Window halves only.
    cover: ((view: View, props: ServerCoverProps) -> UiNode?)?,
    --- Starts or signs in to an entry that is not ready, as a coroutine. Home's Authenticate and a signed-out tab's Sign in run it. Return true once it is ready. The host gives up after 120 seconds.
    connect: ((id: string) -> boolean)?,
    --- Runs when the event stream of a `managed` entry drops, so the plugin can reach it again, for example through a new tunnel.
    reconnect: ((id: string) -> ())?,
    --- Runs before the host forgets an entry that Settings › Servers removes. Raise an error to keep the entry.
    remove: ((id: string) -> ())?,
}

--- A server source a plugin added.
export type ServerSource = {
    --- Replaces its entries. Once `ready`, the list is complete and the host forgets servers it no longer lists.
    SetEntries: (self: ServerSource, entries: { ServerEntry }, ready: boolean) -> (),
    --- Removes the source.
    Withdraw: (self: ServerSource) -> (),
}

--- What a dialog's render receives.
export type DialogProps = {
    --- What `Dialog:Open` passed.
    data: { [string]: any }?,
}

--- A modal dialog, opened with `Dialog:Open`.
export type DialogSpec = {
    --- Its id within the plugin.
    id: string,
    --- Its heading and accessible name.
    title: string?,
    --- Its size preset.
    size: DialogSize?,
    --- Which modal overlay layer the dialog draws in (`"standard"` beneath elevated recovery/server dialogs, or `"elevated"` above them). Defaults to `"elevated"`.
    layer: DialogLayer?,
    --- Corner radius in points. Defaults to 6 for `normal` dialogs and 12 for `tall`, `large` and `x-large`.
    radius: number?,
    --- Hides the scroll bar when a `normal` dialog's content overflows, as Desktop's `dialog-content` does. Defaults to false.
    hideScrollbar: boolean?,
    --- Draws its body inside the host's dialog frame: Desktop's dialog with the overlay elevation. A normal one is as tall as its content on layer-01 with 6-point corners; a tall one is as tall as its content up to its size's height on bg-base with 12-point corners, and what in it may shrink (`shrink = true`, `minH = 0`) gives way once it is that tall; a large or x-large one is its size's height on bg-base with 12-point corners, so its body can fill it (`h = "full"`).
    render: (view: View, props: DialogProps) -> UiNode?,
    --- Runs when Esc or a click outside closes it.
    onDismiss: (() -> ())?,
}

--- What a popover's render receives.
export type PopoverProps = {
    --- What `Popover:Open` or `ui.flyoutRow` passed.
    data: { [string]: any }?,
}

--- Popover content, opened with `Popover:Open` or `ui.flyoutRow`. The host owns placement, focus and dismissal.
export type PopoverSpec = {
    --- Its id within the plugin.
    id: string,
    --- Its heading and accessible name.
    title: string?,
    --- Its width in points.
    width: number?,
    --- Draws its content.
    render: (view: View, props: PopoverProps) -> UiNode?,
}

--- A tool on the app's client MCP server, named `<plugin>_<name>`. Desktop's plugin API calls it `McpTool`.
export type McpToolSpec = {
    --- Its name within the plugin.
    name: string,
    --- What it does, for the model.
    description: string,
    --- Its arguments' JSON Schema.
    inputSchema: { [string]: any },
    --- Runs it, as a task, and returns its result: a string, a table sent as JSON, or an `McpImage`. Raise an error to fail. It may yield (call `...Async` methods): the call answers when it returns, or fails after 30 seconds.
    call: (args: { [string]: any }, context: McpCallContext) -> any,
}

--- An MCP tool result with an image: a text part and the image.
export type McpImage = {
    --- The text part, such as JSON metadata.
    text: string,
    --- The image, base64-encoded.
    image: string,
    --- Its type, such as `image/png`.
    mimeType: string,
}

--- Where an MCP tool call comes from.
export type McpCallContext = {
    --- The session whose agent called it.
    sessionId: string,
    --- That session's workspace folder.
    directory: string,
}

-- Services ----------------------------------------------------------------------------------------

--- The sessions the app holds. Both halves: `ctx:GetService("Sessions")`.
export type SessionsService = {
    --- A session by id.
    Get: (self: SessionsService, sessionId: string) -> Session?,
    --- The session this window shows. Always nil in the app half.
    Routed: (self: SessionsService) -> Session?,
    --- The sessions of open tabs.
    Open: (self: SessionsService) -> { Session },
    --- Fires with a session after it changes.
    Changed: Signal<Session>,
    --- Fires as this window routes another session, or none.
    RoutedChanged: Signal<Session?>,
}

--- The server's model catalog. Both halves: `ctx:GetService("Models")`.
export type ModelsService = {
    --- How the catalog names a model.
    Get: (self: ModelsService, providerId: string, modelId: string) -> ModelInfo,
    --- Every model currently in the host's catalog.
    List: (self: ModelsService) -> { ModelEntry },
    --- Replaces this plugin's inline model-picker badges. Pass an empty list to clear them. Window halves only.
    SetBadges: (self: ModelsService, badges: { ModelBadge }) -> (),
    --- Fires when the catalog changes.
    Changed: Signal<()>,
}

--- The OpenCode server this window works with: the server of the session or draft it shows, so a panel in a tab from another server talks to that server. A path that starts with `local:` goes to the app's own server. In the app half, the app's own server. Both halves: `ctx:GetService("Server")`.
export type ServerService = {
    --- Sends a request with a JSON body, off the UI thread. Yields: call it from a handler, a task or a signal callback.
    RequestAsync: (self: ServerService, method: HttpMethod, path: string, body: any?) -> ServerResponse,
    --- A GET of `path`, cached until `version` changes. Safe in a render: it never yields, and the window redraws when the answer lands. With `items`, a list's answer comes split into `items` off the UI thread instead of decoded into `json`.
    Resource: (self: ServerService, path: string, version: MemoVersion, items: boolean?) -> Resource,
    --- Drops this plugin's cached reads whose path starts with `prefix`.
    Invalidate: (self: ServerService, prefix: string) -> (),
    --- A signal for the server's events whose type starts with `prefix`.
    OnEvent: (self: ServerService, prefix: string) -> Signal<ServerEvent>,
    --- Percent-encodes text for a path or query.
    EncodeUri: (self: ServerService, text: string) -> string,
    --- The version it reported.
    Version: (self: ServerService) -> string?,
    --- It runs on this machine.
    IsLocal: (self: ServerService) -> boolean,
    --- The window works with a live server rather than an offline fixture.
    IsLive: (self: ServerService) -> boolean,
    --- The app may change things on it.
    CanWrite: (self: ServerService) -> boolean,
    --- Fires as its event connection goes up or down.
    Connected: Signal<boolean>,
}

--- The servers the app lists. Both halves: `ctx:GetService("Servers")`.
export type ServersService = {
    --- Every listed server.
    List: (self: ServersService) -> { ServerInfo },
    --- The key of the server this window works with now.
    Current: (self: ServersService) -> string?,
    --- A server's latest health check.
    Health: (self: ServersService, key: string) -> ServerHealth?,
    --- Shows a listed server on Home. Every listed server stays connected, so nothing switches and no tab closes.
    Connect: (self: ServersService, key: string) -> (),
    --- Fires when the list or a server's state changes.
    Changed: Signal<()>,
}

--- HTTP requests off the UI thread. Both halves: `ctx:GetService("Http")`.
export type HttpService = {
    --- Sends a request. Yields: call it from a handler, a task or a signal callback.
    FetchAsync: (self: HttpService, request: HttpRequest) -> HttpResponse,
}

--- Child processes the host supervises. Both halves: `ctx:GetService("Process")`.
export type ProcessService = {
    --- Starts a process. A program that cannot start still returns a handle, whose `Exited` fires with the error.
    Spawn: (self: ProcessService, spec: ProcessSpec) -> ProcessHandle,
    --- Starts a process and yields until it exits, with all its output. Yields: call it from a handler, a task or a signal callback.
    RunAsync: (self: ProcessService, spec: ProcessSpec) -> ProcessOutput,
}

--- Values the host saves for this plugin in the app's database. Both halves and every window share one copy. Every value is in memory before the plugin's setup runs, so a read never waits and has the saved value. A `Set` changes the value at once; it reaches the disk within 250 ms, and always when a window hides or closes and when the app quits. Names starting `rune.` are the host's own. Both halves: `ctx:GetService("Store")`.
export type StoreService = {
    --- A global value.
    Value: <T>(self: StoreService, name: string, default: T) -> StoreValue<T>,
    --- A value per session, kept apart for each server.
    SessionValue: <T>(self: StoreService, name: string, default: T) -> SessionStoreValue<T>,
}

--- Typed values plugins share. A plugin may provide only ids under its own id. Both halves: `ctx:GetService("Contracts")`.
export type ContractsService = {
    --- Provides `value` under `id` (`<plugin>` or `<plugin>.<name>`). From the app half, for the whole process; from a window half, for that window.
    Provide: (self: ContractsService, id: string, value: any) -> Contribution,
    --- Follows a contract another plugin provides. Keep working while it is not active.
    Use: (self: ContractsService, id: string) -> Live<any>,
    --- Every contract id this half sees, with its state.
    Summary: (self: ContractsService) -> { [string]: LiveState },
}

--- The Studio console. `print` and `warn` write here too. Both halves: `ctx:GetService("Console")`.
export type ConsoleService = {
    --- Writes an information line.
    Log: (self: ConsoleService, ...any) -> (),
    --- Writes a warning.
    Warn: (self: ConsoleService, ...any) -> (),
    --- Writes an error, without raising one.
    Error: (self: ConsoleService, ...any) -> (),
    --- Every plugin's lines, oldest first; a repeat of the last line counts up. Read in a render, it redraws as lines come.
    Lines: (self: ConsoleService) -> { ConsoleLine },
    --- Drops every line.
    Clear: (self: ConsoleService) -> (),
    --- Runs Luau `source` in the window half of plugin `pluginId`, as Studio's command bar does: an expression's value, or a chunk's return value, as text. Local and beta builds only; elsewhere it fails.
    Eval: (self: ConsoleService, pluginId: string, source: string) -> ConsoleEvalResult,
}

--- Short messages in a corner of the window. From the app half, in the focused window. Both halves: `ctx:GetService("Toast")`.
export type ToastService = {
    --- Shows a toast.
    Show: (self: ToastService, spec: ToastSpec) -> ToastHandle,
    --- Shows a `default` toast.
    Info: (self: ToastService, title: string, description: string?) -> ToastHandle,
    --- Shows a `success` toast.
    Success: (self: ToastService, title: string, description: string?) -> ToastHandle,
    --- Shows an `error` toast that stays until dismissed.
    Error: (self: ToastService, title: string, description: string?) -> ToastHandle,
}

--- This plugin's popovers. The host places them, moves focus and closes them. The window half only: `ctx:GetService("Popover")`.
export type PopoverService = {
    --- Opens a popover beside an element, closing any other of this plugin's.
    Open: (self: PopoverService, spec: PopoverOpenSpec) -> (),
    --- Opens it, or closes it when it is open.
    Toggle: (self: PopoverService, spec: PopoverOpenSpec) -> (),
    --- Closes this plugin's open popover.
    Close: (self: PopoverService) -> (),
    --- Whether a popover is open.
    IsOpen: (self: PopoverService, id: string) -> boolean,
}

--- This plugin's modal dialogs. The window half only: `ctx:GetService("Dialog")`.
export type DialogService = {
    --- Opens a dialog, closing any other of this plugin's.
    Open: (self: DialogService, id: string, data: { [string]: any }?) -> (),
    --- Closes this plugin's open dialog.
    Close: (self: DialogService) -> (),
    --- Whether a dialog is open.
    IsOpen: (self: DialogService, id: string) -> boolean,
}

--- The app's own menus. The window half only: `ctx:GetService("Menu")`.
export type MenuService = {
    --- Opens a menu beside an element and yields until it closes. Returns the picked item's id, or nil. Yields: call it from a handler, a task or a signal callback.
    OpenAsync: (self: MenuService, spec: MenuOpenSpec) -> string?,
}

--- Panels, settings and drafts in this window. The window half only: `ctx:GetService("Layout")`.
export type LayoutService = {
    --- Opens or selects one of this plugin's panels.
    OpenPanel: (self: LayoutService, panelId: string, options: OpenPanelOptions?) -> (),
    --- Opens a panel, or closes it when it shows.
    TogglePanel: (self: LayoutService, panelId: string, options: OpenPanelOptions?) -> (),
    --- Closes a panel's tab.
    ClosePanel: (self: LayoutService, panelId: string, options: OpenPanelOptions?) -> (),
    --- Where a panel is in a session. Defaults to the routed session.
    PanelState: (self: LayoutService, panelId: string, sessionId: string?) -> PanelState,
    --- Opens Settings at one of this plugin's pages, or at the top.
    OpenSettings: (self: LayoutService, pageId: string?) -> (),
    --- Opens a new session's draft in a folder, with a prompt typed in.
    NewDraft: (self: LayoutService, directory: string, prompt: string?) -> (),
    --- Opens a new session's draft in a folder on a listed server, with a prompt typed in. Does nothing while the server is not listed.
    OpenDraft: (self: LayoutService, spec: DraftSpec) -> (),
    --- Asks for a folder on a server and opens a draft there, as Add project does: the system's folder picker for this machine's server, the server's own folder browser for another. A server that is not listed yet (one the plugin just started) gets 30 seconds to be listed; a newer request replaces a waiting one.
    OpenProject: (self: LayoutService, serverKey: string, title: string?) -> (),
    --- The window is narrow-screen sized.
    Narrow: (self: LayoutService) -> boolean,
    --- The file sidebar `ui.panelSidebar` draws.
    PanelSidebar: (self: LayoutService) -> PanelSidebarInfo,
    --- Shows or hides it.
    TogglePanelSidebar: (self: LayoutService) -> (),
    --- Puts it on an edge of the panel, for every side panel.
    MovePanelSidebar: (self: LayoutService, side: PanelSidebarSide) -> (),
    --- From a `drawer` kind panel's view in a narrow window's More drawer: shows a nested view in the drawer in its place (Desktop's `useDrawer().open`), with a back button named after the panel's view. The panel's render draws it, with `props.drawer` set to `data`. Does nothing elsewhere.
    OpenDrawerView: (self: LayoutService, title: string, data: { [string]: any }?) -> (),
    --- A session's side region is open. Defaults to the routed session.
    RegionOpen: (self: LayoutService, sessionId: string?) -> boolean,
    --- The side region has the keyboard: the last focus, click or key was in it, as Desktop's side focus reads (`#review-panel[data-focused]`). Read it when a command runs: Desktop's Open browser shortcut focuses the new tab's address only from inside the region.
    RegionFocused: (self: LayoutService) -> boolean,
    --- Moves the keyboard to the element with `key`, or to its first focusable control, and draws its focus ring: where a shortcut leaves the keyboard. Official's Toggle browser toolbar shortcut leaves it on the Options button.
    Focus: (self: LayoutService, key: string) -> (),
}

--- Routes local links to the plugins that handle them. The window half only: `ctx:GetService("Links")`.
export type LinksService = {
    --- Opens a link with the best handler. False when no handler matches.
    Open: (self: LinksService, link: Link) -> boolean,
    --- Whether a handler matches and says the target exists.
    Exists: (self: LinksService, link: Link) -> boolean,
}

--- Published commands, as the palette runs them. The window half only: `ctx:GetService("Commands")`.
export type CommandsService = {
    --- Runs a command by id: `<plugin>.<id>` or one of the app's. False when it is unknown or disabled.
    Run: (self: CommandsService, commandId: string, input: string?) -> boolean,
    --- A command's effective keybind.
    Keybind: (self: CommandsService, commandId: string) -> string?,
}

--- The theme and the user's appearance settings. Both halves: `ctx:GetService("Theme")`.
export type ThemeService = {
    --- The theme in use.
    Current: (self: ThemeService) -> ThemeInfo,
    --- The dark scheme is in use.
    IsDark: (self: ThemeService) -> boolean,
    --- A color as `#RRGGBBAA` for the current theme.
    Resolve: (self: ThemeService, color: Color) -> string,
    --- The user's font family name.
    Font: (self: ThemeService, family: FontFamily) -> string,
    --- Fires when the theme or scheme changes.
    Changed: Signal<ThemeInfo>,
}

--- The running app. Both halves: `ctx:GetService("App")`.
export type AppService = {
    --- The app's version.
    Version: string,
    --- Its release channel.
    Channel: Channel,
    --- The operating system.
    Platform: Platform,
    --- The commit this build was made from, when the build knows it.
    Commit: string?,
    --- The changes a local or beta build carries (Desktop's `OPENCODE_BETA_NOTES`): `{ sha, commits, notes }`, each note `{ type, summary, sha, pr?, author? }`, newest first; nil in other builds.
    BetaNotes: any?,
    --- The interface language, such as `"en"`.
    Locale: (self: AppService) -> string,
    --- The value of command-line option `--name`.
    Arg: (self: AppService, name: string) -> string?,
    --- The folder the app writes its logs to.
    LogsDir: (self: AppService) -> string?,
    --- The folder the app saves downloads to.
    DownloadsDir: (self: AppService) -> string?,
    --- Fires with the new interface language.
    LocaleChanged: Signal<string>,
}

--- The clipboard, the system browser and file manager, and the clock. Both halves: `ctx:GetService("System")`.
export type SystemService = {
    --- Puts text on the clipboard.
    CopyText: (self: SystemService, text: string) -> (),
    --- Opens a URL in the system's browser.
    OpenExternal: (self: SystemService, url: string) -> (),
    --- Shows a path in the system's file manager.
    Reveal: (self: SystemService, path: string) -> (),
    --- The wall clock in milliseconds since the Unix epoch.
    Now: (self: SystemService) -> number,
    --- The local time zone's offset from UTC in seconds, at that time.
    UtcOffset: (self: SystemService, ms: number) -> number,
    --- A TCP port nothing listens on now on the loopback address, for a server or tunnel a plugin starts. Nil when the system has none.
    FreePort: (self: SystemService) -> number?,
    --- A random version 4 UUID from the system's secure source, for passwords and tokens.
    RandomId: (self: SystemService) -> string,
}

--- System notifications that follow the user's notification settings. Both halves: `ctx:GetService("Notifications")`.
export type NotificationsService = {
    --- Shows a notification when the window is not in front.
    Notify: (self: NotificationsService, spec: NotificationSpec) -> (),
}

--- Files in the workspace and the apps that open them. Both halves: `ctx:GetService("Workspace")`.
export type WorkspaceService = {
    --- Reads a text file through the server. Yields: call it from a handler, a task or a signal callback.
    ReadFileAsync: (self: WorkspaceService, path: string) -> ReadResult,
    --- Saves content as a file in the downloads folder. Yields: call it from a handler, a task or a signal callback.
    SaveDownloadAsync: (self: WorkspaceService, name: string, content: string) -> SaveResult,
    --- Opens a file for `ui.codeView`.
    OpenDocument: (self: WorkspaceService, path: string) -> CodeDocument,
    --- Opens a folder for `ui.fileTree`.
    OpenTree: (self: WorkspaceService, directory: string) -> FileTreeModel,
    --- The installed apps that can open a path.
    OpenInApps: (self: WorkspaceService) -> { OpenInApp },
    --- Opens a path in one of those apps.
    OpenIn: (self: WorkspaceService, appId: string, path: string) -> (),
    --- A path inside a workspace folder, relative to it with `/` between parts; nil for a path outside it. A relative path stays as it is.
    Relative: (self: WorkspaceService, path: string, directory: string) -> string?,
    --- Opens workspace file `path` in a tab of the session's side region, leaving the keyboard where it was.
    OpenFile: (self: WorkspaceService, sessionId: string, path: string, options: OpenFileOptions?) -> (),
    --- Fires with a workspace path after it changes on disk.
    FileChanged: Signal<string>,
}

--- Session changes for `ui.diffView`. The window half only: `ctx:GetService("Diff")`.
export type DiffService = {
    --- Opens a set of changes.
    Open: (self: DiffService, spec: DiffOpenSpec) -> DiffDocument,
    --- The mode diffs draw in now. A `SetMode` or `ui.diffView` `mode` change lands with a later frame, so what sizes around the diff reads this to change with it.
    Mode: (self: DiffService) -> DiffMode,
    --- Draws every diff in `mode`.
    SetMode: (self: DiffService, mode: DiffMode) -> (),
}

--- Terminal sessions on the server, for `ui.terminalSurface`. Both halves: `ctx:GetService("Terminal")`.
export type TerminalService = {
    --- Starts a terminal session.
    Create: (self: TerminalService, spec: TerminalSpec) -> PtySession,
    --- Reattaches to a running session by id.
    Attach: (self: TerminalService, id: string) -> PtySession?,
    --- Session `sessionId`'s terminals in strip order. Until the server lists them, the stored tabs `open` names, hidden; after a list that failed, those tabs shown.
    List: (self: TerminalService, sessionId: string, open: { string }) -> { TerminalInfo },
    --- This window can start terminals: its server has them and the window isn't read-only.
    Available: (self: TerminalService) -> boolean,
    --- Starts a terminal in the session (the routed one by default) and opens its tab beside its others, or where Settings place a first one; it takes the keyboard.
    New: (self: TerminalService, sessionId: string?) -> (),
    --- `terminal.toggle`: a new terminal when the routed session has none; the active terminal's pane hides while its tab shows; otherwise its tab opens and takes the keyboard.
    Toggle: (self: TerminalService) -> (),
    --- Makes terminal `id` the session's active one, and with `focus` gives it the keyboard.
    Select: (self: TerminalService, sessionId: string, id: string, focus: boolean) -> (),
    --- Shows session `sessionId`'s terminal tab, starting a terminal when `newTerminal` or it has none, and leaves the keyboard where it was. Yields: call it from a handler, a task or a signal callback.
    OpenAsync: (self: TerminalService, sessionId: string, newTerminal: boolean) -> TerminalOpened,
}

--- Pages in this window's embedded browser, each a tab of a session's side region. The window half only: `ctx:GetService("Browser")`.
export type BrowserService = {
    --- Opens a page in a new tab of a session's side region, and returns it once its tab is there. Yields: call it from a handler, a task or a signal callback.
    OpenAsync: (self: BrowserService, spec: BrowserOpenSpec) -> BrowserTab,
    --- Session `sessionId`'s open pages, oldest first; without a session, every open page.
    Tabs: (self: BrowserService, sessionId: string?) -> { BrowserTab },
    --- An open page by id.
    Get: (self: BrowserService, id: string) -> BrowserTab?,
    --- The pages of the stored tabs `open` names, in that order, for a panel's `list`.
    List: (self: BrowserService, sessionId: string, open: { string }) -> { BrowserTabInfo },
    --- The page a session shows: its selected browser tab, else its newest open page, else the page the engine shows.
    Active: (self: BrowserService, sessionId: string) -> BrowserTab?,
    --- The page whose tab is selected in a session's open side region (the routed session by default).
    Selected: (self: BrowserService, sessionId: string?) -> BrowserTab?,
    --- The routed session's selected page while the keyboard is in its pane: its page, or a field of this plugin's.
    Focused: (self: BrowserService) -> BrowserTab?,
    --- Pages can open here: this build has a browser engine for its platform.
    Available: (self: BrowserService) -> boolean,
    --- Where the browser engine stands.
    Engine: (self: BrowserService) -> BrowserEngine,
    --- Normalizes a URL for `sessionId`'s URL policy (adding `http://` or `https://` when omitted), or raises the agent refusal message when the URL is not supported.
    NormalizeUrl: (self: BrowserService, url: string, sessionId: string?) -> string,
    --- A page is likely to open within `seconds` (5 by default, at most 60): the pointer is on a launcher for one, or an agent that uses the browser starts a turn. The engine's process starts now, so the page doesn't wait for it, and stops again once `seconds` pass with no page opened; a later call's `seconds` replace the earlier's. Does nothing while a page is open or the engine runs, and never downloads it: call it on a sign of intent, not at launch or on a timer.
    Prepare: (self: BrowserService, seconds: number?) -> (),
    --- Fires with a page's id when its address, title or loading changes, or it closes.
    Changed: Signal<string>,
    --- Fires when the element picker picks an element.
    Picked: Signal<BrowserPick>,
}

--- The active session's or draft's composer. The window half only: `ctx:GetService("Composer")`.
export type ComposerService = {
    --- Puts a pill at the caret, with a space on each side, and gives the composer the keyboard.
    InsertPill: (self: ComposerService, id: string, label: string) -> (),
    --- Whether the current screen has a composer (a draft or a root session).
    Active: (self: ComposerService) -> boolean,
    --- The composer's workspace folder (the active session's, or the draft's).
    Directory: (self: ComposerService) -> string,
    --- The model currently selected in the composer, when listed in the model catalog.
    Model: (self: ComposerService) -> ComposerModel?,
    --- Which composer picker overlay is currently open, or nil when none is open.
    Picker: (self: ComposerService) -> ComposerPicker?,
    --- Fires when a composer picker opens or closes, with the open picker kind or nil.
    PickerChanged: Signal<ComposerPicker?>,
}

--- Office documents laid out and drawn by the host, each on a thread of its own. The window half only: `ctx:GetService("Office")`.
export type OfficeService = {
    --- Reads a file of the session's folder `directory` through the server and, when its bytes are a zip archive, opens and lays it out as `kind`. Yields: call it from a handler, a task or a signal callback.
    OpenAsync: (self: OfficeService, path: string, directory: string, kind: OfficeKind) -> OfficeFile,
    --- Grows with every change its views draw: pages laid out, zoom, a sheet's frame. Read in a render, it redraws as it grows.
    Version: (self: OfficeService, document: OfficeDocument) -> number,
    --- Shows a workbook's sheet `index`, from 1.
    SetSheet: (self: OfficeService, document: OfficeDocument, index: number) -> (),
    --- The workbook's sheet on show, from 1: the one `Office:SetSheet` chose, or the one a link in the sheet went to. Reading it redraws the caller when it changes.
    Sheet: (self: OfficeService, document: OfficeDocument) -> number,
    --- Releases it and ends its thread.
    Close: (self: OfficeService, document: OfficeDocument) -> (),
}

--- The app's own updates (Desktop's `Updater` IPC). The host checks, downloads and verifies on its own schedule; a plugin draws where it stands and asks for a check or the install. Both halves: `ctx:GetService("Updater")`.
export type UpdaterService = {
    --- Where it stands; nil when no updater runs in this build. Read in a render, it redraws as it moves.
    State: (self: UpdaterService) -> UpdaterState?,
    --- Checks for a newer version now, joining a check already running, and resumes with the state the check settled on: `up-to-date`, `ready`, `download-required`, `error`, or the work under way. Yields: call it from a handler, a task or a signal callback.
    CheckAsync: (self: UpdaterService) -> UpdaterState,
    --- Restarts into the ready version, or opens a required download's page: resumes with nil once it handed off (a restart then quits the app on its own), else why it could not. Yields: call it from a handler, a task or a signal callback.
    InstallAsync: (self: UpdaterService) -> string?,
    --- Fires with each new state.
    StateChanged: Signal<UpdaterState?>,
}

--- Developer diagnostics. Local and beta builds only, except `Get` of an experiment and `HeapSnapshotAsync`; elsewhere the rest return nil. Both halves: `ctx:GetService("Diagnostics")`.
export type DiagnosticsService = {
    --- The process's memory counters.
    Memory: (self: DiagnosticsService) -> { [string]: any }?,
    --- A plugin's live costs. Defaults to this plugin.
    Metrics: (self: DiagnosticsService, pluginId: string?) -> PluginMetrics?,
    --- The ids of the plugins running in this window, for a console's target.
    Plugins: (self: DiagnosticsService) -> { string },
    --- A developer setting: an experiment (`sources.enabled` `"on"` or `"off"`, `sources.icons` `"site"` or `"letters"`), the animation clock (`speed` a number, `slow` a boolean), the tuning panel (`knobs` a boolean), or memory diagnostics (`memoryLog` `"off"`, `"on"`, or `"env"` when the environment turned them on for this launch), and the layout tools: `direction` (`"ltr"` or `"rtl"`, the window's layout direction) and `forceFocus` (a boolean: every control draws its keyboard focus ring). Read in a render, it redraws as the setting changes.
    Get: (self: DiagnosticsService, name: string) -> any,
    --- Changes a setting `Get` reads (`memoryLog` takes a boolean and keeps it across launches; `direction` overrides the language's direction for this launch, and setting the language's own direction drops the override), or opens a developer tool: `components` the components lab.
    Set: (self: DiagnosticsService, name: string, value: any) -> (),
    --- Captures heap snapshots in a new folder in the downloads folder: the app's own server's snapshot, where the memory is per process (`memory.json`), and `manifest.json`, written last. `path` is the folder; `error` says why nothing was captured, or which snapshot failed. Every build offers it. Yields: call it from a handler, a task or a signal callback.
    HeapSnapshotAsync: (self: DiagnosticsService) -> SaveResult,
}

--- Display sleep. Both halves: `ctx:GetService("Power")`.
export type PowerService = {
    --- Keeps the display awake until the lock is released or the plugin goes away.
    KeepAwake: (self: PowerService, reason: string) -> PowerLock,
}

--- QR codes. Both halves: `ctx:GetService("Qr")`.
export type QrService = {
    --- Encodes text as UTF-8 bytes at low error correction, in the smallest version that fits.
    Encode: (self: QrService, text: string) -> QrCode,
}

--- Credential prompts from child processes such as `ssh`. The app half only: `ctx:GetService("Askpass")`.
export type AskpassService = {
    --- Starts a loopback prompt endpoint.
    Listen: (self: AskpassService) -> AskpassListener,
}

--- Images for `ui.image`: loaded from a source or handed over, decoded off the UI thread by the window's image cache (PNG and APNG, JPEG, GIF, WebP, BMP, ICO, AVIF, ICNS and SVG). The window half only: `ctx:GetService("Images")`.
export type ImagesService = {
    --- The image at `src`, loading off the UI thread: an `https:` or `http:` URL, fetched as a browser loads an image (no credentials, 15 s, at most 8 MB); a path from `/`, read from the window's server with its sign-in (`/api/...`); a `data:` URL; or a file of the plugin's own folder by its relative path (`icons/logo.svg`). The same source is the same image, and its state is shared: one that failed fails again at once. `Images:State` says where it stands.
    Load: (self: ImagesService, src: string) -> ImageId,
    --- Where the image stands. Read in a render, it redraws as the image lands or fails.
    State: (self: ImagesService, image: ImageId) -> ImageState,
    --- Whether the `ui.video` playing `src` can't play: its box then shows what's behind it. Read in a render, it redraws when the clip fails; a render memoized with `view:Memo` should put it in the memo's version, as with `Images:State`.
    VideoFailed: (self: ImagesService, src: string) -> boolean,
    --- A PNG, JPEG, GIF or WebP image, decoded off the UI thread. Nil when refused.
    FromEncoded: (self: ImagesService, data: buffer | string) -> ImageId?,
    --- An SVG image.
    FromSvg: (self: ImagesService, svg: string) -> ImageId?,
    --- Straight-alpha RGBA8 pixels.
    FromRgba: (self: ImagesService, pixels: buffer, width: number, height: number) -> ImageId?,
    --- Frees an image once nothing draws it.
    Release: (self: ImagesService, image: ImageId) -> (),
}

--- The plugin's own messages in the interface language, and numbers, dates and lists written the way the app writes them. A plugin's catalogs are `locales/<id>.json` in its folder: flat objects of message keys, the app's format. Both halves: `ctx:GetService("I18n")`.
export type I18nService = {
    --- Message `key` in the interface language, else in `locales/en.json`, else the key itself. `{{name}}` placeholders take `args`, and a missing one reads empty. With a number `args.count`, the form `key.<category>` for the count's plural category is used when the catalog has it, else `key.other`, else `key`.
    T: (self: I18nService, key: string, args: { [string]: any }?) -> string,
    --- The interface language's catalog, or the English one, has message `key`.
    Has: (self: I18nService, key: string) -> boolean,
    --- The interface language.
    Locale: (self: I18nService) -> LocaleInfo,
    --- Which way the interface language reads. Lay out rows from the reading start; the host mirrors its own layout.
    Direction: (self: I18nService) -> TextDirection,
    --- The plural category the interface language uses for `count`.
    Plural: (self: I18nService, count: number) -> PluralCategory,
    --- `value` with the language's digits, separators and signs.
    Number: (self: I18nService, value: number, format: NumberFormat?) -> string,
    --- The moment `ms` (milliseconds since the Unix epoch) in local time, in `style`.
    Date: (self: I18nService, ms: number, style: DateStyle) -> string,
    --- A relative time: `-3, "day"` reads "3 days ago", `2, "hour"` "in 2 hours".
    RelativeTime: (self: I18nService, value: number, unit: TimeUnit) -> string,
    --- "today", "yesterday" or "tomorrow" for 0, -1 and 1 days from today, in the language. Nil for other days.
    RelativeDay: (self: I18nService, days: number) -> string?,
    --- Items joined as the language's long conjunction: "a, b, and c".
    List: (self: I18nService, items: { string }) -> string,
    --- Fires as the interface language changes. Everything drawn redraws on its own; connect for state you keep.
    Changed: Signal<LocaleInfo>,
}

-- Plugin contexts ---------------------------------------------------------------------------------

export type WindowContextCreateSignal = <T...>(self: WindowContext) -> ScriptSignal<T...>

--- What a plugin half's setup function receives. The window half's: see `plugin.defineWindow`.
declare extern type WindowContext with
    --- The plugin's id.
    id: string
    --- Which half this is.
    half: Half
    --- The contract version the host implements, such as `"1.0"`.
    contract: string
    --- A signal this plugin fires itself.
    CreateSignal: WindowContextCreateSignal
    --- A host-owned text field's content. The n-th editor a setup creates is the same field after a hot reload.
    function CreateEditor(self, initial: string?): Editor
    --- Carries plain data across a hot reload: the host calls `save` on the old code and `load` on the new.
    function BindHotState(self, save: () -> any, load: (state: any) -> ()): ()
    --- Returns a host service by name; each overload narrows the type. The app half alone has: Askpass.
    function GetService(self, name: "Sessions"): SessionsService
    --- Returns a host service by name; each overload narrows the type. The app half alone has: Askpass.
    function GetService(self, name: "Models"): ModelsService
    --- Returns a host service by name; each overload narrows the type. The app half alone has: Askpass.
    function GetService(self, name: "Server"): ServerService
    --- Returns a host service by name; each overload narrows the type. The app half alone has: Askpass.
    function GetService(self, name: "Servers"): ServersService
    --- Returns a host service by name; each overload narrows the type. The app half alone has: Askpass.
    function GetService(self, name: "Http"): HttpService
    --- Returns a host service by name; each overload narrows the type. The app half alone has: Askpass.
    function GetService(self, name: "Process"): ProcessService
    --- Returns a host service by name; each overload narrows the type. The app half alone has: Askpass.
    function GetService(self, name: "Store"): StoreService
    --- Returns a host service by name; each overload narrows the type. The app half alone has: Askpass.
    function GetService(self, name: "Contracts"): ContractsService
    --- Returns a host service by name; each overload narrows the type. The app half alone has: Askpass.
    function GetService(self, name: "Console"): ConsoleService
    --- Returns a host service by name; each overload narrows the type. The app half alone has: Askpass.
    function GetService(self, name: "Toast"): ToastService
    --- Returns a host service by name; each overload narrows the type. The app half alone has: Askpass.
    function GetService(self, name: "Popover"): PopoverService
    --- Returns a host service by name; each overload narrows the type. The app half alone has: Askpass.
    function GetService(self, name: "Dialog"): DialogService
    --- Returns a host service by name; each overload narrows the type. The app half alone has: Askpass.
    function GetService(self, name: "Menu"): MenuService
    --- Returns a host service by name; each overload narrows the type. The app half alone has: Askpass.
    function GetService(self, name: "Layout"): LayoutService
    --- Returns a host service by name; each overload narrows the type. The app half alone has: Askpass.
    function GetService(self, name: "Links"): LinksService
    --- Returns a host service by name; each overload narrows the type. The app half alone has: Askpass.
    function GetService(self, name: "Commands"): CommandsService
    --- Returns a host service by name; each overload narrows the type. The app half alone has: Askpass.
    function GetService(self, name: "Theme"): ThemeService
    --- Returns a host service by name; each overload narrows the type. The app half alone has: Askpass.
    function GetService(self, name: "App"): AppService
    --- Returns a host service by name; each overload narrows the type. The app half alone has: Askpass.
    function GetService(self, name: "System"): SystemService
    --- Returns a host service by name; each overload narrows the type. The app half alone has: Askpass.
    function GetService(self, name: "Notifications"): NotificationsService
    --- Returns a host service by name; each overload narrows the type. The app half alone has: Askpass.
    function GetService(self, name: "Workspace"): WorkspaceService
    --- Returns a host service by name; each overload narrows the type. The app half alone has: Askpass.
    function GetService(self, name: "Diff"): DiffService
    --- Returns a host service by name; each overload narrows the type. The app half alone has: Askpass.
    function GetService(self, name: "Terminal"): TerminalService
    --- Returns a host service by name; each overload narrows the type. The app half alone has: Askpass.
    function GetService(self, name: "Browser"): BrowserService
    --- Returns a host service by name; each overload narrows the type. The app half alone has: Askpass.
    function GetService(self, name: "Composer"): ComposerService
    --- Returns a host service by name; each overload narrows the type. The app half alone has: Askpass.
    function GetService(self, name: "Office"): OfficeService
    --- Returns a host service by name; each overload narrows the type. The app half alone has: Askpass.
    function GetService(self, name: "Updater"): UpdaterService
    --- Returns a host service by name; each overload narrows the type. The app half alone has: Askpass.
    function GetService(self, name: "Diagnostics"): DiagnosticsService
    --- Returns a host service by name; each overload narrows the type. The app half alone has: Askpass.
    function GetService(self, name: "Power"): PowerService
    --- Returns a host service by name; each overload narrows the type. The app half alone has: Askpass.
    function GetService(self, name: "Qr"): QrService
    --- Returns a host service by name; each overload narrows the type. The app half alone has: Askpass.
    function GetService(self, name: "Images"): ImagesService
    --- Returns a host service by name; each overload narrows the type. The app half alone has: Askpass.
    function GetService(self, name: "I18n"): I18nService
    --- Adds a command to the palette, its keybind and its slash command.
    function AddCommand(self, spec: CommandSpec): Contribution
    --- Adds a panel: tabs in the session's side region.
    function AddPanel(self, spec: PanelSpec): Contribution
    --- Adds a row to Settings › Plugins.
    function AddSetting(self, spec: SettingSpec): Contribution
    --- Adds a settings page, or a section on a host page.
    function AddSettingsPage(self, spec: SettingsPageSpec): Contribution
    --- Adds an item to a host menu.
    function AddMenuItem(self, spec: MenuItemSpec): Contribution
    --- Opens local links it matches.
    function AddLinkHandler(self, spec: LinkHandlerSpec): Contribution
    --- Adds a pill at the titlebar's end.
    function AddTitlebarItem(self, spec: TitlebarItemSpec): Contribution
    --- Adds a pill above the session composer.
    function AddComposerPill(self, spec: ComposerPillSpec): Contribution
    --- Renders fenced code blocks of some languages.
    function AddCodeBlock(self, spec: CodeBlockSpec): Contribution
    --- Adds a button beside code blocks' Copy button.
    function AddCodeBlockAction(self, spec: CodeBlockActionSpec): Contribution
    --- Fills one of the shell's slots.
    function AddSlot(self, spec: SlotSpec): Contribution
    --- Adds a color theme.
    function AddTheme(self, spec: ThemeSpec): Contribution
    --- Shows files with some extensions.
    function AddFileViewer(self, spec: FileViewerSpec): Contribution
    --- Adds a source of servers. Add it in the app half so every window shares one list.
    function AddServerSource(self, spec: ServerSourceSpec): ServerSource
    --- Adds a modal dialog, opened with `Dialog:Open`.
    function AddDialog(self, spec: DialogSpec): Contribution
    --- Adds popover content, opened with `Popover:Open` or `ui.flyoutRow`.
    function AddPopover(self, spec: PopoverSpec): Contribution
    --- Adds a tool to the app's client MCP server.
    function AddMcpTool(self, spec: McpToolSpec): Contribution
    --- Fires as the half goes away: the plugin turns off, reloads, or its window closes. The host disconnects every connection afterwards.
    Disposing: Signal<()>
    --- Fires as the window gains or loses focus.
    FocusChanged: Signal<boolean>
end

export type AppContextCreateSignal = <T...>(self: AppContext) -> ScriptSignal<T...>

--- What a plugin half's setup function receives. The app half's: see `plugin.defineApp`.
declare extern type AppContext with
    --- The plugin's id.
    id: string
    --- Which half this is.
    half: Half
    --- The contract version the host implements, such as `"1.0"`.
    contract: string
    --- A signal this plugin fires itself.
    CreateSignal: AppContextCreateSignal
    --- Carries plain data across a hot reload: the host calls `save` on the old code and `load` on the new.
    function BindHotState(self, save: () -> any, load: (state: any) -> ()): ()
    --- Returns a host service by name; each overload narrows the type. The window half alone has: Popover, Dialog, Menu, Layout, Links, Commands, Diff, Browser, Composer, Office, Images.
    function GetService(self, name: "Sessions"): SessionsService
    --- Returns a host service by name; each overload narrows the type. The window half alone has: Popover, Dialog, Menu, Layout, Links, Commands, Diff, Browser, Composer, Office, Images.
    function GetService(self, name: "Models"): ModelsService
    --- Returns a host service by name; each overload narrows the type. The window half alone has: Popover, Dialog, Menu, Layout, Links, Commands, Diff, Browser, Composer, Office, Images.
    function GetService(self, name: "Server"): ServerService
    --- Returns a host service by name; each overload narrows the type. The window half alone has: Popover, Dialog, Menu, Layout, Links, Commands, Diff, Browser, Composer, Office, Images.
    function GetService(self, name: "Servers"): ServersService
    --- Returns a host service by name; each overload narrows the type. The window half alone has: Popover, Dialog, Menu, Layout, Links, Commands, Diff, Browser, Composer, Office, Images.
    function GetService(self, name: "Http"): HttpService
    --- Returns a host service by name; each overload narrows the type. The window half alone has: Popover, Dialog, Menu, Layout, Links, Commands, Diff, Browser, Composer, Office, Images.
    function GetService(self, name: "Process"): ProcessService
    --- Returns a host service by name; each overload narrows the type. The window half alone has: Popover, Dialog, Menu, Layout, Links, Commands, Diff, Browser, Composer, Office, Images.
    function GetService(self, name: "Store"): StoreService
    --- Returns a host service by name; each overload narrows the type. The window half alone has: Popover, Dialog, Menu, Layout, Links, Commands, Diff, Browser, Composer, Office, Images.
    function GetService(self, name: "Contracts"): ContractsService
    --- Returns a host service by name; each overload narrows the type. The window half alone has: Popover, Dialog, Menu, Layout, Links, Commands, Diff, Browser, Composer, Office, Images.
    function GetService(self, name: "Console"): ConsoleService
    --- Returns a host service by name; each overload narrows the type. The window half alone has: Popover, Dialog, Menu, Layout, Links, Commands, Diff, Browser, Composer, Office, Images.
    function GetService(self, name: "Toast"): ToastService
    --- Returns a host service by name; each overload narrows the type. The window half alone has: Popover, Dialog, Menu, Layout, Links, Commands, Diff, Browser, Composer, Office, Images.
    function GetService(self, name: "Theme"): ThemeService
    --- Returns a host service by name; each overload narrows the type. The window half alone has: Popover, Dialog, Menu, Layout, Links, Commands, Diff, Browser, Composer, Office, Images.
    function GetService(self, name: "App"): AppService
    --- Returns a host service by name; each overload narrows the type. The window half alone has: Popover, Dialog, Menu, Layout, Links, Commands, Diff, Browser, Composer, Office, Images.
    function GetService(self, name: "System"): SystemService
    --- Returns a host service by name; each overload narrows the type. The window half alone has: Popover, Dialog, Menu, Layout, Links, Commands, Diff, Browser, Composer, Office, Images.
    function GetService(self, name: "Notifications"): NotificationsService
    --- Returns a host service by name; each overload narrows the type. The window half alone has: Popover, Dialog, Menu, Layout, Links, Commands, Diff, Browser, Composer, Office, Images.
    function GetService(self, name: "Workspace"): WorkspaceService
    --- Returns a host service by name; each overload narrows the type. The window half alone has: Popover, Dialog, Menu, Layout, Links, Commands, Diff, Browser, Composer, Office, Images.
    function GetService(self, name: "Terminal"): TerminalService
    --- Returns a host service by name; each overload narrows the type. The window half alone has: Popover, Dialog, Menu, Layout, Links, Commands, Diff, Browser, Composer, Office, Images.
    function GetService(self, name: "Updater"): UpdaterService
    --- Returns a host service by name; each overload narrows the type. The window half alone has: Popover, Dialog, Menu, Layout, Links, Commands, Diff, Browser, Composer, Office, Images.
    function GetService(self, name: "Diagnostics"): DiagnosticsService
    --- Returns a host service by name; each overload narrows the type. The window half alone has: Popover, Dialog, Menu, Layout, Links, Commands, Diff, Browser, Composer, Office, Images.
    function GetService(self, name: "Power"): PowerService
    --- Returns a host service by name; each overload narrows the type. The window half alone has: Popover, Dialog, Menu, Layout, Links, Commands, Diff, Browser, Composer, Office, Images.
    function GetService(self, name: "Qr"): QrService
    --- Returns a host service by name; each overload narrows the type. The window half alone has: Popover, Dialog, Menu, Layout, Links, Commands, Diff, Browser, Composer, Office, Images.
    function GetService(self, name: "Askpass"): AskpassService
    --- Returns a host service by name; each overload narrows the type. The window half alone has: Popover, Dialog, Menu, Layout, Links, Commands, Diff, Browser, Composer, Office, Images.
    function GetService(self, name: "I18n"): I18nService
    --- Adds an item to the app menu bar.
    function AddMenubarItem(self, spec: MenubarItemSpec): Contribution
    --- Adds a source of servers. Add it in the app half so every window shares one list.
    function AddServerSource(self, spec: ServerSourceSpec): ServerSource
    --- Adds a tool to the app's client MCP server.
    function AddMcpTool(self, spec: McpToolSpec): Contribution
    --- Fires as the half goes away: the plugin turns off, reloads, or its window closes. The host disconnects every connection afterwards.
    Disposing: Signal<()>
end

-- Modules -----------------------------------------------------------------------------------------

--- `require("@opencode/ui")`: declarative node constructors. Nodes build host elements directly; there is no virtual DOM.
export type UiLibrary = {
    --- A container with no direction of its own: children stack on top of each other.
    box: ((props: UiBoxProps, children: { UiNode }?) -> UiNode) & ((style: UiBoxProps, props: UiBoxProps?, children: { UiNode }?) -> UiNode),
    --- A horizontal stack.
    row: ((props: UiBoxProps, children: { UiNode }?) -> UiNode) & ((style: UiBoxProps, props: UiBoxProps?, children: { UiNode }?) -> UiNode),
    --- A vertical stack.
    col: ((props: UiBoxProps, children: { UiNode }?) -> UiNode) & ((style: UiBoxProps, props: UiBoxProps?, children: { UiNode }?) -> UiNode),
    --- A layout container whose border takes room in layout, snapped to device pixels at every display scale as a CSS `border-box` border does.
    frame: ((props: UiFrameProps, children: { UiNode }?) -> UiNode) & ((style: UiFrameProps, props: UiFrameProps?, children: { UiNode }?) -> UiNode),
    --- Flexible space in a stack.
    spacer: (props: UiSpacerProps?) -> UiNode,
    --- A 1 px rule, laid out as a 1px CSS border is: one device pixel up to 1.5x, two at 2x. A box whose height counts its border holds it beside its content.
    divider: (props: UiDividerProps?) -> UiNode,
    --- Text. Values after the props fill `{1}`, `{2}`... in `content`, written straight into the text: `ui.text("{1} / {2}", VALUE, read, write)`. Other braces stay as they are.
    text: ((content: string, props: UiTextProps?, ...string | number) -> UiNode) & ((content: string, style: UiTextProps, props: UiTextProps, ...string | number) -> UiNode),
    --- One of the app's icons.
    icon: ((name: IconName, props: UiIconProps?) -> UiNode) & ((name: IconName, style: UiIconProps, props: UiIconProps) -> UiNode),
    --- An image the host holds, or loads from `src`. While it loads its box shows the `placeholder`; `fallback` takes its place when it fails (a site's letter in place of its icon).
    image: ((props: UiImageProps, fallback: UiNode?) -> UiNode) & ((style: UiImageProps, props: UiImageProps, fallback: UiNode?) -> UiNode),
    --- A video playing muted on a loop while it shows, as a page's `<video autoplay loop muted playsinline>`. Until its first frame, and if it can't play, its box shows what's behind it.
    video: (props: UiVideoProps) -> UiNode,
    --- A number shown as text. Hosts that can animate changed digits do.
    metric: (props: UiMetricProps) -> UiNode,
    --- Text with the app's loading shimmer.
    shimmer: (content: string, props: UiTextProps?) -> UiNode,
    --- A keyboard shortcut chip.
    keybind: (props: UiKeybindProps) -> UiNode,
    --- The app's own button.
    button: (props: UiButtonProps) -> UiNode,
    --- The app's own icon button.
    iconButton: (props: UiIconButtonProps) -> UiNode,
    --- The app's own switch.
    switch: (props: UiSwitchProps) -> UiNode,
    --- Desktop's compact checkbox with its label.
    checkbox: (props: UiCheckboxProps) -> UiNode,
    --- A radio row.
    radio: (props: UiRadioProps) -> UiNode,
    --- The app's own text field.
    field: (props: UiFieldProps) -> UiNode,
    --- A listbox container (`role="listbox"`) of selectable `ui.option` rows.
    listbox: (props: UiListboxProps?, children: { UiNode }?) -> UiNode,
    --- One selectable option row (`role="option"`) inside a `ui.listbox`.
    option: (props: UiOptionProps?, children: { UiNode }?) -> UiNode,
    --- Desktop's select dropdown control.
    select: (props: UiSelectProps) -> UiNode,
    --- The app's segmented control.
    segmented: (props: UiSegmentedProps) -> UiNode,
    --- The app's spinner.
    loader: (props: UiLoaderProps?) -> UiNode,
    --- The app's progress ring: a track and an arc from twelve o'clock, clockwise.
    progressCircle: (props: UiProgressCircleProps) -> UiNode,
    --- A server's health indicator, for a server source's row.
    serverIndicator: (props: UiServerIndicatorProps) -> UiNode,
    --- A status capsule.
    pill: (props: UiPillProps) -> UiNode,
    --- Desktop's compact badge.
    badge: (props: UiBadgeProps) -> UiNode,
    --- A settings row: title and description beside a control.
    settingsRow: (props: UiSettingsRowProps, control: UiNode?) -> UiNode,
    --- A card of settings rows.
    settingsList: (props: UiSettingsListProps, rows: { UiNode }) -> UiNode,
    --- A popover row that opens a nested popover.
    flyoutRow: (props: UiFlyoutRowProps) -> UiNode,
    --- The app's dialog header: title, description and close button. Its close button closes this plugin's dialog, as Esc does.
    dialogHeader: (props: UiDialogHeaderProps) -> UiNode,
    --- Markdown, as the timeline draws a reply. Native: the host renders it.
    markdown: (props: UiMarkdownProps) -> UiNode,
    --- A long list that builds only its visible rows. Native: the host renders it.
    virtualList: (props: UiVirtualListProps) -> UiNode,
    --- A terminal. Native: the host renders it.
    terminalSurface: (props: UiTerminalSurfaceProps) -> UiNode,
    --- A terminal tab's label: its prompt glyph and title, at most 160 wide. Native: the host renders it.
    terminalTabLabel: (props: UiTerminalTabLabelProps) -> UiNode,
    --- A web page. Native: the host renders it.
    browserSurface: (props: UiBrowserSurfaceProps) -> UiNode,
    --- A highlighted file. Native: the host renders it.
    codeView: (props: UiCodeViewProps) -> UiNode,
    --- Changes, split or unified. Native: the host renders it.
    diffView: (props: UiDiffViewProps) -> UiNode,
    --- A folder's file tree. Native: the host renders it.
    fileTree: (props: UiFileTreeProps) -> UiNode,
    --- A resizable sidebar at a panel's start or end, its children stacked in it. Native: the host renders it.
    panelSidebar: (props: UiPanelSidebarProps, children: { UiNode }?) -> UiNode,
    --- The app's Open in split button for a path. Native: the host renders it.
    openInButton: (props: UiOpenInButtonProps) -> UiNode,
    --- A Word document's pages or a deck's slides, fit to the pane's width, with zoom controls. Native: the host renders it.
    officePages: (props: UiOfficePagesProps) -> UiNode,
    --- A workbook's sheet: its headers and the cells under the scroll position. Native: the host renders it.
    officeSheet: (props: UiOfficeSheetProps) -> UiNode,
    --- An interactive UI outline, from a ```` ```ui ```` code block. Native: the host renders it.
    genUi: (props: UiGenUiProps) -> UiNode,
    --- A literal color from `#RRGGBB` or `#RRGGBBAA`. Prefer a semantic token, which follows the theme.
    hex: (color: string) -> HexColor,
    --- A node for each item, as children to pass on: `build(item, index)`, memoized per item as `view:Memo` is, by its key and version. A render builds again only the items whose version changed. `key` and `version` are field names (`"id"`) or functions of the item. An item's key becomes its node's key, so keep keys unique in the render. Put everything the row draws with, the theme among it, in its version.
    each: <T>(items: { T }, key: string | ((item: T, index: number) -> string | number), version: string | ((item: T, index: number) -> MemoVersion), build: (item: T, index: number) -> UiNode) -> { UiNode },
}

--- `require("@opencode/plugin")`: defines a plugin's halves.
export type PluginLibrary = {
    --- The contract version the host implements, such as `"1.0"`.
    contract: string,
    --- Defines the window half: `setup` runs once per window. Return the result from `init.luau`.
    defineWindow: (setup: (ctx: WindowContext) -> ()) -> PluginHalf,
    --- Defines the app half: `setup` runs once per process, before any window half. Return the result from `app.luau`.
    defineApp: (setup: (ctx: AppContext) -> ()) -> PluginHalf,
}

-- Globals -----------------------------------------------------------------------------------------

--- Cooperative scheduling on the host's frame clock. Waiting costs no CPU.
declare task: {
    --- Runs a function or thread now, until it first yields.
    spawn: <A..., R...>(callback: ((A...) -> R...) | thread, A...) -> thread,
    --- Runs a function or thread at the end of the current handler or event batch, before the frame commits.
    defer: <A..., R...>(callback: ((A...) -> R...) | thread, A...) -> thread,
    --- Runs a function or thread after `seconds`.
    delay: <A..., R...>(seconds: number, callback: ((A...) -> R...) | thread, A...) -> thread,
    --- Yields for `seconds`, or until the next frame without it. Returns the seconds that passed. Not allowed in a render. Yields: call it from a handler, a task or a signal callback.
    wait: (seconds: number?) -> number,
    --- Cancels a scheduled or waiting thread and its pending request.
    cancel: (thread: thread) -> (),
}

--- Writes a warning to the console.
declare function warn(...: any): ()
