--!strict
-- Desktop's built-in review plugin (`packages/gui-plugins/src/review`): the Review tab of a
-- session's side region (its title the count of changed files), its narrow window's Changes
-- view, Open review (Ctrl+Alt+R, ⌃⌘R on macOS) and the + menu's Review row, the `open_diff` MCP
-- tool, and the `review.changes` and `review.details` contracts the Files tree and the session
-- details read. The changes themselves come from the host's diff engine (`Diff`).

local plugin = require("@opencode/plugin")
local kinds = require("@self/kinds")
local panel = require("@self/panel")

-- The tab's id: its key is `review:changes`, as Desktop's.
local TAB = "changes"

return plugin.defineWindow(function(ctx)
    local I18n = ctx:GetService("I18n")
    local Sessions = ctx:GetService("Sessions")
    local Layout = ctx:GetService("Layout")
    local Store = ctx:GetService("Store")
    local Contracts = ctx:GetService("Contracts")
    local App = ctx:GetService("App")
    local Workspace = ctx:GetService("Workspace")
    local env: panel.Env = {
        I18n = I18n,
        Diff = ctx:GetService("Diff"),
        Layout = Layout,
        Menu = ctx:GetService("Menu"),
        Theme = ctx:GetService("Theme"),
        menuTrigger = true,
        docs = {},
        filter = ctx:CreateEditor(""),
        -- Desktop's `layout` review style (split) and `review-panel-v2` expand mode (collapse).
        style = Store:Value("diff", { diffStyle = "split" :: panel.DiffStyle }),
        expand = Store:Value("panel", { expandMode = "collapse" :: panel.ExpandMode }),
        source = Store:SessionValue("session", {} :: { mode: DiffSource?, file: string? }),
    }

    env.Diff:SetMode(env.style:Get().diffStyle)

    -- Enter in the filter shows the first file that matches (`applyFileListKeyDown`).
    env.filter.Submitted:Connect(function(text: string)
        local session = Sessions:Routed()
        local query = string.lower(string.gsub(text, "^%s*(.-)%s*$", "%1"))
        if session == nil or query == "" then
            return
        end
        local doc = panel.doc(env, session.id)
        for _, path in kinds.sorted(kinds.paths(doc:Files())) do
            if string.find(string.lower(path), query, 1, true) then
                panel.focus(env, session.id, path)
                return
            end
        end
    end)

    local function count(sid: string): number
        return panel.doc(env, sid):Totals().files
    end

    -- Opens the review tab, the side region with it, unless the session has no project (`show`).
    local function show(sid: string?)
        local session = if sid then Sessions:Get(sid) else Sessions:Routed()
        if session and session.projectId and not Layout:Narrow() then
            Layout:OpenPanel(TAB, { sessionId = session.id })
        end
    end

    ctx:AddPanel({
        id = TAB,
        title = I18n:T("tab.title"),
        icon = "review",
        launcher = false,
        tabTitle = function(sid: string): string
            local n = count(sid)
            return if n > 0 then I18n:T("tab.count", { count = n }) else I18n:T("tab.title")
        end,
        -- The split diff needs the wider session minimum while the side region is open: as the diffs
        -- draw, so the region resizes in the frame they change.
        isWide = function(): boolean
            return env.Diff:Mode() == "split"
        end,
        -- Closed, it lists as "Review" rather than its count.
        closed = { title = I18n:T("tab.title"), icon = "review" },
        mobile = { title = I18n:T("mobile.title", { count = 0 }), order = 10, kind = "tab" },
        render = function(view, props)
            if props.placement == "mobile" then
                return panel.mobile(env, view, props)
            end
            return panel.wide(env, view, props)
        end,
    })

    ctx:AddCommand({
        id = "open",
        title = I18n:T("command.open"),
        group = I18n:T("command.category.view"),
        section = "session",
        bind = if App.Platform == "macos" then "ctrl+mod+r" else "ctrl+alt+r",
        editable = true,
        requiresSession = true,
        run = function()
            show(nil)
        end,
    })

    -- The + menu's and New tab's Review row (order 10), its count of changed files once there are
    -- any; New tab leaves it out while the strip holds the tab.
    ctx:AddMenuItem({
        menu = "session.panel",
        id = "open",
        title = I18n:T("tab.title"),
        icon = "review",
        keybind = "review.open",
        order = 10,
        when = function(sid: string): boolean
            local session = Sessions:Get(sid)
            return session ~= nil and session.projectId ~= nil and not Layout:Narrow()
        end,
        opened = function(sid: string): boolean
            return Layout:PanelState(TAB, sid) ~= "closed"
        end,
        detail = function(sid: string): string?
            local n = count(sid)
            return if n > 0 then I18n:T("menu.count", { count = n }) else nil
        end,
        run = function(sid: string?)
            show(sid)
        end,
    })

    local VIEWS: { [string]: DiffSource } = { uncommitted = "git", branch = "branch", turn = "turn" }

    ctx:AddMcpTool({
        name = "open_diff",
        description = "Open the Changes diff view in the Brocode side panel, optionally scoped to a mode and file.",
        inputSchema = {
            type = "object",
            properties = {
                path = { type = "string", description = "Optional file path in Changes to reveal and scroll to." },
                mode = { type = "string", enum = { "uncommitted", "branch", "turn" }, description = "Which changes to show (defaults to the session's current mode)." },
                view = { type = "string", enum = { "unified", "split" }, description = "Unified or split diff layout." },
                line = { type = "integer", minimum = 1, description = "Optional 1-based line number in the new file to scroll near when 'path' is set." },
            },
            additionalProperties = false,
        },
        call = function(args, context)
            local sid = context.sessionId
            local mode = args.mode
            if mode ~= nil and VIEWS[mode] == nil then
                error(`Invalid diff mode '{mode}'`)
            end
            if args.view ~= nil and args.view ~= "unified" and args.view ~= "split" then
                error(`Invalid diff view '{args.view}'`)
            end
            if mode then
                panel.setSource(env, sid, VIEWS[mode])
            end
            if args.view then
                panel.setStyle(env, args.view)
            end
            local doc = panel.doc(env, sid)
            local path: string? = nil
            if type(args.path) == "string" and args.path ~= "" then
                path = Workspace:Relative(args.path, context.directory)
                if path == nil or string.find(`/{path}/`, "/%.%./") then
                    error(`Path '{args.path}' is outside the session folder '{context.directory}'`)
                end
                panel.focus(env, sid, path :: string)
            end
            Layout:OpenPanel(TAB, { sessionId = sid })
            local shown: { [string]: string } = { git = "uncommitted", branch = "branch", turn = "turn" }
            return { mode = shown[if mode then VIEWS[mode] else doc:Source()], view = args.view or env.style:Get().diffStyle, path = path }
        end,
    })

    -- `Changes` (`@opencode/plugin/gui/review`): the routed session's changes as the panel shows
    -- them, for the Files tree's change letters and the session details.
    Contracts:Provide("review.changes", {
        diffs = function(sid: string): { DiffFile }
            return panel.cached(env, sid).files
        end,
        ready = function(sid: string): boolean
            return panel.doc(env, sid):Status() ~= "loading"
        end,
        kinds = function(sid: string): { [string]: ChangeKind }
            return panel.cachedKinds(env, sid)
        end,
        kindsVersion = function(sid: string): number
            return panel.doc(env, sid):Tree():Version()
        end,
        active = function(sid: string): string?
            return panel.doc(env, sid):Active()
        end,
        focus = function(sid: string, path: string)
            show(sid)
            panel.focus(env, sid, path)
        end,
        open = function(sid: string)
            Layout:OpenPanel(TAB, { sessionId = sid })
        end,
    })

    -- `ReviewDetails`: the session folder's uncommitted changes and how their read went.
    Contracts:Provide("review.details", {
        view = function(sid: string): { [string]: any }
            local doc = panel.doc(env, sid)
            local status = doc:Status()
            if status == "ready" then
                return { status = "ready", rows = doc:Files(), version = doc:Version() }
            end
            return { status = status }
        end,
        retry = function(sid: string)
            panel.doc(env, sid):Retry()
        end,
    })

    -- A changed source or file set redraws the tab's title and the menu's count.
    ctx.Disposing:Connect(function()
        for _, doc in env.docs do
            doc:Close()
        end
    end)
end)
