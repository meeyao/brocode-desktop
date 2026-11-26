--!strict
-- The Context tab (tab.tsx): the session's figures in a two-column grid (one column under 32rem)
-- and its raw messages, each opening to its JSON, with Export session. The messages are those
-- Desktop's timeline would hold (the host's `windowStart` on), read off the server again whenever
-- the session changes.

local ui = require("@opencode/ui")
local json = require("./json")

local tab = {}

-- Export session: idle, or reading and saving one session's messages.
export type Export = { kind: "idle" } | { kind: "exporting", sessionId: string }

-- One raw message: its text as the server wrote it and what the tab reads off it.
type Raw = { text: string, id: string, kind: string, created: number? }

export type State = {
    -- Raw message rows shown open, by message id.
    open: { [string]: boolean },
    -- Moves each time a row opens or shuts, for the tab's memo version.
    toggles: number,
    export: Export,
    -- Each session's messages Desktop holds, as last read, split once per answer and window.
    parsed: { [string]: { key: string, raws: { Raw } } },
    -- Each page of messages read, by its path: split once per answer.
    pages: { [string]: { generation: number, raws: { Raw }, next: string? } },
}

export type Env = {
    Sessions: SessionsService,
    Models: ModelsService,
    Server: ServerService,
    I18n: I18nService,
    Theme: ThemeService,
    Toast: ToastService,
    Workspace: WorkspaceService,
    refresh: () -> (),
    state: State,
}

local function messagesPath(session: string): string
    return `/api/session/{session}/message?limit=200&order=asc`
end

-- The session's newest 80 messages, newest first: Desktop's first read (40, then up to three
-- pages of 20 for a cut turn) and a little room, without the whole of a long session's newest
-- 200 (2.6 MB on a long real one).
local function latestPath(session: string): string
    return `/api/session/{session}/message?limit=80&order=desc`
end

-- Props every use shares, frozen: the host decodes a frozen table once and copies the node.
-- What Desktop's formatter writes for a figure it doesn't have (`format.ts`).
local DASH = "\u{2014}"
local STAT = table.freeze({ gap = 4, w = 0, grow = 1, minW = 0 } :: UiBoxProps)
-- `packages/gui-plugins/src/context/tab.tsx:34-35` (`4d4a95a3239c`), `utilities.css:21-37`,
-- `theme.css:8,12-13,16,19`: `.text-12-regular` & `.text-12-medium` are both `13px`, `line-height: 150%` (`19.5px`).
local STAT_NAME = table.freeze({ size = 13, weight = "regular" :: FontWeight, lineHeight = 19.5, color = "text-weak" } :: UiTextProps)
local STAT_VALUE = table.freeze({
    size = 13,
    weight = "medium" :: FontWeight,
    lineHeight = 19.5,
    color = "text-strong",
    selectable = true,
} :: UiTextProps)
local STAT_LINE = table.freeze({ w = "full", gap = 16 } :: UiBoxProps)
local STATS = table.freeze({ key = "context-stats", w = "full", gap = 16 } :: UiBoxProps)
local CURRENCY = table.freeze({ style = "currency" } :: NumberFormat)
local HEADING = table.freeze({ size = 13, weight = "regular" :: FontWeight, lineHeight = 19.5, color = "text-weak" } :: UiTextProps)
local CONTENT = table.freeze({ key = "session-usage-content", w = "full", pl = 24, pr = 32, pt = 16, pb = 40, gap = 40 } :: UiBoxProps)
local TAB = table.freeze({ key = "context-tab", w = "full", h = "full", pt = 8 } :: UiBoxProps)
local TAB_MOBILE = table.freeze({ key = "context-tab", w = "full", h = "full", pt = 0 } :: UiBoxProps)
local SCROLL = table.freeze({ key = "context-scroll", w = "full", grow = 1, minH = 0, scrollY = true } :: UiBoxProps)
local EXPORT = table.freeze({
    key = "context-export",
    role = "button",
    h = 24,
    px = 8,
    gap = 6,
    align = "center",
    radius = 4,
    hover = "surface-hover",
    press = "surface-press",
    hoverTransition = "instant",
    hoverGroup = true,
} :: UiBoxProps)
local EXPORT_ICON = table.freeze({ size = 16, color = "text-weak", hoverGroupColor = "text-chrome" } :: UiIconProps)
local EXPORT_TEXT = table.freeze({
    size = 13,
    weight = "medium" :: FontWeight,
    lineHeight = 20,
    tracking = -0.04,
    tabular = true,
    color = "text-weak",
    hoverGroupColor = "text-chrome",
} :: UiTextProps)
local RAW_SECTION = table.freeze({ key = "context-raw-section", w = "full", gap = 8 } :: UiBoxProps)
local RAW_HEAD = table.freeze({ w = "full", align = "center", justify = "between" } :: UiBoxProps)
-- A message's row: its kind and id at the start (the id cut to fit), its time and the chevron
-- at the end. One row of four: the id grows into the space between, 16 short of the time, as
-- the kind and id's group, a spacer and the time's group laid out 8 apart.
local RAW_ROW = table.freeze({
    role = "button",
    w = "full",
    h = 32,
    -- `accordion-trigger` (`accordion.css:35-46`): 1 pt border + 12 pt inline padding = 13 pt inset.
    px = 13,
    gap = 8,
    align = "center",
    border = "border-base",
    fill = "bg-base",
    hover = "surface-hover",
    hoverTransition = "instant",
} :: UiBoxProps)
-- By whether the row starts the list, then whether it ends it shut: its corners.
local RAW_CORNERS = table.freeze({
    [0] = table.freeze({ [0] = table.freeze({ 0, 0, 0, 0 }), [1] = table.freeze({ 0, 0, 8, 8 }) }),
    [1] = table.freeze({ [0] = table.freeze({ 8, 8, 0, 0 }), [1] = table.freeze({ 8, 8, 8, 8 }) }),
})
local RAW_BODY = table.freeze({
    w = "full",
    pad = 12,
    corners = { 0, 0, 0, 0 },
    border = "border-base",
    mt = -1,
    fill = "bg-base",
} :: UiBoxProps)
local RAW_BODY_LAST = table.freeze({
    w = "full",
    pad = 12,
    corners = { 0, 0, 8, 8 },
    border = "border-base",
    mt = -1,
    fill = "bg-base",
} :: UiBoxProps)
local RAW_WRAP_FIRST = table.freeze({ w = "full" } :: UiBoxProps)
local RAW_WRAP = table.freeze({ w = "full" } :: UiBoxProps)
local RAW_KIND = table.freeze({
    size = 13,
    weight = "regular" :: FontWeight,
    lineHeight = 19.5,
    color = "text-chrome",
    noWrap = true,
} :: UiTextProps)
local RAW_ID = table.freeze({
    size = 13,
    weight = "regular" :: FontWeight,
    lineHeight = 19.5,
    color = "text-base",
    truncate = true,
    shrink = true,
    minW = 0,
    grow = 1,
    ml = -8,
} :: UiTextProps)
local RAW_TIME = table.freeze({
    size = 13,
    weight = "regular" :: FontWeight,
    lineHeight = 19.5,
    color = "text-weak",
    noWrap = true,
    ml = 8,
} :: UiTextProps)
local RAW_CHEVRON = table.freeze({ size = 16, color = "text-weak", ml = 4 } :: UiIconProps)
local RAW_JSON = table.freeze({
    size = 12,
    weight = "regular" :: FontWeight,
    lineHeight = 18,
    mono = true,
    wrap = true,
    selectable = true,
    color = "text-chrome",
} :: UiTextProps)

-- `raws`: the messages Desktop holds, nil until they're read; the counts wait for them, the step's
-- figures (the host's, off the same messages) don't.
local function stats(env: Env, session: Session?, sid: string, raws: { Raw }?, wide: boolean): ui.Node
    local I18n = env.I18n
    -- The step whose tokens fill the context window, as the header reads it (`lastAssistantWithUsage`
    -- on the messages the app holds, after the last finished compaction and before a staged revert).
    local tokens: { input: number, output: number, reasoning: number, read: number, write: number }? = nil
    local provider: string?, model: string? = nil, nil
    if session and session.usage then
        local u = session.usage
        tokens =
            { input = u.input, output = u.output, reasoning = u.reasoning, read = u.cacheRead, write = u.cacheWrite }
        provider, model = session.usageProviderId, session.usageModelId
    end
    local catalog = if provider and model then env.Models:Get(provider, model) else nil
    local total = if tokens then tokens.input + tokens.output + tokens.reasoning + tokens.read + tokens.write else nil
    local limit = if catalog then catalog.contextLimit else nil
    local usage = if total and limit and limit > 0 then math.round(total / limit * 100) else nil
    local function num(v: number?): string
        return if v then I18n:Number(v) else DASH
    end
    local function count(kind: string?): string
        if raws == nil then
            return DASH
        end
        local n = 0
        for _, m in raws do
            if kind == nil or m.kind == kind then
                n += 1
            end
        end
        return I18n:Number(n)
    end
    local function time(ms: number?): string
        return if ms and ms > 0 then I18n:Date(ms, "dateTime") else DASH
    end
    local providerName = if provider then (if catalog then catalog.providerName else nil) or provider else DASH
    local modelName = if model then (if catalog then catalog.modelName else nil) or model else DASH
    local title = if session then session.title elseif sid == "" then DASH else sid
    local function stat(name: string, value: string): ui.Node
        return ui.col(STAT, { ui.text(I18n:T(name), STAT_NAME), ui.text(value, STAT_VALUE) })
    end
    -- `tab.tsx`: omit Total Cost when no session is selected, or when spend is zero on Brocode Go.
    local costProvider: string? = if session and session.modelId and session.modelId ~= ""
        then session.providerId
        else session and session.usageProviderId
    local costValue: string? = nil
    if sid ~= "" then
        local spent = if session then session.cost else 0
        if spent == 0 and costProvider == nil and raws then
            for i = #raws, 1, -1 do
                if raws[i].kind == "assistant" then
                    local modelJson = json.member(raws[i].text, "model")
                    if modelJson then
                        costProvider = json.string(json.member(modelJson, "providerID"))
                    end
                    break
                end
            end
        end
        if not (spent == 0 and costProvider == "opencode-go") then
            costValue = I18n:Number(spent, CURRENCY)
        end
    end
    local cells: { ui.Node } = {
        stat("stats.session", title),
        stat("stats.messages", count(nil)),
        stat("stats.provider", providerName),
        stat("stats.model", modelName),
        stat("stats.limit", num(limit)),
        stat("stats.totalTokens", num(total)),
        stat("stats.usage", if usage then `{I18n:Number(usage)}%` else DASH),
        stat("stats.inputTokens", num(tokens and tokens.input)),
        stat("stats.outputTokens", num(tokens and tokens.output)),
        stat("stats.reasoningTokens", num(tokens and tokens.reasoning)),
        ui.col(STAT, {
            ui.text(I18n:T("stats.cacheTokens"), STAT_NAME),
            ui.text("{1} / {2}", STAT_VALUE, num(tokens and tokens.read), num(tokens and tokens.write)),
        }),
        stat("stats.userMessages", count("user")),
        stat("stats.assistantMessages", count("assistant")),
    }
    if costValue then
        table.insert(cells, stat("stats.totalCost", costValue))
    end
    table.insert(cells, stat("stats.sessionCreated", time(session and session.created)))
    table.insert(cells, stat("stats.lastActivity", time(session and session.usageCreated)))
    local lines: { ui.Node } = table.create(if wide then math.ceil(#cells / 2) else #cells)
    if wide then
        for i = 1, #cells, 2 do
            table.insert(lines, ui.row(STAT_LINE, { cells[i], cells[i + 1] or ui.col(STAT, {}) }))
        end
    else
        for _, cell in cells do
            table.insert(lines, ui.row(STAT_LINE, { cell }))
        end
    end
    return ui.col(STATS, lines)
end

-- `sessionExportFilename`: the title (or slug, or id) lowercased, runs of anything but letters,
-- digits, `_` and `-` as one `-`, at most 250, trimmed of `-`, plus `.json`.
local function filename(info: any, id: string): string
    local name = id
    for _, k in { "title", "slug" } do
        local v = if type(info) == "table" then info[k] else nil
        if type(v) == "string" and v ~= "" then
            name = v
            break
        end
    end
    local clean = string.lower(name):gsub("[^%w_%-]+", "-"):gsub("%-%-+", "-")
    clean = string.sub(clean, 1, 250):gsub("^%-+", ""):gsub("%-+$", "")
    return `{if clean == "" then id else clean}.json`
end

local function startExport(env: Env, sid: string)
    env.state.export = { kind = "exporting", sessionId = sid }
    env.refresh()
    task.spawn(function()
        local I18n = env.I18n
        local function failed(err: string?)
            env.state.export = { kind = "idle" }
            env.refresh()
            env.Toast:Error(
                I18n:T("export.failed.title"),
                if err and err ~= "" then err else I18n:T("export.failed.description")
            )
        end
        local res = env.Server:RequestAsync("GET", `/api/session/{sid}`)
        if not res.ok then
            return failed(res.error)
        end
        local info = json.member(res.text, "data") or res.text
        local messages: { string } = {}
        local cursor: string? = nil
        repeat
            -- A cursor carries its order; the server refuses both together.
            local path = if cursor
                then `/api/session/{sid}/message?limit=200&cursor={env.Server:EncodeUri(cursor)}`
                else messagesPath(sid)
            local page = env.Server:RequestAsync("GET", path)
            if not page.ok then
                return failed(page.error)
            end
            for _, item in json.items(json.member(page.text, "data") or "[]") do
                table.insert(messages, item)
            end
            local next = if type(page.json) == "table" and type(page.json.cursor) == "table"
                then page.json.cursor.next
                else nil
            cursor = if type(next) == "string" then next else nil
        until cursor == nil
        local raw = `\{"info":{if info == "" then "{}" else info},"messages":[{table.concat(messages, ",")}]\}`
        local name = filename(if type(res.json) == "table" then res.json.data or res.json else nil, sid)
        local saved = env.Workspace:SaveDownloadAsync(name, json.pretty(raw))
        env.state.export = { kind = "idle" }
        env.refresh()
        if saved.error then
            env.Toast:Error(I18n:T("export.failed.title"), saved.error)
        else
            env.Toast:Info(I18n:T("export.success.title"), I18n:T("export.success.description", { filename = name }))
        end
    end)
end

local function rawMessages(
    env: Env,
    view: View,
    sid: string,
    raws: { Raw },
    generation: string,
    exporting: boolean,
    look: string
): ui.Node
    local disabled = exporting or sid == ""
    local words = env.I18n:T("export.session")
    local export = ui.row(EXPORT, {
        label = words,
        opacity = if disabled then 0.5 else 1,
        onPress = if disabled
            then nil
            else view:Handler(function()
                startExport(env, sid)
            end),
    }, {
        ui.icon("download", EXPORT_ICON),
        ui.text(words, EXPORT_TEXT),
    })
    local head = ui.row(RAW_HEAD, {
        ui.text(env.I18n:T("rawMessages.title"), HEADING),
        export,
    })
    local open = env.state.open
    -- Each row's own version: whether it shows its JSON. The list's moves with the messages and
    -- the look, so opening one row rebuilds that row alone.
    local opened: { boolean } = table.create(#raws)
    for i, m in raws do
        opened[i] = open[m.id] == true
    end
    local function row(i: number): ui.Node
        local m = raws[i]
        local id, kind, isOpen = m.id, m.kind, opened[i]
        local last = i == #raws
        local created = m.created
        local header = ui.row(RAW_ROW, {
            key = `raw-{id}`,
            label = `{kind} {id}`,
            corners = RAW_CORNERS[if i == 1 then 1 else 0][if last and not isOpen then 1 else 0],
            onPress = view:Handler(function()
                open[id] = (if open[id] then nil else true) :: any
                env.state.toggles += 1
                env.refresh()
            end),
        }, {
            ui.text("{1} ", RAW_KIND, kind),
            ui.text("\u{2022} {1}", RAW_ID, id),
            ui.text(if type(created) == "number" then env.I18n:Date(created, "dateTime") else "-", RAW_TIME),
            ui.icon("chevron-grabber-vertical", RAW_CHEVRON),
        })
        local body = if isOpen
            then ui.col(
                if last then RAW_BODY_LAST else RAW_BODY,
                { key = `raw-body-{id}` },
                { ui.text(json.pretty(m.text), RAW_JSON) }
            )
            else false
        -- The list keys a row's root, so the row's own key sits a level down.
        return ui.col(if i > 1 then RAW_WRAP else RAW_WRAP_FIRST, { header, body } :: { any })
    end
    return ui.col(RAW_SECTION, {
        head,
        -- Only the rows the scroll shows are built, as they come into view.
        ui.virtualList({
            key = "context-raw",
            scrollY = false,
            gap = -1,
            w = "full",
            count = #raws,
            estimatedRowHeight = 32,
            version = `{generation}:{look}`,
            rowVersions = opened,
            renderRow = row,
        }),
    })
end

-- One page of a session's messages, newest first, and the cursor past it; split once per answer.
-- Nil while it loads.
local function page(env: Env, path: string, version: number): ({ Raw }?, string?, number)
    -- The host splits the page off the UI thread (`items`).
    local res = env.Server:Resource(path, version, true)
    if res.status ~= "ready" then
        return nil, nil, res.generation
    end
    local cached = env.state.pages[path]
    if cached and cached.generation == res.generation then
        return cached.raws, cached.next, res.generation
    end
    local items: { ResourceItem } = res.items or {}
    local raws: { Raw } = table.create(#items)
    for _, item in items do
        table.insert(raws, { text = item.text, id = item.id, kind = item.kind, created = item.created })
    end
    local data = if type(res.json) == "table" then res.json else {}
    local next = if type(data.cursor) == "table" and type(data.cursor.next) == "string" then data.cursor.next else nil
    env.state.pages[path] = { generation = res.generation, raws = raws, next = next }
    return raws, next, res.generation
end

-- Desktop's rows for the inbox's undelivered prompts and synthetic inputs, after `shown`: each
-- `{ id, type, ...payload, time: { created } }` (`data.ts` `materializeInboxMessage`), unless the
-- transcript already holds it.
local function pending(inbox: Resource, shown: { Raw })
    if inbox.status ~= "ready" then
        return
    end
    local have: { [string]: boolean } = {}
    for _, m in shown do
        have[m.id] = true
    end
    local listed: { ResourceItem } = inbox.items or {}
    for _, item in listed do
        local kind: string, id: string = item.kind, item.id
        if (kind == "user" or kind == "synthetic") and not have[id] then
            local payload = json.member(item.text, "payload") or "{}"
            local inner = payload:match("^%s*{(.*)}%s*$") or ""
            local members = if inner:match("%S") then `{inner},` else ""
            local raw = `\{"id":{json.member(item.text, "id") or "\"\""},"type":"{kind}",{members}"time":\{"created":{item.created or 0}\}\}`
            local row: Raw = { text = raw, id = id, kind = kind, created = item.created }
            table.insert(shown, row)
        end
    end
end

-- Session `sid`'s messages as Desktop holds them (`data.session.message.list`), oldest first: from
-- the host's `windowStart` (the window's arithmetic is the host's, `context_window.rs`) to the
-- newest, read newest first in pages (80, then 100 at a time) until the start shows. Nil until they're known; what showed
-- stays while a page loads.
local function held(env: Env, sid: string, session: Session?): ({ Raw }?, string)
    local start = if session then session.windowStart else nil
    if start == "" then
        return nil, "unknown"
    end
    -- The newest page is read again only as new messages come (`messagesVersion`); a window
    -- that grows as the reader scrolls back reads further pages, each once.
    local path = latestPath(sid)
    local version = if session then session.messagesVersion else 0
    -- Desktop lists each undelivered prompt and synthetic input after the messages (`data.ts`
    -- `materializeInboxMessage`).
    -- Asked for with the newest page, so both reads go at once.
    local inbox = env.Server:Resource(`/api/session/{sid}/inbox`, version, true)
    page(env, path, version)
    if inbox.status == "loading" then
        local cached = env.state.parsed[sid]
        return if cached then cached.raws else nil, if cached then cached.key else "loading"
    end
    local key = `{start}:{inbox.generation}`
    local newest: { Raw } = {}
    while true do
        local raws, next, generation = page(env, path, version)
        if raws == nil then
            local cached = env.state.parsed[sid]
            return if cached then cached.raws else nil, if cached then cached.key else key
        end
        key ..= `:{generation}`
        local cached = env.state.parsed[sid]
        if cached and cached.key == key then
            return cached.raws, key
        end
        local found = false
        for _, m in raws do
            table.insert(newest, m)
            if m.id == start then
                found = true
                break
            end
        end
        if found or start == nil or next == nil then
            local shown: { Raw } = table.create(#newest)
            for i = #newest, 1, -1 do
                table.insert(shown, newest[i])
            end
            pending(inbox, shown)
            env.state.parsed[sid] = { key = key, raws = shown }
            return shown, key
        end
        -- Older pages hardly change; the newest page's read carries the session's version.
        path = `/api/session/{sid}/message?limit=100&cursor={env.Server:EncodeUri(next :: string)}`
        version = 0
    end
end

function tab.panel(env: Env, view: View, props: PanelProps): ui.Node?
    local sid = props.sessionId
    local session = env.Sessions:Get(sid)
    local raws, generation = held(env, sid, session)
    local wide = props.width >= 512
    local export = env.state.export
    local exporting = export.kind == "exporting" and export.sessionId == sid
    local look = `{env.Theme:Current().id}:{env.Theme:IsDark()}:{env.I18n:Locale().id}`
    local version =
        `{session and session.version}:{generation}:{env.state.toggles}:{exporting}:{wide}:{math.round(props.width)}:{props.placement}:{look}`
    return view:Memo(`panel-{sid}`, version, function()
        -- The figures move only with the session and its messages: a rebuild for a raw message
        -- opening or an export reuses them.
        local figures = `{session and session.version}:{generation}:{wide}:{look}`
        local content = ui.col(CONTENT, {
            view:Memo(`stats-{sid}`, figures, function()
                return stats(env, session, sid, raws, wide)
            end),
            rawMessages(env, view, sid, raws or {}, generation, exporting, look),
        })
        -- The side region's tab sits 8 under the strip; a narrow window's view has no strip.
        return ui.col(if props.placement == "mobile" then TAB_MOBILE else TAB, {
            ui.col(SCROLL, { content }),
        })
    end)
end

return tab
