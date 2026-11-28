--!strict
-- The browser pane matching upstream `packages/gui-plugins/src/browser/panel.tsx` and `bar.css`:
-- toolbar (Back, Forward, Reload/Stop, left-elided punycode address bar with Site info & zoom
-- badge, Device toolbar toggle, '...' options menu), stacked bars (refusal notice with 500 ms
-- activation delay, failure notice, Find in page bar, Device toolbar), and page / home /
-- plain-language failure page.

local ui = require("@opencode/ui")
local address = require("./address")
local failure = require("./failure")
local find = require("./find")
local history = require("./history")
local hostDisplay = require("./host_display")
local refusal = require("./refusal")

local panel = {}

-- The new tab page lists this many of the newest visits.
local RECENT = 5

export type Env = {
    device: ((View, BrowserTab, PanelProps) -> ui.Node?)?,
    deviceOpen: ((BrowserTab) -> boolean)?,
    I18n: I18nService,
    Browser: BrowserService,
    Server: ServerService,
    System: SystemService,
    menuTrigger: boolean,
    -- The window's address editor and find editor.
    editor: Editor,
    findEditor: Editor,
    -- The address machine and each page's report counts.
    state: () -> address.State,
    epoch: (tab: BrowserTab) -> address.Epoch,
    visits: () -> { history.Visit },
    -- The system browser here can open the address.
    reachable: (url: string, tab: BrowserTab?) -> boolean,
    -- Popover states.
    siteOpen: () -> boolean,
    optionsOpen: () -> boolean,
    hasExtensions: (() -> boolean)?,
    extMenuOpen: (() -> boolean)?,
    serverConnected: (() -> boolean)?,
    -- Active notices and find state per tab.
    refusalNotice: (tab: BrowserTab) -> refusal.Notice?,
    failureNotice: (tab: BrowserTab) -> failure.Notice?,
    findView: (tab: BrowserTab) -> find.View?,
    -- Whether an agent just revealed `tab` within the 500 ms click-guard window.
    revealGuarded: (tab: BrowserTab) -> boolean,
    -- What the pane's controls do.
    act: (what: string, tab: BrowserTab?, arg: any?) -> (),
    -- The sign-in offer: a `bar` over a page or a `card` on the new tab page (`signins.luau`).
    signIns: ((view: View, tab: BrowserTab, variant: string) -> ui.Node?)?,
}

-- `text-13-regular` at `--line-height-compact`, as bar.css sets the field and its rows.
local TEXT = table.freeze({ size = 13, weight = "regular", lineHeight = 16, tracking = -0.04, noWrap = true })

local function text(content: string, color: string, extra: { [string]: any }?): ui.Node
    local props: { [string]: any } = table.clone(TEXT)
    props.color = color
    if extra then
        for k, v in extra do
            props[k] = v
        end
    end
    return ui.text(content, props :: any)
end

-- Desktop's `IconButton variant="ghost-muted" size="large"`: 28 square, 16 pt icon.
local function large(view: View, key: string, icon: string, tip: string, enabled: boolean, press: (ev: any?) -> (), extra: { [string]: any }?): ui.Node
    local props: { [string]: any } = {
        key = key,
        icon = icon,
        variant = "ghost-muted",
        size = "large",
        label = tip,
        help = tip,
        helpSide = "top",
        disabled = not enabled,
        onPress = if enabled then view:Handler(function(ev: any)
            press(ev)
        end) else nil,
    }
    if extra then
        for k, v in extra do
            props[k] = v
        end
    end
    return ui.iconButton(props :: any)
end

-- Preview kind for chrome markers (`local` | `remote` | `artifact` | nil).
local function tabPreviewKind(tab: BrowserTab, url: string): string?
    local tAny = tab :: any
    if type(tAny.PreviewKind) == "function" then
        local k = tAny:PreviewKind()
        if type(k) == "string" and k ~= "" then
            return k
        end
    end
    local lower = string.lower(url)
    if string.sub(lower, 1, 7) == "file://" then
        return "local"
    elseif string.sub(lower, 1, 12) == "workspace://" then
        return "remote"
    elseif string.sub(lower, 1, 11) == "artifact://" then
        return "artifact"
    end
    return nil
end

local function previewLabelKey(kind: string): string
    if kind == "remote" then
        return "preview.workspace"
    elseif kind == "artifact" then
        return "preview.artifact"
    end
    return "site.file"
end

-- Site button inside the address bar: 20 square with 4 corners; keeps pressed overlay while open.
-- Preview tabs show a folder glyph instead of sliders so users never mistake previews for live sites.
local function siteButton(view: View, iconName: IconName, tip: string, expanded: boolean, press: () -> ()): ui.Node
    local glyph = ui.icon(iconName, { size = 16, color = if expanded then "icon-base" else "icon-weak" })
    return ui.row({
        fill = if expanded then "surface-press" else nil,
        key = "browser-site",
        role = "button",
        -- A button inside the address field: the arrow, not the field's I-beam.
        cursor = "default",
        label = tip,
        help = tip,
        helpSide = "top",
        w = 20,
        h = 20,
        radius = 4,
        center = true,
        hover = "surface-hover",
        press = "surface-press",
        hoverTransition = "instant",
        onPress = view:Handler(function()
            press()
        end),
    }, { glyph })
end

-- The URL at rest (`host-display.ts` + `bar.css:110-118`):
-- - Canonical ASCII/punycode authority for HTTP(S) without credentials.
-- - Path drops first (`truncate = true, shrink = true`).
-- - When the host itself is wider than the field, elide from the LEFT (`…registrable.example`)
--   so the registrable right edge is never hidden behind a spoofed subdomain prefix.
local function display(url: string, availWidth: number): ui.Node
    local web = hostDisplay.parts(url)
    if web ~= nil then
        -- Conservative average glyph width (7.2 pt at 13 pt UI sans) for left-elision budget.
        local schemeW = #web.scheme * 7.0
        local maxHostChars = math.max(12, math.floor((availWidth - schemeW) / 7.2))
        local shownHost = hostDisplay.elideLeft(web.host, maxHostChars)
        local elided = shownHost ~= web.host
        local hostProps: { [string]: any } = { noWrap = true }
        if elided then
            hostProps.help = web.host
        end
        local items: { ui.Node } = {
            text(web.scheme, "text-faint"),
            text(shownHost, "text-chrome", hostProps),
        }
        if not elided and web.rest ~= "" then
            table.insert(items, text(web.rest, "text-faint", { truncate = true, shrink = true, minW = 0 }))
        end
        return ui.row({ key = "browser-address-url", dir = "ltr", grow = 1, shrink = true, minW = 0, align = "center" }, items)
    end
    local scheme, host, rest = address.parts(url)
    if host == "" then
        if scheme ~= "" then
            -- Official `bar.css:89-105`: `file:///` in faint color, file path in strong chrome color.
            local prefix = scheme
            local tail = rest
            if string.lower(scheme) == "file://" and string.sub(tail, 1, 1) == "/" then
                prefix = `{scheme}/`
                tail = string.sub(tail, 2)
            end
            return ui.row({ key = "browser-address-url", dir = "ltr", grow = 1, shrink = true, minW = 0, align = "center" }, {
                text(prefix, "text-faint"),
                text(tail, "text-chrome", { truncate = true, shrink = true, minW = 0 }),
            })
        end
        return text(url, "text-chrome", { truncate = true, shrink = true, minW = 0 })
    end
    local items: { ui.Node } = {}
    if scheme ~= "" then
        table.insert(items, text(scheme, "text-faint"))
    end
    table.insert(items, text(host, "text-chrome", { shrink = true, minW = 0, truncate = rest == "" }))
    if rest ~= "" then
        table.insert(items, text(rest, "text-faint", { truncate = true, shrink = true, minW = 0 }))
    end
    return ui.row({ key = "browser-address-url", dir = "ltr", grow = 1, shrink = true, minW = 0, align = "center" }, items)
end

-- `browser-address` (`panel.tsx:969-1097`, `live-browser-loaded-dark.png`):
-- 28 tall, 6 corners, surface-1 (surface-2 on hover at rest), focus outline while editing.
-- Contains Site info button / globe, address text/input, and Zoom badge when zoom != 100%.
local function field(env: Env, view: View, tab: BrowserTab, bar: address.View, zoom: number, fieldWidth: number): ui.Node
    local url = tab:Url()
    local page = if url == "about:blank" then "" else url
    local previewKind = tabPreviewKind(tab, bar.field)
    local site = bar.field ~= "" and not bar.editing
    local zoomed = page ~= "" and math.abs(zoom - 1) > 0.001
    local items: { ui.Node } = {}
    if site then
        local siteTip = if previewKind then env.I18n:T(previewLabelKey(previewKind)) else env.I18n:T("site.info")
        -- Site information opens 10 left of the button (`shift={-10}`), from a mark there.
        table.insert(items, ui.box({ key = "browser-site-frame" }, {
            siteButton(view, "outline-sliders", siteTip, env.siteOpen(), function()
                env.act("site", tab)
            end),
            ui.box({ key = "browser-site-anchor", pin = { left = -10, top = 0, bottom = 0 }, w = 1 }),
        }))
    else
        local tAny = tab :: any
        local liveFav: string? = if type(tAny.Favicon) == "function" then tAny:Favicon() else nil
        local chosen = bar.chosen
        -- Editing leading icon (`panel.tsx:277-285,987`): search glass for search rows,
        -- chosen page's favicon, or current page's favicon before typing.
        if bar.editing and chosen ~= nil and chosen.type == "search" then
            table.insert(items, ui.icon("magnifying-glass", { size = 16, color = "icon-weak" }))
        elseif bar.editing and chosen ~= nil and chosen.type == "page" and chosen.page.icon ~= nil and chosen.page.icon ~= "" then
            table.insert(items, ui.image({ src = chosen.page.icon, w = 16, h = 16 }, {
                ui.icon("outline-globe", { size = 16, color = "icon-weak" }),
            }))
        elseif bar.editing and not bar.typed and bar.field ~= "" and liveFav ~= nil and liveFav ~= "" then
            table.insert(items, ui.image({ src = liveFav, w = 16, h = 16 }, {
                ui.icon("outline-globe", { size = 16, color = "icon-weak" }),
            }))
        else
            table.insert(items, ui.icon("outline-globe", { size = 16, color = "icon-weak" }))
        end
    end
    local body: { ui.Node } = {}
    local textAvail = math.max(16, fieldWidth - 44 - (if zoomed then 48 else 0))
    if bar.editing then
        table.insert(body, ui.field({
            key = "browser-address-input",
            editor = env.editor,
            variant = "bare",
            dir = "ltr",
            placeholder = env.I18n:T("address.search"),
            keys = { "ArrowUp", "ArrowDown", "Tab", "Shift+Tab", "Escape", "Shift+Delete", "Shift+Backspace" },
            label = env.I18n:T("address.label"),
            grow = 1,
        }))
    elseif bar.field ~= "" then
        table.insert(body, display(bar.field, textAvail))
    else
        table.insert(body, text(env.I18n:T("address.search"), "text-faint", { truncate = true, shrink = true, minW = 0 }))
    end
    table.insert(items, ui.box({ key = "browser-address-text", grow = 1, shrink = true, minW = 0, h = "full", clip = true, align = "center" }, { ui.row({ w = "full", h = "full", clip = true, align = "center" }, body) }))
    -- Any zoom but 100% stays in sight; a click resets it.
    if zoomed then
        table.insert(items, ui.row({
            key = "browser-address-zoom",
            role = "button",
            -- `bar.css:137`: `cursor: default` inside the field.
            cursor = "default",
            label = env.I18n:T("zoom.reset"),
            help = env.I18n:T("zoom.reset"),
            helpSide = "top",
            h = 16,
            px = 4,
            radius = 2,
            border = "border-base",
            borderWidth = 0.5,
            fill = "surface-2",
            align = "center",
            hoverGroup = true,
            onPress = view:Handler(function()
                env.act("zoom", tab, "reset")
            end),
        }, { ui.text(env.I18n:T("zoom.percent", { percent = math.floor(zoom * 100 + 0.5) }), { size = 11, weight = "medium", lineHeight = 12, tracking = 0.05, tabular = true, noWrap = true, color = "text-muted", hoverGroupColor = "text-chrome" }) }))
    end
    local editingPage = bar.editing and not bar.typed and bar.field ~= ""
    return ui.row({
        key = "browser-address",
        -- `bar.css:22`: the whole field shows the I-beam, at rest and while it is edited (the field keeps a
        -- press handler then, so its own box, not only the input in it, is under the pointer).
        cursor = "text",
        dir = "ltr",
        grow = 1,
        shrink = true,
        minW = 0,
        h = 28,
        pl = if site then 4 else 8,
        pr = 8,
        gap = if site or editingPage then 8 else 6,
        radius = 6,
        align = "center",
        fill = "surface-1",
        hover = if bar.editing then nil else "surface-2",
        hoverTransition = "instant",
        border = if bar.editing then "border-focus" else nil,
        borderWidth = if bar.editing then 1 else nil,
        -- A press on the edited field's own box (its padding, its icon) keeps the keyboard in the input
        -- (`panel.tsx:1000-1006`); at rest a press starts the edit.
        onPress = view:Handler(function()
            env.act(if bar.editing then "focus" else "edit", tab)
        end),
    }, items)
end

-- The bar (`panel.tsx:910-1325`): 44 tall, 12 in, 8 between groups, hairline under it.
-- Right controls: [Extensions slot for WS-9] -> Comment on part of the page (pressed while picking, accented while
-- the picker or its comment is up) -> '...' Options trigger. The device toolbar's toggle is in Options > More tools.
local function bar(env: Env, view: View, tab: BrowserTab, state: address.View, width: number): ui.Node
    local loading = tab:Loading()
    local url = tab:Url()
    local page = if url == "about:blank" then "" else url
    local inspectable = page ~= "" and tab:LoadError() == nil
    local reload: ui.Node
    if loading then
        reload = ui.row({
            key = "browser-reload",
            role = "button",
            label = env.I18n:T("action.stop"),
            help = env.I18n:T("action.stop"),
            helpSide = "top",
            w = 28,
            h = 28,
            radius = 6,
            center = true,
            hover = "surface-hover",
            press = "surface-press",
            hoverTransition = "instant",
            onPress = view:Handler(function()
                tab:Stop()
            end),
        }, { ui.loader({ size = 16 }) })
    else
        reload = large(view, "browser-reload", "outline-rotate-clockwise", env.I18n:T("action.reload"), page ~= "", function(ev: any?)
            local tAny = tab :: any
            if type(ev) == "table" and ev.shift == true and type(tAny.ReloadIgnoreCache) == "function" then
                tAny:ReloadIgnoreCache()
            elseif type(tAny.ReloadFromClick) == "function" then
                tAny:ReloadFromClick()
            else
                tab:Reload()
            end
        end, { keybind = "browser.reload" })
    end
    local tAny = tab :: any
    local picking = tab:Inspecting() or (type(tAny.OverlayAccent) == "function" and tAny:OverlayAccent() == true)
    local showExt = env.hasExtensions ~= nil and env.hasExtensions()
    local rightCount = 2 + (if showExt then 1 else 0)
    local rightW = rightCount * 28 + math.max(0, rightCount - 1) * 4
    local fieldW = math.max(40, width - 24 - 92 - 8 - 8 - rightW)
    local rightControls: { ui.Node } = {}
    if showExt then
        table.insert(rightControls, large(
            view,
            "browser-extensions",
            "puzzle",
            env.I18n:T("settings.extensions.title"),
            true,
            function()
                env.act("extensions", tab)
            end,
            { pressed = env.extMenuOpen ~= nil and env.extMenuOpen(), menuTrigger = env.menuTrigger }
        ))
    end
    table.insert(rightControls, large(
        view,
        "browser-inspect",
        "select-element",
        env.I18n:T("inspect"),
        inspectable,
        function()
            env.act("inspect", tab)
        end,
        { pressed = tab:Inspecting(), color = if picking then "accent" else nil, keybind = "browser.inspect" }
    ))
    table.insert(rightControls, large(
        view,
        "browser-options",
        "outline-dots",
        env.I18n:T("options"),
        true,
        function()
            env.act("options", tab, width)
        end,
        {
            pressed = env.optionsOpen(),
            menuTrigger = env.menuTrigger,
        }
    ))
    return ui.col({ key = "browser-bar-frame", w = "full" }, {
        ui.row({ key = "browser-bar", w = "full", h = 43, pt = 1, px = 12, gap = 8, align = "center" }, {
            ui.row({ align = "center", gap = 4 }, {
                large(view, "browser-back", "outline-arrow-left", env.I18n:T("common.goBack"), tab:CanGoBack(), function()
                    tab:Back()
                end),
                large(view, "browser-forward", "outline-arrow-right", env.I18n:T("common.goForward"), tab:CanGoForward(), function()
                    tab:Forward()
                end),
                reload,
            }),
            field(env, view, tab, state, tab:Zoom(), fieldW),
            ui.row({ align = "center", gap = 4 }, rightControls),
        }),
        -- `border-b-[0.5px] border-v2-border-border-muted` (`panel.tsx:938`): Chromium clamps 0.5px to 1 device px at full colour.
        ui.col({ w = "full", h = 1, fill = "border-weak" }),
    })
end

-- A page in the suggestions or the new tab page: its icon, title and address, matches heavier.
local function marked(content: string, query: string, color: string, extra: { [string]: any }): ui.Node
    local parts = history.highlight(content, query)
    if #parts == 1 then
        return text(content, color, extra)
    end
    local items: { ui.Node } = {}
    for _, part in parts do
        table.insert(items, text(part.text, color, { weight = if part.match then "medium" else "regular" }))
    end
    local props: { [string]: any } = { align = "center", minW = 0, shrink = true, clip = true }
    for k, v in extra do
        props[k] = v
    end
    return ui.row(props :: any, items)
end

function panel.row(view: View, key: string, visit: history.Visit, query: string, width: number, suggestion: boolean, selected: boolean, open: () -> ()): ui.Node
    local textW = math.max(0, width - 16 - 16 - 8)
    local items: { ui.Node } = {}
    if visit.title ~= "" then
        table.insert(items, marked(visit.title, query, "text-chrome", { maxW = textW * 0.6, truncate = true, shrink = true, minW = 0 }))
        table.insert(items, text("-", "text-chrome"))
    end
    table.insert(items, marked(history.bare(visit.url), query, if suggestion then "text-chrome" else "text-muted", { grow = 1, truncate = true, shrink = true, minW = 0 }))
    return ui.row({
        key = key,
        dir = "ltr",
        role = "button",
        label = if visit.title ~= "" then visit.title else visit.url,
        w = "full",
        h = 28,
        px = 8,
        gap = 8,
        radius = 6,
        align = "center",
        fill = if selected then "surface-press" else nil,
        hover = "surface-hover",
        hoverTransition = "instant",
        onPress = view:Handler(function()
            open()
        end),
    }, {
        if visit.icon ~= nil and visit.icon ~= ""
            then ui.image({ src = visit.icon, w = 16, h = 16 }, {
                ui.icon("outline-globe", { size = 16, color = "icon-weak" }),
            })
            else ui.icon("outline-globe", { size = 16, color = "icon-weak" }),
        ui.row({ grow = 1, shrink = true, minW = 0, gap = 4, align = "center" }, items),
    })
end

-- The new tab page: Open a page and what it's for, top-aligned; or the newest visits.
local function home(env: Env, view: View, tab: BrowserTab, width: number): ui.Node
    local visits = env.visits()
    -- `panel.tsx:1474-1477`: the sign-in offer heads the new tab page while it offers. With no offer the page holds
    -- nothing above the prompt: a `ui.spacer` stand-in is flexible space and pushed the prompt to the pane's bottom.
    local offer = if env.signIns then env.signIns(view, tab, "card") else nil
    local items: { ui.Node } = if offer then { offer } else {}
    if #visits == 0 then
        table.insert(items, ui.col({ key = "browser-home-prompt", w = "full", maxW = 360, px = 24, pt = 80, gap = 16, align = "center" }, {
            ui.icon("outline-globe", { size = 20, color = "icon-weak" }),
            ui.col({ w = "full", gap = 8, align = "center" }, {
                ui.text(env.I18n:T("home.title"), { size = 13, weight = "medium", lineHeight = 16, color = "text-chrome", textAlign = "center" }),
                ui.text(env.I18n:T("home.description"), { size = 13, weight = "regular", lineHeight = 16, color = "text-muted", textAlign = "center", wrap = true }),
            }),
        }))
        return ui.col({ key = "browser-home", dir = "auto", w = "full", grow = 1, minH = 0, scrollY = true, align = "center" }, items)
    end
    local listW = math.min(608, width) - 48
    local rows: { ui.Node } = {}
    for n, visit in visits do
        if n > RECENT then
            break
        end
        table.insert(rows, panel.row(view, `browser-recent-{n}`, visit, "", listW, false, false, function()
            env.act("go", tab, visit.url)
        end))
    end
    table.insert(items, ui.col({ key = "browser-home-recent", w = "full", maxW = 608, px = 24, pt = 48, pb = 40, gap = 16 }, {
        ui.text(env.I18n:T("home.recent"), { size = 13, weight = "regular", lineHeight = 16, color = "text-muted" }),
        ui.col({ w = "full", gap = 8 }, rows),
    }))
    return ui.col({ key = "browser-home", dir = "auto", w = "full", grow = 1, minH = 0, scrollY = true, align = "center" }, items)
end

-- Plain-language failed-load error page (`failure.ts`, `failure-view.tsx`, `story-browser-load-failed-dark.png`).
local function failed(env: Env, view: View, tab: BrowserTab, loadError: string): ui.Node
    local info = failure.load(loadError)
    local actions: { failure.Action } = {}
    if info.reload then
        table.insert(actions, {
            key = "browser-failed-reload",
            label = env.I18n:T("action.reload"),
            run = function()
                tab:Reload()
            end,
        })
    end
    local url = tab:Url()
    if info.kind == "insecure" and url ~= "" and url ~= "about:blank" then
        local _, rawHost = address.parts(url)
        local hostName = string.match(rawHost, "^%[[^%]]*%]") or string.match(rawHost, "^[^:]*") or rawHost
        if address.loopback(hostName) and env.reachable(url, tab) then
            table.insert(actions, {
                key = "browser-failed-external",
                label = env.I18n:T("external.open"),
                run = function()
                    env.act("external", tab)
                end,
            })
        end
    end
    return failure.renderPage(view, env.I18n, info, actions)
end

local function omniboxRow(
    env: Env,
    view: View,
    tab: BrowserTab,
    n: number,
    row: address.Row,
    query: string,
    width: number,
    selected: boolean
): ui.Node
    local key = `browser-suggestion-{n}`
    if row.type == "search" then
        return ui.row({
            key = key,
            dir = "ltr",
            role = "button",
            label = `{row.text} - {env.I18n:T("address.search.web")}`,
            w = "full",
            h = 28,
            px = 8,
            gap = 8,
            radius = 6,
            align = "center",
            fill = if selected then "surface-press" else nil,
            hover = "surface-hover",
            hoverTransition = "instant",
            onPress = view:Handler(function()
                env.act("go", tab, row.url)
            end),
        }, {
            ui.icon("magnifying-glass", { size = 14, color = "icon-weak" }),
            ui.row({ grow = 1, shrink = true, minW = 0, gap = 4, align = "center" }, {
                text(row.text, "text-chrome", { truncate = true, shrink = true, minW = 0 }),
                text("—", "text-muted"),
                text(env.I18n:T("address.search.web"), "text-muted", { noWrap = true }),
            }),
        })
    elseif row.type == "address" then
        return ui.row({
            key = key,
            dir = "ltr",
            role = "button",
            label = row.text,
            w = "full",
            h = 28,
            px = 8,
            gap = 8,
            radius = 6,
            align = "center",
            fill = if selected then "surface-press" else nil,
            hover = "surface-hover",
            hoverTransition = "instant",
            onPress = view:Handler(function()
                env.act("go", tab, row.url)
            end),
        }, {
            ui.icon("outline-globe", { size = 16, color = "icon-weak" }),
            ui.row({ grow = 1, shrink = true, minW = 0, gap = 4, align = "center" }, {
                text(row.text, "text-chrome", { truncate = true, shrink = true, minW = 0 }),
            }),
        })
    end
    local page = row.page
    local textW = math.max(0, width - 16 - 16 - 8 - 28)
    -- Render IDN hosts as punycode (`xn--...`) in omnibox suggestions to prevent homograph spoofing.
    local shownUrl = (string.gsub(history.bare(page.url), "^([^/?#]+)/$", "%1"))
    local web = hostDisplay.parts(page.url)
    if web ~= nil then
        local hostBare = string.gsub(web.host, "^www%.", "")
        local restClean = if web.rest == "/" then "" else web.rest
        shownUrl = `{hostBare}{restClean}`
    end
    local items: { ui.Node } = {}
    if page.title ~= "" then
        table.insert(items, marked(page.title, query, "text-chrome", { maxW = textW * 0.55, truncate = true, shrink = true, minW = 0 }))
        table.insert(items, text("—", "text-muted"))
    end
    table.insert(items, marked(shownUrl, query, if page.title == "" then "text-chrome" else "text-muted", { grow = 1, truncate = true, shrink = true, minW = 0 }))
    return ui.row({
        key = key,
        dir = "ltr",
        role = "button",
        label = if page.title ~= "" then page.title else page.url,
        w = "full",
        h = 28,
        pl = 8,
        pr = 4,
        gap = 8,
        radius = 6,
        align = "center",
        fill = if selected then "surface-press" else nil,
        hover = "surface-hover",
        hoverTransition = "instant",
        onPress = view:Handler(function()
            env.act("go", tab, row.url)
        end),
    }, {
        if page.icon ~= nil and page.icon ~= ""
            then ui.image({ src = page.icon, w = 16, h = 16 }, {
                ui.icon("outline-globe", { size = 16, color = "icon-weak" }),
            })
            else ui.icon("outline-globe", { size = 16, color = "icon-weak" }),
        ui.row({ grow = 1, shrink = true, minW = 0, gap = 4, align = "center" }, items),
        ui.iconButton({
            key = `browser-suggestion-forget-{n}`,
            icon = "close",
            variant = "ghost-muted",
            size = "small",
            label = env.I18n:T("common.delete"),
            help = env.I18n:T("common.delete"),
            onPress = view:Handler(function()
                env.act("forget", tab, row.url)
            end),
        }),
    })
end

-- The suggestions under the field (`browser-suggestions`).
local function suggestions(env: Env, view: View, tab: BrowserTab, state: address.View, width: number): ui.Node?
    if not state.listed then
        return nil
    end
    -- The bar's right controls: the comment button, Options, and Extensions when shown.
    local rightCount = 2 + (if env.hasExtensions ~= nil and env.hasExtensions() then 1 else 0)
    local rightW = rightCount * 28 + math.max(0, rightCount - 1) * 4
    local fieldW = math.max(0, width - 24 - 92 - 8 - 8 - rightW)
    local rows: { ui.Node } = {}
    for n, row in state.rows do
        table.insert(rows, omniboxRow(env, view, tab, n, row, state.query, fieldW, n == state.active))
    end
    return ui.col({
        key = "browser-suggestions",
        role = "list",
        label = env.I18n:T("address.suggestions"),
        pin = { left = 12 + 92 + 8 - 8, top = 8 + 28 + 4 },
        w = fieldW + 16,
        pad = 8,
        radius = 12,
        fill = "bg-base",
        shadow = "floating",
    }, rows)
end

-- Failed download alert rendered through `FailureBar` (never generic "Request failed").
local function downloadFailure(env: Env, view: View, tab: BrowserTab): ui.Node?
    local files = tab:Files()
    local last = files[#files]
    if last == nil or last.state ~= "failed" then
        return nil
    end
    local words = if last.error == "too_large"
        then env.I18n:T("download.failed.tooLarge", { name = last.name })
        elseif last.error == "canceled" then env.I18n:T("download.failed.canceled", { name = last.name })
        elseif last.error == "too_many" then env.I18n:T("download.failed.tooMany", { name = last.name })
        else env.I18n:T("download.failed.other", { name = last.name, reason = last.error or "failed" })
    return failure.renderBar(view, env.I18n, "browser-download-error", words, nil, function()
        env.act("dismiss.download", tab)
    end)
end

-- A tab's pane.
function panel.render(env: Env, view: View, props: PanelProps): ui.Node
    local tab = if props.instance then env.Browser:Get(props.instance) else nil
    if tab == nil then
        return ui.col({ key = "browser-pane", w = "full", h = "full", fill = "bg-base" }, {})
    end
    local state = address.view(env.state(), tab.Id, env.epoch(tab), env.visits())
    local engine = env.Browser:Engine().state
    local held = env.revealGuarded(tab)
    local items: { ui.Node } = {}
    local chromeH = 0

    if engine ~= "unsupported" then
        table.insert(items, bar(env, view, tab, state, props.width))
        chromeH += 45
    end

    local url = tab:Url()
    local lowerUrl = string.lower(url)
    local isServerPreview = string.sub(lowerUrl, 1, 12) == "workspace://" or string.sub(lowerUrl, 1, 11) == "artifact://"
    local offline = isServerPreview and env.serverConnected ~= nil and not env.serverConnected()
    local suspended = offline or tab:Refusal() == "browser.proxy.required"
    -- A new tab loads about:blank while the engine starts (seconds on Windows): its page shows at once, as
    -- official's does. A tab opened on an address has that address from the start, so it never reads as blank.
    local blank = (url == "" or url == "about:blank")
        and (not tab:Loading() or url == "about:blank")
        and not state.navigating
        and tab:Refusal() ~= "browser.controlled.inherited_permission"

    -- Stacked bars in exact upstream order (`panel.tsx:1326-1417`):
    -- 0. The sign-in offer over a page (`panel.tsx:1357-1360`), never over the new tab page.
    -- `empty()` is a tab with no address, loading or not: the new tab page shows the card instead.
    local hasAddress = url ~= "" and url ~= "about:blank"
    if env.signIns and hasAddress and not suspended and engine ~= "unsupported" then
        local offer = env.signIns(view, tab, "bar")
        if offer then
            table.insert(items, offer)
            chromeH += 45
        end
    end

    -- 1. Refusal notice (with 500 ms activation delay + agent reveal hold)
    local loadErr = tab:LoadError()
    local refNotice = if loadErr == nil then env.refusalNotice(tab) else nil
    if refNotice ~= nil then
        table.insert(items, refusal.renderBar(
            view,
            env.I18n,
            refNotice,
            function()
                return env.System:Now()
            end,
            held,
            function()
                env.act("refusal.action", tab, refNotice)
            end,
            function()
                env.act("refusal.dismiss", tab, refNotice.token)
            end
        ))
        chromeH += 44
    end

    -- 2. Command / download failure bar
    local failNotice = env.failureNotice(tab)
    if failNotice ~= nil then
        table.insert(items, failure.renderBar(view, env.I18n, "browser-failure-bar", failNotice.text, failNotice.retry, function()
            env.act("failure.dismiss", tab, failNotice.id)
        end))
        chromeH += 40
    else
        local dlBar = downloadFailure(env, view, tab)
        if dlBar ~= nil then
            table.insert(items, dlBar)
            chromeH += 40
        end
    end

    -- 3. Find in page bar (`live-browser-find-dark.png`)
    local findView = if engine ~= "unsupported" and loadErr == nil then env.findView(tab) else nil
    if findView ~= nil then
        table.insert(items, find.render(
            view,
            env.I18n,
            env.findEditor,
            findView,
            function()
                env.act("find.previous", tab)
            end,
            function()
                env.act("find.next", tab)
            end,
            function()
                env.act("find.close", tab)
            end
        ))
        chromeH += 36
    end

    -- 4. Device emulation toolbar (`UP-039`, `#63819`)
    local toolbar = if env.device and engine ~= "unsupported" then env.device(view, tab, props) else nil
    if toolbar then
        table.insert(items, toolbar)
        chromeH += 36
    end

    -- 5. Page area: suspended/offline notice, failed-load plain page, empty new tab home, or live browser surface.
    if suspended then
        table.insert(items, ui.col({
            key = "browser-suspended",
            role = "status",
            w = "full",
            grow = 1,
            minH = 0,
            px = 24,
            center = true,
        }, {
            ui.text(env.I18n:T(if offline then "preview.offline" else "suspended"), {
                size = 13,
                weight = "regular",
                lineHeight = 20,
                color = "text-muted",
                textAlign = "center",
                wrap = true,
            }),
        }))
    elseif loadErr ~= nil and not tab:Loading() then
        table.insert(items, failed(env, view, tab, loadErr))
    elseif blank and (engine == "ready" or engine == "starting") then
        table.insert(items, home(env, view, tab, props.width))
    else
        local surfaceProps: { [string]: any } = {
            key = "browser-page",
            tab = tab,
            grow = 1,
        }
        -- Only pass device-preview area dimensions while the device toolbar is open so
        -- `viewport.rs` wraps Responsive mode in 14 pt resize gutters and drag handles.
        if toolbar ~= nil then
            surfaceProps.areaWidth = props.width
            surfaceProps.areaHeight = math.max(1, props.height - chromeH)
        end
        table.insert(items, ui.browserSurface(surfaceProps :: any))
    end

    local list = suggestions(env, view, tab, state, props.width)
    if list then
        table.insert(items, list)
    end
    return ui.col({ key = "browser-pane", w = "full", h = "full", minW = 0, fill = "bg-base", clip = true }, items)
end

-- Browser address bar at the top of the side panel's New tab view (`packages/gui-plugins/src/browser/new-tab.tsx`, opencode#66818).
function panel.newTabBar(
    env: Env,
    view: View,
    width: number,
    onEdit: () -> (),
    onFocus: () -> (),
    onGo: (url: string) -> (),
    onForget: (url: string) -> ()
): ui.Node
    local barState = address.view(env.state(), address.NEW_TAB_ID, address.BLANK_EPOCH, env.visits())
    local rightW = 2 * 28 + 4
    local fieldW = math.max(40, width - 24 - 92 - 8 - 8 - rightW)
    local fieldItems: { ui.Node } = {}
    local chosen = barState.chosen
    if barState.editing and chosen ~= nil and chosen.type == "search" then
        table.insert(fieldItems, ui.icon("magnifying-glass", { size = 16, color = "icon-weak" }))
    elseif barState.editing and chosen ~= nil and chosen.type == "page" and chosen.page.icon ~= nil and chosen.page.icon ~= "" then
        table.insert(fieldItems, ui.image({ src = chosen.page.icon, w = 16, h = 16 }, {
            ui.icon("outline-globe", { size = 16, color = "icon-weak" }),
        }))
    else
        table.insert(fieldItems, ui.icon("outline-globe", { size = 16, color = "icon-weak" }))
    end
    local body: { ui.Node } = {}
    if barState.editing then
        table.insert(body, ui.field({
            key = "browser-address-input",
            editor = env.editor,
            variant = "bare",
            dir = "ltr",
            placeholder = env.I18n:T("address.search"),
            keys = { "ArrowUp", "ArrowDown", "Tab", "Shift+Tab", "Escape", "Shift+Delete", "Shift+Backspace" },
            label = env.I18n:T("address.label"),
            grow = 1,
        }))
    elseif barState.field ~= "" then
        table.insert(body, display(barState.field, math.max(16, fieldW - 44)))
    else
        table.insert(body, text(env.I18n:T("address.search"), "text-faint", { truncate = true, shrink = true, minW = 0 }))
    end
    table.insert(fieldItems, ui.box({ key = "browser-address-text", grow = 1, shrink = true, minW = 0, h = "full", clip = true, align = "center" }, {
        ui.row({ w = "full", h = "full", clip = true, align = "center" }, body),
    }))
    local addrField = ui.row({
        key = "browser-address",
        cursor = "text",
        dir = "ltr",
        grow = 1,
        shrink = true,
        minW = 0,
        h = 28,
        pl = 8,
        pr = 8,
        gap = 6,
        radius = 6,
        align = "center",
        fill = "surface-1",
        hover = if barState.editing then nil else "surface-2",
        hoverTransition = "instant",
        border = if barState.editing then "border-focus" else nil,
        borderWidth = if barState.editing then 1 else nil,
        onPress = view:Handler(function()
            if barState.editing then
                onFocus()
            else
                onEdit()
            end
        end),
    }, fieldItems)
    local noop = function() end
    local children: { ui.Node } = {
        ui.row({ key = "browser-bar", w = "full", h = 43, pt = 1, px = 12, gap = 8, align = "center", fill = "bg-base" }, {
            ui.row({ align = "center", gap = 4 }, {
                large(view, "browser-back", "outline-arrow-left", env.I18n:T("common.goBack"), false, noop),
                large(view, "browser-forward", "outline-arrow-right", env.I18n:T("common.goForward"), false, noop),
                large(view, "browser-reload", "outline-rotate-clockwise", env.I18n:T("action.reload"), false, noop, { keybind = "browser.reload" }),
            }),
            addrField,
            ui.row({ align = "center", gap = 4 }, {
                large(view, "browser-inspect", "select-element", env.I18n:T("inspect"), false, noop, { keybind = "browser.inspect" }),
                large(view, "browser-options", "outline-dots", env.I18n:T("options"), false, noop),
            }),
        }),
        ui.col({ w = "full", h = 1, fill = "border-weak" }),
    }
    if barState.listed then
        local sRows: { ui.Node } = {}
        local proxyEnv: Env = table.clone(env)
        proxyEnv.act = function(what: string, _tab: BrowserTab?, arg: any?)
            if what == "go" then
                onGo(arg :: string)
            elseif what == "forget" then
                onForget(arg :: string)
            end
        end
        for n, r in barState.rows do
            table.insert(sRows, omniboxRow(proxyEnv, view, nil :: any, n, r, barState.query, fieldW, n == barState.active))
        end
        table.insert(children, ui.col({
            key = "browser-suggestions",
            role = "list",
            label = env.I18n:T("address.suggestions"),
            pin = { left = 12 + 92 + 8 - 8, top = 8 + 28 + 4 },
            w = fieldW + 16,
            pad = 8,
            radius = 12,
            fill = "bg-base",
            shadow = "floating",
        }, sRows))
    end
    return ui.col({ key = "browser-bar-frame", w = "full" }, children)
end

-- Recent browser pages below the side panel's New tab launchers (`new-tab.tsx:66-85`, opencode#66818).
function panel.newTabRecent(env: Env, view: View, width: number, onOpen: (url: string) -> ()): ui.Node?
    local list = env.visits()
    if #list == 0 then
        return nil
    end
    local listW = math.max(120, math.min(560, width - 48))
    local rows: { ui.Node } = {
        ui.box({ px = 8, pb = 4 }, {
            ui.text(env.I18n:T("home.recent"), { size = 13, weight = "regular", lineHeight = 16, color = "text-muted" }),
        }),
    }
    for n, visit in list do
        if n > address.SUGGESTIONS then
            break
        end
        table.insert(rows, panel.row(view, `browser-new-tab-recent-{n}`, visit, "", listW, false, false, function()
            onOpen(visit.url)
        end))
    end
    return ui.col({ key = "browser-new-tab-recent", w = "full", pt = 12, gap = 4 }, rows)
end

return panel
