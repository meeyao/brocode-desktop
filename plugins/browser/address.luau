--!strict
-- Browser address bar omnibox state machine matching upstream `UP-106` (`#65073`,
-- `packages/gui-plugins/src/browser/address-machine.ts` and `address.ts`).
--
-- Explicit state machine:
-- - `Edit`: `"rest"` | `"editing"` (with explicit `List`: `"following"` | `"held"` | `"closed"`).
-- - Chrome omnibox row order: first row is what Enter opens (`page` when prefix-completed,
--   `search` for search queries, or `address` for URLs), followed by search (if not first) and
--   ranked history pages from `history.sqlite` merged with the window's recent visits.
-- - Arrow/Tab cycling holds rows and writes each row's text into the editor (`set-text` effect).
-- - Escape unwinds one layer at a time: held row -> following -> closed list -> restore URL -> blur.
-- - Shift+Delete (or clicking a row's forget button) removes a history page from SQLite + recent.

local history = require("./history")

local address = {}

address.SUGGESTIONS = 5
address.HISTORY_ANSWER = 8
address.HINT_MAX_LENGTH = 48
address.NEW_TAB_ID = "__new_tab__"
address.BLANK_EPOCH = table.freeze({ url = "", loading = false, settled = 0, followed = 0, rejected = 0, moved = 0 })

export type Epoch = {
    url: string,
    loading: boolean,
    settled: number,
    followed: number,
    rejected: number,
    moved: number,
}

export type Hold = { text: string, followed: number, rejected: number, moved: number }

export type Row =
    { type: "search", text: string, url: string }
    | { type: "address", text: string, url: string }
    | { type: "page", page: history.Suggestion, url: string }

export type List =
    { type: "following" }
    | { type: "held", rows: { Row }, line: number }
    | { type: "closed" }

export type Edit = { status: "rest" } | {
    status: "editing",
    tab: string,
    text: string,
    typed: boolean,
    list: List,
    answer: { history.HistoryPage },
    completing: boolean,
}

export type State = { asked: number, edit: Edit, holds: { [string]: Hold } }

export type Event =
    { type: "focused", tab: string, epoch: Epoch }
    | { type: "blurred" }
    | { type: "input", value: string, deleting: boolean, visits: { history.Visit } }
    | { type: "answered", attempt: number, pages: { history.HistoryPage }, visits: { history.Visit } }
    | { type: "key", key: string, epoch: Epoch, visits: { history.Visit } }
    | { type: "forget", url: string?, visits: { history.Visit } }
    | { type: "submitted", tab: string, epoch: Epoch, visits: { history.Visit } }
    | { type: "navigated", tab: string, url: string, epoch: Epoch }
    | { type: "page.followed", tab: string, epoch: Epoch }

export type Effect =
    { type: "select" }
    | { type: "select-completion", start: number }
    | { type: "blur" }
    | { type: "prepare" }
    | { type: "set-text", text: string }
    | { type: "ask", attempt: number, query: string }
    | { type: "forget", url: string }
    | { type: "navigate", tab: string, url: string, typed: boolean }

function address.initial(): State
    return { asked = 0, edit = { status = "rest" }, holds = {} }
end

local SELECT: Effect = { type = "select" }
local BLUR: Effect = { type = "blur" }
local PREPARE: Effect = { type = "prepare" }
local FOLLOWING: List = { type = "following" }
local CLOSED: List = { type = "closed" }

function address.parts(url: string): (string, string, string)
    local scheme = string.match(url, "^[%a][%w+.-]*://") or ""
    local rest = string.sub(url, #scheme + 1)
    local host = string.match(rest, "^[^/?#]*") or rest
    return scheme, host, string.sub(rest, #host + 1)
end

function address.loopback(host: string): boolean
    local h = string.lower(host)
    return h == "localhost" or h == "[::1]" or h == "::1" or string.match(h, "^127%.%d+%.%d+%.%d+$") ~= nil or string.sub(h, -10) == ".localhost"
end

function address.searches(input: string): boolean
    local value = string.gsub(input, "^%s*(.-)%s*$", "%1")
    local lower = string.lower(value)
    if value == "" or string.match(lower, "^about:%S*$") or string.match(lower, "^data:%S*$") or string.match(lower, "^javascript:%S*$") or string.match(lower, "^blob:%S*$") or string.match(lower, "^file:%S*$") then
        return false
    end
    if string.match(lower, "^file://") then
        return string.match(value, "^[%a][%w+.-]*://%S+") == nil
    end
    if string.match(value, "^[%a][%w+.-]*://") then
        return string.match(value, "^[%a][%w+.-]*://[^/?#%s]+") == nil
    end
    if string.find(value, "%s") then
        return true
    end
    local host = string.match(value, "^[^/?#]*") or ""
    if string.find(host, "@", 1, true) then
        return true
    end
    local named = string.match(string.lower(host), "^localhost:?%d*$") ~= nil or string.match(host, "^%[[%x:.]+%]:?%d*$") ~= nil or string.match(host, ":%d+$") ~= nil or string.match(host, "^[^.]+%.[^.]") ~= nil
    return not named
end

local function encodeQuery(text: string): string
    return (string.gsub(text, "([^%w%-_%.~])", function(c: string)
        return string.format("%%%02X", string.byte(c))
    end))
end

function address.searchUrl(text: string): string
    return `https://www.google.com/search?q={encodeQuery(text)}`
end

function address.resolveUrl(input: string): string
    local trimmed = string.gsub(input, "^%s*(.-)%s*$", "%1")
    if trimmed == "" then
        return "about:blank"
    end
    if string.match(trimmed, "^[%a][%w+.-]*:") ~= nil then
        return trimmed
    end
    if address.searches(trimmed) then
        return address.searchUrl(trimmed)
    end
    local host = string.match(trimmed, "^[^/?#]*") or ""
    local name = string.match(host, "^%[[^%]]*%]") or string.match(host, "^[^:]*") or host
    local scheme = if address.loopback(name) then "http://" else "https://"
    return `{scheme}{trimmed}`
end

function address.rowText(row: Row): string
    if row.type == "page" then
        return history.filled(row.url)
    end
    return row.text
end

function address.fold(previous: Epoch?, url: string, loading: boolean, rejected: number): Epoch
    if previous == nil then
        return { url = url, loading = loading, settled = 0, followed = 0, rejected = rejected, moved = 0 }
    end
    local settled = if previous.loading and not loading then previous.settled + 1 else previous.settled
    local followed = if url ~= previous.url or settled ~= previous.settled then previous.followed + 1 else previous.followed
    local moved = if url ~= previous.url or loading ~= previous.loading or rejected ~= previous.rejected then previous.moved + 1 else previous.moved
    return { url = url, loading = loading, settled = settled, followed = followed, rejected = rejected, moved = moved }
end

local function activeHold(state: State, tab: string, epoch: Epoch): Hold?
    local hold = state.holds[tab]
    if hold and hold.followed == epoch.followed and hold.rejected == epoch.rejected then
        return hold
    end
    return nil
end

-- Chrome omnibox row ordering (`address-machine.ts::listRows`).
local function listRows(text: string, pages: { history.Suggestion }, completing: boolean): { Row }
    local typed = string.gsub(text, "^%s*(.-)%s*$", "%1")
    if typed == "" then
        return {}
    end
    local searchRow: Row = { type = "search", text = typed, url = address.searchUrl(typed) }
    local pageRows: { Row } = {}
    for _, p in pages do
        table.insert(pageRows, { type = "page", page = p, url = p.url })
    end
    local lead = pages[1]
    if lead ~= nil and (lead.completion == "" or (completing and lead.completion ~= nil)) then
        local out: { Row } = { pageRows[1], searchRow }
        for i = 2, #pageRows do
            table.insert(out, pageRows[i])
        end
        return out
    end
    if address.searches(typed) then
        local out: { Row } = { searchRow }
        for _, r in pageRows do
            table.insert(out, r)
        end
        return out
    end
    local out: { Row } = { { type = "address", text = typed, url = address.resolveUrl(typed) }, searchRow }
    for _, r in pageRows do
        table.insert(out, r)
    end
    return out
end

local function computeRows(edit: Edit, visits: { history.Visit }): { Row }
    if edit.status ~= "editing" or not edit.typed then
        return {}
    end
    if edit.list.type == "held" then
        return edit.list.rows
    end
    return listRows(edit.text, history.suggestMerged(edit.text, visits, edit.answer, address.SUGGESTIONS), edit.completing)
end

local function isListed(edit: Edit, rows: { Row }): boolean
    return edit.status == "editing" and edit.typed and edit.list.type ~= "closed" and #rows > 0
end

local function chosenRow(edit: Edit, rows: { Row }): (Row?, number)
    if edit.status ~= "editing" or not isListed(edit, rows) then
        return nil, 0
    end
    local line = if edit.list.type == "held" then edit.list.line else 1
    return rows[line], line
end

local function completionSuffix(edit: Edit, rows: { Row }): string
    if edit.status ~= "editing" or not edit.typed or edit.list.type ~= "following" or not edit.completing then
        return ""
    end
    local first = rows[1]
    if first ~= nil and first.type == "page" and first.page.completion ~= nil then
        return first.page.completion
    end
    return ""
end

local function selecting(edit: Edit, visits: { history.Visit }): { Effect }
    if edit.status == "editing" and completionSuffix(edit, computeRows(edit, visits)) ~= "" then
        return { { type = "select-completion", start = #edit.text } }
    end
    return {}
end

local function typedText(state: State, tab: string, epoch: Epoch): string
    local edit = state.edit
    if edit.status == "editing" and edit.tab == tab then
        return edit.text
    end
    local hold = activeHold(state, tab, epoch)
    return if hold then hold.text else epoch.url
end

local function navigate(state: State, tab: string, url: string, epoch: Epoch, typed: boolean): (State, { Effect })
    local holds = table.clone(state.holds)
    holds[tab] = {
        text = if url == "about:blank" then "" else url,
        followed = epoch.followed,
        rejected = epoch.rejected,
        moved = epoch.moved,
    }
    return { asked = state.asked, edit = { status = "rest" }, holds = holds }, {
        { type = "navigate", tab = tab, url = url, typed = typed },
        BLUR,
    }
end

function address.step(state: State, event: Event): (State, { Effect })
    local edit = state.edit
    if event.type == "focused" then
        if edit.status == "editing" and edit.tab == event.tab then
            return state, {}
        end
        local hold = activeHold(state, event.tab, event.epoch)
        local text = if hold then hold.text else event.epoch.url
        local asked = state.asked + 1
        return {
            asked = asked,
            edit = {
                status = "editing",
                tab = event.tab,
                text = text,
                typed = false,
                list = FOLLOWING,
                answer = {},
                completing = false,
            },
            holds = state.holds,
        }, { SELECT, PREPARE }
    elseif event.type == "blurred" then
        if edit.status ~= "editing" then
            return state, {}
        end
        return { asked = state.asked, edit = { status = "rest" }, holds = state.holds }, {}
    elseif event.type == "input" then
        if edit.status ~= "editing" then
            return state, {}
        end
        local asked = state.asked + 1
        local trimmed = string.gsub(event.value, "^%s*(.-)%s*$", "%1")
        local searching = trimmed ~= ""
        local nextEdit: Edit = {
            status = "editing",
            tab = edit.tab,
            text = event.value,
            typed = true,
            list = FOLLOWING,
            answer = if searching then edit.answer else {},
            completing = not event.deleting,
        }
        local effects: { Effect } = {}
        if searching then
            table.insert(effects, { type = "ask", attempt = asked, query = event.value })
        end
        for _, eff in selecting(nextEdit, event.visits) do
            table.insert(effects, eff)
        end
        return { asked = asked, edit = nextEdit, holds = state.holds }, effects
    elseif event.type == "answered" then
        if edit.status ~= "editing" or event.attempt ~= state.asked or edit.list.type == "held" then
            return state, {}
        end
        local nextEdit: Edit = {
            status = "editing",
            tab = edit.tab,
            text = edit.text,
            typed = edit.typed,
            list = edit.list,
            answer = event.pages,
            completing = edit.completing,
        }
        return { asked = state.asked, edit = nextEdit, holds = state.holds }, selecting(nextEdit, event.visits)
    elseif event.type == "forget" then
        if edit.status ~= "editing" then
            if event.url ~= nil and event.url ~= "" then
                return state, { { type = "forget", url = event.url } }
            end
            return state, {}
        end
        local rows = computeRows(edit, event.visits)
        local targetUrl = event.url
        local line = if edit.list.type == "held" then edit.list.line else 1
        if targetUrl == nil then
            local picked = if isListed(edit, rows) then rows[line] else nil
            if picked == nil or picked.type ~= "page" then
                return state, {}
            end
            targetUrl = picked.url
        end
        local forgetUrl = targetUrl :: string
        local forgetKey = history.pageKey(forgetUrl)
        local forgetHost = string.match(history.bare(forgetUrl), "^([^/?#]+)")
        local remainingPages: { history.Suggestion } = {}
        local otherOnHost = false
        for _, r in rows do
            if r.type == "page" then
                local rk = history.pageKey(r.url)
                if r.url ~= forgetUrl and (forgetKey == nil or rk ~= forgetKey) then
                    local rh = string.match(history.bare(r.url), "^([^/?#]+)")
                    if forgetHost ~= nil and rh == forgetHost and rk ~= string.lower(forgetHost) then
                        otherOnHost = true
                    end
                    table.insert(remainingPages, r.page)
                end
            end
        end
        if forgetHost ~= nil and not otherOnHost then
            local hostKey = string.lower(forgetHost)
            local pruned: { history.Suggestion } = {}
            for _, p in remainingPages do
                if history.pageKey(p.url) ~= hostKey or p.title ~= "" then
                    table.insert(pruned, p)
                end
            end
            remainingPages = pruned
        end
        local nextAnswer: { history.HistoryPage } = {}
        for _, p in edit.answer do
            local pk = history.pageKey(p.url)
            if p.url ~= forgetUrl and (forgetKey == nil or pk ~= forgetKey) then
                table.insert(nextAnswer, p)
            end
        end
        local left = listRows(edit.text, remainingPages, edit.completing)
        local nextLine = math.max(1, math.min(line, #left))
        local nextList: List = if #left > 0 then { type = "held", rows = left, line = nextLine } else CLOSED
        local nextEdit: Edit = {
            status = "editing",
            tab = edit.tab,
            text = edit.text,
            typed = edit.typed,
            list = nextList,
            answer = nextAnswer,
            completing = edit.completing,
        }
        return { asked = state.asked, edit = nextEdit, holds = state.holds }, { { type = "forget", url = forgetUrl } }
    elseif event.type == "key" then
        if edit.status ~= "editing" then
            return state, {}
        end
        local rows = computeRows(edit, event.visits)
        local open = isListed(edit, rows)
        if event.key == "Escape" then
            if edit.list.type == "held" then
                local nextEdit: Edit = {
                    status = "editing",
                    tab = edit.tab,
                    text = edit.text,
                    typed = edit.typed,
                    list = FOLLOWING,
                    answer = edit.answer,
                    completing = edit.completing,
                }
                return { asked = state.asked, edit = nextEdit, holds = state.holds }, selecting(nextEdit, event.visits)
            end
            if open then
                local nextEdit: Edit = {
                    status = "editing",
                    tab = edit.tab,
                    text = edit.text,
                    typed = edit.typed,
                    list = CLOSED,
                    answer = edit.answer,
                    completing = edit.completing,
                }
                return { asked = state.asked, edit = nextEdit, holds = state.holds }, {}
            end
            if not edit.typed then
                return { asked = state.asked, edit = { status = "rest" }, holds = state.holds }, { BLUR }
            end
            local hold = activeHold(state, edit.tab, event.epoch)
            local restored = if hold then hold.text else event.epoch.url
            local asked = state.asked + 1
            local nextEdit: Edit = {
                status = "editing",
                tab = edit.tab,
                text = restored,
                typed = false,
                list = FOLLOWING,
                answer = {},
                completing = false,
            }
            return { asked = asked, edit = nextEdit, holds = state.holds }, { { type = "set-text", text = restored }, SELECT }
        end
        if event.key == "Shift+Delete" or event.key == "Shift+Backspace" then
            return address.step(state, { type = "forget", url = nil, visits = event.visits })
        end
        if not open then
            if event.key == "Tab" or event.key == "Shift+Tab" or string.gsub(edit.text, "^%s*(.-)%s*$", "%1") == "" then
                return state, {}
            end
            local asked = state.asked + 1
            local nextEdit: Edit = {
                status = "editing",
                tab = edit.tab,
                text = edit.text,
                typed = true,
                list = FOLLOWING,
                answer = edit.answer,
                completing = edit.typed and edit.completing,
            }
            local effects: { Effect } = { { type = "ask", attempt = asked, query = edit.text } }
            for _, eff in selecting(nextEdit, event.visits) do
                table.insert(effects, eff)
            end
            return { asked = asked, edit = nextEdit, holds = state.holds }, effects
        end
        local line = if edit.list.type == "held" then edit.list.line else 1
        local by = if event.key == "ArrowDown" or event.key == "Tab" then 1 else -1
        local nextLine = ((line - 1 + by + #rows) % #rows) + 1
        local picked = rows[nextLine]
        local nextEdit: Edit = {
            status = "editing",
            tab = edit.tab,
            text = edit.text,
            typed = edit.typed,
            list = { type = "held", rows = rows, line = nextLine },
            answer = edit.answer,
            completing = edit.completing,
        }
        return { asked = state.asked, edit = nextEdit, holds = state.holds }, {
            { type = "set-text", text = address.rowText(picked) },
        }
    elseif event.type == "submitted" then
        local rows = computeRows(edit, event.visits)
        local picked = chosenRow(edit, rows)
        if picked ~= nil then
            return navigate(state, event.tab, picked.url, event.epoch, true)
        end
        local text = string.gsub(typedText(state, event.tab, event.epoch), "^%s*(.-)%s*$", "%1")
        if text ~= "" then
            return navigate(state, event.tab, text, event.epoch, true)
        end
        return { asked = state.asked, edit = { status = "rest" }, holds = state.holds }, { BLUR }
    elseif event.type == "navigated" then
        return navigate(state, event.tab, event.url, event.epoch, false)
    elseif event.type == "page.followed" then
        if edit.status ~= "editing" or edit.tab ~= event.tab or edit.typed or edit.list.type ~= "following" then
            return state, {}
        end
        if activeHold(state, event.tab, event.epoch) ~= nil or edit.text == event.epoch.url then
            return state, {}
        end
        local nextEdit: Edit = {
            status = "editing",
            tab = edit.tab,
            text = event.epoch.url,
            typed = false,
            list = edit.list,
            answer = edit.answer,
            completing = false,
        }
        return { asked = state.asked, edit = nextEdit, holds = state.holds }, { SELECT }
    else
        return state, {}
    end
end

export type View = {
    editing: boolean,
    field: string,
    query: string,
    typed: boolean,
    listed: boolean,
    rows: { Row },
    active: number,
    chosen: Row?,
    completion: string,
    completing: boolean,
    navigating: boolean,
}

function address.view(state: State, tab: string, epoch: Epoch, visits: { history.Visit }): View
    local edit = state.edit
    local editing = edit.status == "editing" and edit.tab == tab
    local rows = if editing then computeRows(edit, visits) else {}
    local shown = editing and isListed(edit, rows)
    -- Not `if shown then chosenRow(...) else nil, 0`: an if-expression keeps only the first result.
    local picked: Row?, active = nil, 0
    if shown then
        picked, active = chosenRow(edit, rows)
    end
    local completed = if editing then completionSuffix(edit, rows) else ""
    local fieldText: string
    if editing and edit.status == "editing" and edit.list.type == "held" and picked ~= nil then
        fieldText = address.rowText(picked)
    else
        fieldText = typedText(state, tab, epoch) .. completed
    end
    local hold = state.holds[tab]
    return {
        editing = editing,
        field = fieldText,
        query = if editing and edit.status == "editing" then edit.text else "",
        typed = editing and edit.status == "editing" and edit.typed,
        listed = shown,
        rows = rows,
        active = active,
        chosen = picked,
        completion = completed,
        completing = editing and edit.status == "editing" and edit.typed and edit.list.type == "following" and edit.completing,
        navigating = hold ~= nil and hold.moved == epoch.moved,
    }
end

return address
