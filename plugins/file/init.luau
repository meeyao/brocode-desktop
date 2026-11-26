--!strict
-- Desktop's built-in file plugin (`packages/gui-plugins/src/file`): a session's file tabs in its
-- side region (`file:open`, the Open file launcher, and `file://<path>`, one per open file), Open
-- file (Ctrl+Alt+P, ^?P on macOS) and the + menu's Open file row, and the narrow window's Files
-- view. The tree, the files and their code come from the host's file engine (`Workspace`); the
-- change letters from the review's `review.changes`.

local plugin = require("@opencode/plugin")
local panel = require("@self/panel")

-- The panel's id: its tabs key as `file:<instance>`, as Desktop's.
local PANEL = "main"

return plugin.defineWindow(function(ctx)
    local I18n = ctx:GetService("I18n")
    local Sessions = ctx:GetService("Sessions")
    local Layout = ctx:GetService("Layout")
    local App = ctx:GetService("App")
    local Workspace = ctx:GetService("Workspace")
    local Contracts = ctx:GetService("Contracts")
    local env: panel.Env = {
        I18n = I18n,
        Layout = Layout,
        Workspace = Workspace,
        changes = Contracts:Use("review.changes"),
        filter = ctx:CreateEditor(""),
    }

    -- Opens the Open file launcher in the routed session's side region and focuses its filter
    -- (`openPicker`): the one tab Desktop's narrow screens keep.
    local function browse(sid: string?)
        local session = if sid then Sessions:Get(sid) else Sessions:Routed()
        if session == nil then
            return
        end
        Layout:OpenPanel(PANEL, { sessionId = session.id, instance = panel.OPEN })
        env.filter:Focus()
    end

    -- Enter in the filter opens the file it found first, kept (`onSelectPermanent`); the filter
    -- keeps its text.
    env.filter.Submitted:Connect(function(text: string)
        local session = Sessions:Routed()
        local first = if session then panel.found(env, session.directory, text) else nil
        if session and first then
            panel.open(env, session.id, first)
        end
    end)

    ctx:AddPanel({
        id = PANEL,
        title = I18n:T("command.open"),
        icon = "file-tree",
        launcher = false,
        list = function(sid: string, open: { string }): { PanelTab }
            return panel.tabs(env, sid, open)
        end,
        mobile = { title = I18n:T("mobile.title"), order = 20, kind = "menu", icon = "folder" },
        render = function(view, props)
            if props.placement == "mobile" then
                return panel.narrow(env, props)
            end
            return panel.wide(env, view, props)
        end,
    })

    ctx:AddCommand({
        id = "browse",
        title = I18n:T("command.open"),
        group = I18n:T("command.category.view"),
        section = "session",
        bind = if App.Platform == "macos" then "ctrl+mod+p" else "ctrl+alt+p",
        editable = true,
        requiresSession = true,
        run = function()
            browse(nil)
        end,
    })

    -- Opens a workspace file for the agent, as Desktop's file plugin's `open_file` does.
    ctx:AddMcpTool({
        name = "open_file",
        description = "Open a workspace file in the Brocode side panel, optionally selecting a line or line range.",
        inputSchema = {
            type = "object",
            properties = {
                path = { type = "string", description = "Workspace-relative or absolute file path inside the session folder." },
                startLine = { type = "integer", minimum = 1, description = "1-based start line to select and scroll into view." },
                endLine = { type = "integer", minimum = 1, description = "1-based end line (defaults to startLine)." },
                preview = { type = "boolean", description = "Open in the transient preview tab instead of pinning (default false)." },
            },
            required = { "path" },
            additionalProperties = false,
        },
        call = function(args, context)
            local raw = if type(args.path) == "string" then args.path else ""
            if string.match(raw, "^%s*$") then
                error("Missing 'path'", 0)
            end
            local session = Sessions:Get(context.sessionId)
            local dir = if session and session.directory ~= "" then session.directory else context.directory
            local norm = string.gsub(raw, "\\", "/")
            local hadDotSlash = string.sub(norm, 1, 2) == "./"
            while string.sub(norm, 1, 2) == "./" do
                norm = string.sub(norm, 3)
            end
            local unsafe = norm == ""
                or string.match(raw, "^[/\\][/\\]") ~= nil
                or string.match(raw, "^[/\\]%?%?[/\\]") ~= nil
                or string.match(norm, "^//") ~= nil
            if not unsafe then
                -- Only allow `:` if the original `raw` (without `./` prefix) is a genuine
                -- Windows drive path inside `dir`.
                local stripped = norm
                if not hadDotSlash and string.match(norm, "^[A-Za-z]:/") ~= nil then
                    stripped = string.sub(norm, 4)
                end
                if string.find(stripped, ":", 1, true) ~= nil then
                    unsafe = true
                else
                    for part in string.gmatch(stripped, "[^/]+") do
                        if part == ".." or string.match(part, "[%. ]$") ~= nil then
                            unsafe = true
                            break
                        end
                    end
                end
            end
            local path = if unsafe then nil else Workspace:Relative(raw, dir)
            if path == nil then
                error(`Path '{raw}' is outside the session folder '{dir}'`, 0)
            end
            local first: number? = if type(args.startLine) == "number" then math.max(1, math.floor(args.startLine)) else nil
            local last: number? = first
            if first and type(args.endLine) == "number" then
                last = math.max(first, math.floor(args.endLine))
            end
            Workspace:OpenFile(context.sessionId, path, { preview = args.preview == true, startLine = first, endLine = last })
            return { path = path, lines = if first and last then { first, last } else nil }
        end,
    })

    -- The + menu's and New tab's Open file row (order 30): offered for every session, with or
    -- without a project (`packages/gui-plugins/src/file/renderer.tsx:513-527`).
    ctx:AddMenuItem({
        menu = "session.panel",
        id = "open",
        title = I18n:T("command.open"),
        icon = "file-tree",
        keybind = "file.browse",
        order = 30,
        when = function(sid: string): boolean
            return Sessions:Get(sid) ~= nil
        end,
        run = function(sid: string?)
            browse(sid)
        end,
    })
end)
