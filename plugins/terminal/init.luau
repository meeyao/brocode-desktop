--!strict
-- Desktop's built-in terminal plugin (`packages/gui-plugins/src/terminal`): one tab per terminal of
-- the session (`terminal:<pty id>`), titled as Desktop titles them, Toggle terminal (Ctrl+`) and
-- New terminal (Ctrl+Alt+T), the + menu's Terminal row, and the narrow window's Terminal view.
-- The shells, their screens and their keys are the host's terminal engine's (`Terminal`).

local plugin = require("@opencode/plugin")
local ui = require("@opencode/ui")

-- Default titles, including those of releases that created them in the interface language
-- (title.ts): "Terminal 2" reads as the language's own.
local NUMBERED: { { string } } = {
    { "Terminal ", "" },
    { "محطة طرفية ", "" },
    { "Терминал ", "" },
    { "ターミナル ", "" },
    { "터미널 ", "" },
    { "เทอร์มินัล ", "" },
    { "终端 ", "" },
    { "終端機 ", "" },
}

-- The number of a default title such as "Terminal 2" (`titleNumber`).
local function titleNumber(title: string): number?
    for _, parts in NUMBERED do
        local before = parts[1]
        if string.sub(title, 1, #before) == before then
            local digits = string.sub(title, #before + 1)
            if string.match(digits, "^[1-9]%d*$") then
                return tonumber(digits)
            end
        end
    end
    return nil
end

return plugin.defineWindow(function(ctx)
    local I18n = ctx:GetService("I18n")
    local Sessions = ctx:GetService("Sessions")
    local Terminal = ctx:GetService("Terminal")

    -- A terminal's label (`terminalTabLabel`): a default title in the interface language, else
    -- the server's title; one the server hasn't named yet by its place.
    local function label(info: TerminalInfo): string
        local number = titleNumber(info.title)
        if number then
            return I18n:T("title.numbered", { number = number })
        end
        if info.title ~= "" then
            return info.title
        end
        if info.number > 0 then
            return I18n:T("title.numbered", { number = info.number })
        end
        return I18n:T("tab.title")
    end

    local function titles(sid: string): { [string]: string }
        local out: { [string]: string } = {}
        for _, info in Terminal:List(sid, {}) do
            out[info.id] = label(info)
        end
        return out
    end

    ctx:AddPanel({
        id = "main",
        title = I18n:T("tab.title"),
        icon = "terminal",
        launcher = false,
        -- A tab whose terminal the server no longer lists (closed here or elsewhere, or exited)
        -- leaves the strip.
        transient = true,
        pane = "dock",
        mobile = { title = I18n:T("tab.title"), order = 30, kind = "menu", icon = "terminal" },
        -- One tab per terminal of the session; until the server lists them, the stored tabs stay
        -- listed, hidden, so a restore keeps them.
        list = function(sid: string, open: { string }): { PanelTab }
            local out: { PanelTab } = {}
            for _, info in Terminal:List(sid, open) do
                table.insert(out, { instance = info.id, title = label(info), icon = "terminal", hidden = info.hidden })
            end
            return out
        end,
        label = function(_view, props)
            local session = Terminal:Attach(props.instance or "")
            if session == nil then
                return nil
            end
            local title = titles(props.sessionId)[props.instance or ""] or I18n:T("tab.title")
            return ui.terminalTabLabel({ session = session, title = title, selected = props.active })
        end,
        render = function(_view, props)
            if props.placement == "mobile" then
                return ui.terminalSurface({ narrow = true, sessionId = props.sessionId, grow = 1 })
            end
            return ui.terminalSurface({ session = Terminal:Attach(props.instance or ""), width = props.width, height = props.height, focused = props.active, grow = 1 })
        end,
        -- A selected tab selects its terminal, and takes the keyboard unless a restore selected it.
        onFocus = function(props)
            if props.instance then
                Terminal:Select(props.sessionId, props.instance, props.restored ~= true)
            end
        end,
        -- Closing a terminal's tab ends its shell, as Desktop's does.
        onClose = function(props)
            local session = if props.instance then Terminal:Attach(props.instance) else nil
            if session then
                session:Close()
            end
        end,
    })

    ctx:AddCommand({
        id = "toggle",
        title = I18n:T("command.toggle"),
        group = I18n:T("command.category.view"),
        section = "terminal",
        slash = { name = "terminal", after = "open" },
        featured = true,
        bind = "ctrl+`",
        editable = true,
        requiresSession = true,
        run = function()
            Terminal:Toggle()
        end,
    })

    ctx:AddCommand({
        id = "new",
        title = I18n:T("command.new"),
        description = I18n:T("command.new.description"),
        group = I18n:T("command.category"),
        section = "terminal",
        bind = "ctrl+alt+t",
        editable = true,
        requiresSession = true,
        run = function()
            if Terminal:Available() then
                Terminal:New(nil)
            end
        end,
    })

    -- Shows the session's terminal for the agent, answering with the terminal it shows.
    ctx:AddMcpTool({
        name = "open_terminal",
        description = "Show the integrated terminal dock in Brocode for this session's workspace.",
        inputSchema = {
            type = "object",
            properties = {
                newTab = { type = "boolean", description = "Create a new terminal tab instead of reusing the active one (default false)." },
            },
            additionalProperties = false,
        },
        call = function(args, context)
            local shown = Terminal:OpenAsync(context.sessionId, args.newTab == true)
            return { open = true, tabID = shown.id, placement = shown.placement }
        end,
    })

    -- The + menu's and New tab's Terminal row (order 20): it opens a new terminal. A read-only
    -- window offers none.
    ctx:AddMenuItem({
        menu = "session.panel",
        id = "new",
        title = I18n:T("tab.title"),
        icon = "terminal",
        keybind = "terminal.new",
        order = 20,
        when = function(sid: string): boolean
            return Sessions:Get(sid) ~= nil and Terminal:Available()
        end,
        run = function(sid: string?)
            Terminal:New(sid)
        end,
    })
end)
