--!strict
-- Desktop's built-in browser plugin (`packages/gui-plugins/src/browser`): one side-region tab per
-- page of the session (`browser:<page id>`), titled by its page with activity dot; the pane with
-- its reorganized toolbar, left-elided punycode address bar, two-view Site info popover, '...'
-- options popover + More tools submenu, Find in page bar, Refusal notices with 500 ms activation
-- delay, Device toolbar, plain-language error page, new tab page, and MCP tools.

local plugin = require("@opencode/plugin")
local ui = require("@opencode/ui")
local address = require("@self/address")
local createDevice = require("@self/device")
local extensions = require("@self/extensions")
local failure = require("@self/failure")
local find = require("@self/find")
local history = require("@self/history")
local hostDisplay = require("@self/host_display")
local mcp = require("@self/mcp")
local options = require("@self/options")
local panel = require("@self/panel")
local refusal = require("@self/refusal")
local savedTabs = require("@self/saved_tabs")
local settings = require("@self/settings")
local signins = require("@self/signins")
local site = require("@self/site")

-- The panel's id: its tabs key as `browser:<page id>`, as Desktop's.
local PANEL = "main"
-- How long an agent-revealed tab holds clicks on its bars/notices (ms).
local REVEAL_GUARD_MS = 500

return plugin.defineWindow(function(ctx)
    local I18n = ctx:GetService("I18n")
    local Sessions = ctx:GetService("Sessions")
    local Browser = ctx:GetService("Browser")
    local Layout = ctx:GetService("Layout")
    local App = ctx:GetService("App")
    local Store = ctx:GetService("Store")
    local Contracts = ctx:GetService("Contracts")
    local System = ctx:GetService("System")
    local Toast = ctx:GetService("Toast")
    local Composer = ctx:GetService("Composer")
    local Server = ctx:GetService("Server")
    local Servers = ctx:GetService("Servers")
    local Popover = ctx:GetService("Popover")
    local Dialog = ctx:GetService("Dialog")
    local Menu = ctx:GetService("Menu")
    local Images = ctx:GetService("Images")
    local Workspace = ctx:GetService("Workspace")

    -- The window's address field, find field, and extension store link field.
    local editor = ctx:CreateEditor("")
    local findEditor = ctx:CreateEditor("")
    local extLinkEditor = ctx:CreateEditor("")
    local stored = Store:Value("history", { visits = {} })
    local prefsStore = Store:Value("preferences", { links = true, agentPages = true, advanced = false, welcomed = false })
    local sitePermsStore = Store:Value("rune.sitePermissions", {})
    local extensionsStore = Store:Value("extensions", {})
    local savedTabsStore = Store:Value("savedTabs", { sessions = {} })
    local recorder = Contracts:Use("browser.history")
    local extContract = Contracts:Use("browser.extensions")
    local profileContract = Contracts:Use("browser.profile")

    local state = address.initial()
    local epochs: { [string]: address.Epoch } = {}
    -- Each page's last recorded visit, so repeated reports don't rewrite history.
    local recorded: { [string]: string } = {}
    local recordedUrl: { [string]: string } = {}
    local typedNavs: { [string]: string } = {}
    local syncingEditor = false
    local adoptedHistory = false
    -- Per-tab Find state, refusal notice state, transient failure notices, activity dot, reveal guard.
    local findStates: { [string]: find.State } = {}
    local activeFindTab: string? = nil
    local refusalNotices: { [string]: refusal.Notice } = {}
    local dismissedRefusals: { [string]: number } = {}
    local failureNotices: { [string]: failure.Notice } = {}
    local nextNoticeId = 0
    local unseenTabs: { [string]: boolean } = {}
    local revealUntil: { [string]: number } = {}
    local panelHandle: Contribution? = nil
    local newTabBarHandle: Contribution? = nil
    local newTabRecentHandle: Contribution? = nil
    local newTabTarget: { sessionId: string?, pane: string? } = { sessionId = nil, pane = nil }
    local lastFocusToken: number? = nil

    local function refresh()
        if panelHandle then
            panelHandle:Refresh()
        end
        if newTabBarHandle then
            newTabBarHandle:Refresh()
        end
        if newTabRecentHandle then
            newTabRecentHandle:Refresh()
        end
    end

    local function visits(): { history.Visit }
        local value = stored:Get()
        return if type(value) == "table" and type(value.visits) == "table" then value.visits else {}
    end

    local device = if App.Channel == "local" or App.Channel == "dev" then createDevice(ctx, I18n, Popover, refresh) else nil

    local function pageUrl(tab: BrowserTab): string
        local url = tab:Url()
        return if url == "about:blank" then "" else url
    end

    local function isServerFile(url: string): boolean
        return string.sub(url, 1, 12) == "workspace://" or string.sub(url, 1, 11) == "artifact://" or string.sub(url, 1, 13) == "oc-preview://"
    end

    local function isPreviewUrl(url: string): boolean
        local lower = string.lower(url)
        return string.sub(lower, 1, 7) == "file://" or isServerFile(lower)
    end

    local function isPreviewTab(tab: BrowserTab): boolean
        local tAny = tab :: any
        if type(tAny.PreviewKind) == "function" and tAny:PreviewKind() ~= nil then
            return true
        end
        return isPreviewUrl(pageUrl(tab))
    end

    local function decodePercent(s: string): string
        return (string.gsub(s, "%%(%x%x)", function(hex)
            local byte = tonumber(hex, 16)
            return if byte and byte >= 32 then string.char(byte) else `%{hex}`
        end))
    end

    local function localFilePath(url: string): string?
        if string.sub(url, 1, 8) == "file:///" then
            local path = string.sub(url, 8)
            path = string.match(path, "^([^?#]*)") or path
            path = decodePercent(path)
            if App.Platform == "windows" and string.match(path, "^/[A-Za-z]:") ~= nil then
                path = string.sub(path, 2)
            end
            return path
        end
        return nil
    end

    -- Resolves workspace-relative source path for BOTH local (`file:///...`) and remote
    -- (`workspace://<session_id>/<rel>`) previews so "Show the file's code" works everywhere.
    local function workspaceSourceRel(url: string, session: Session?): string?
        if session == nil then
            return nil
        end
        local localPath = localFilePath(url)
        if localPath ~= nil then
            return Workspace:Relative(localPath, session.directory)
        end
        local prefix = `workspace://{string.lower(session.id)}/`
        if string.sub(string.lower(url), 1, #prefix) == prefix then
            local rest = string.sub(url, #prefix + 1)
            rest = string.match(rest, "^([^?#]*)") or rest
            if rest ~= "" then
                return decodePercent(rest)
            end
        end
        return nil
    end

    -- Engine capability probes (gracefully false on older engines without them).
    local function engineHas(flag: string): boolean
        local eng = Browser:Engine() :: any
        return eng ~= nil and eng[flag] == true
    end

    -- Page `tab`'s report counts, folded with what it reports now.
    local function epoch(tab: BrowserTab): address.Epoch
        local e = address.fold(epochs[tab.Id], pageUrl(tab), tab:Loading(), tab:Rejected())
        epochs[tab.Id] = e
        return e
    end

    local function showFailure(tabId: string, text: string, lasting: boolean, retry: (() -> ())?)
        nextNoticeId += 1
        local id = nextNoticeId
        failureNotices[tabId] = { id = id, text = text, lasting = lasting, retry = retry }
        if not lasting then
            task.delay(failure.NOTICE_MS / 1000, function()
                local cur = failureNotices[tabId]
                if cur and cur.id == id then
                    failureNotices[tabId] = nil
                    refresh()
                end
            end)
        end
        refresh()
    end

    local function ensureAdopted()
        if adoptedHistory then
            return
        end
        local bAny = Browser :: any
        if type(bAny.HistoryAdopt) == "function" then
            adoptedHistory = true
            bAny:HistoryAdopt(visits())
        end
    end

    local lastEditorField = ""

    local function setEditorText(text: string)
        lastEditorField = text
        syncingEditor = true
        editor:SetText(text)
        syncingEditor = false
    end

    local function editingNewTab(): boolean
        local e = state.edit
        return e.status == "editing" and e.tab == address.NEW_TAB_ID
    end

    local function editing(): BrowserTab?
        local e = state.edit
        return if e.status == "editing" and e.tab ~= address.NEW_TAB_ID then Browser:Get(e.tab) else nil
    end

    local function editingEpoch(): (string?, address.Epoch?)
        if editingNewTab() then
            return address.NEW_TAB_ID, address.BLANK_EPOCH
        end
        local tab = editing()
        if tab then
            return tab.Id, epoch(tab)
        end
        return nil, nil
    end

    local dispatch: (event: address.Event) -> ()

    local function run(effects: { address.Effect })
        local bAny = Browser :: any
        for _, effect in effects do
            if effect.type == "select" then
                editor:SelectAll()
            elseif effect.type == "select-completion" then
                local editId, editEp = editingEpoch()
                if editId and editEp and type(bAny.SetAddressSelection) == "function" then
                    local bar = address.view(state, editId, editEp, visits())
                    if bar.completion ~= "" then
                        syncingEditor = true
                        bAny:SetAddressSelection(#bar.field, #bar.field - #bar.completion)
                        syncingEditor = false
                    end
                end
            elseif effect.type == "blur" then
                editor:Blur()
            elseif effect.type == "prepare" then
                ensureAdopted()
                if type(bAny.HistoryPrepare) == "function" then
                    bAny:HistoryPrepare()
                end
            elseif effect.type == "set-text" then
                setEditorText(effect.text)
            elseif effect.type == "ask" then
                ensureAdopted()
                local pages: { history.HistoryPage } = {}
                if type(bAny.HistorySearch) == "function" then
                    local raw = bAny:HistorySearch(effect.query, address.HISTORY_ANSWER)
                    if type(raw) == "table" then
                        pages = raw :: { history.HistoryPage }
                    end
                end
                dispatch({ type = "answered", attempt = effect.attempt, pages = pages, visits = visits() })
            elseif effect.type == "forget" then
                if type(bAny.HistoryForget) == "function" then
                    bAny:HistoryForget(effect.url)
                end
                local keeper = recorder:Get()
                if keeper and type(keeper.forget) == "function" then
                    keeper.forget(effect.url)
                end
            elseif effect.type == "navigate" then
                if effect.tab == address.NEW_TAB_ID then
                    local targetSid = newTabTarget.sessionId
                    local targetPane = newTabTarget.pane
                    local navUrl = effect.url
                    local wasTyped = effect.typed
                    task.spawn(function()
                        local opened = Browser:OpenAsync({ url = navUrl, sessionId = targetSid, pane = targetPane })
                        if opened and wasTyped then
                            typedNavs[opened.Id] = navUrl
                        end
                    end)
                else
                    local tab = Browser:Get(effect.tab)
                    if tab then
                        if effect.typed then
                            typedNavs[effect.tab] = effect.url
                        end
                        tab:Navigate(effect.url)
                    end
                end
            end
        end
    end

    dispatch = function(event: address.Event)
        local nextState, effects = address.step(state, event)
        state = nextState
        local editId, editEp = editingEpoch()
        if editId and editEp then
            local bar = address.view(state, editId, editEp, visits())
            if editor:Text() ~= bar.field then
                setEditorText(bar.field)
            else
                lastEditorField = bar.field
            end
        end
        run(effects)
        refresh()
    end

    -- Puts the side panel New tab's address bar into editing mode.
    local function editNewTab(sid: string?, pane: string?)
        newTabTarget = { sessionId = sid, pane = pane }
        Browser:Prepare()
        local already = state.edit.status == "editing" and state.edit.tab == address.NEW_TAB_ID
        local nextState, effects = address.step(state, { type = "focused", tab = address.NEW_TAB_ID, epoch = address.BLANK_EPOCH })
        state = nextState
        local e = state.edit
        if not already and e.status == "editing" then
            setEditorText(e.text)
        end
        editor:Focus()
        if already then
            editor:SelectAll()
        end
        run(effects)
        refresh()
    end

    -- Puts page `tab`'s address in the field and the keyboard there, the whole address selected.
    local function edit(tab: BrowserTab)
        local already = state.edit.status == "editing" and state.edit.tab == tab.Id
        local nextState, effects = address.step(state, { type = "focused", tab = tab.Id, epoch = epoch(tab) })
        state = nextState
        local e = state.edit
        if not already and e.status == "editing" then
            setEditorText(e.text)
        end
        editor:Focus()
        if already then
            editor:SelectAll()
        end
        run(effects)
        refresh()
    end

    local editorFocused = false
    editor.FocusChanged:Connect(function(focused: boolean)
        editorFocused = focused
        if not focused then
            task.delay(0.12, function()
                if not editorFocused then
                    dispatch({ type = "blurred" })
                end
            end)
        end
    end)
    editor.Changed:Connect(function(value: string)
        if syncingEditor then
            return
        end
        local e = state.edit
        if e.status ~= "editing" or value == lastEditorField then
            return
        end
        local bAny = Browser :: any
        local sel = if type(bAny.AddressSelection) == "function" then bAny:AddressSelection() else nil
        local atEnd = sel == nil or (not sel.composing and sel.start == sel.len and sel["end"] == sel.len)
        local pasted = sel ~= nil and sel.pasted == true
        local prevTyped = if e.typed then e.text else ""
        local deletedCompletion = lastEditorField ~= e.text and value == e.text
        local prevLen = utf8.len(prevTyped) or #prevTyped
        local curLen = utf8.len(value) or #value
        local singleAppend = curLen == prevLen + 1 and string.sub(value, 1, #prevTyped) == prevTyped
        local deleting = deletedCompletion or pasted or not singleAppend or not atEnd
        lastEditorField = value
        dispatch({ type = "input", value = value, deleting = deleting, visits = visits() })
    end)
    local bSelSig = Browser :: any
    if bSelSig.AddressSelectionChanged ~= nil then
        bSelSig.AddressSelectionChanged:Connect(function(sel: any)
            if syncingEditor or type(sel) ~= "table" or sel.focused ~= true then
                return
            end
            local editId, editEp = editingEpoch()
            if not editId or not editEp then
                return
            end
            local bar = address.view(state, editId, editEp, visits())
            if not bar.completing then
                return
            end
            local value = editor:Text()
            if value ~= bar.field or sel.len ~= #value then
                return
            end
            if not sel.composing and sel.start == #bar.field - #bar.completion and sel["end"] == sel.len then
                return
            end
            dispatch({
                type = "input",
                value = value,
                deleting = sel.composing or not (sel.start == sel.len and sel["end"] == sel.len),
                visits = visits(),
            })
        end)
    end
    editor.Submitted:Connect(function()
        local editId, editEp = editingEpoch()
        if editId and editEp then
            dispatch({ type = "submitted", tab = editId, epoch = editEp, visits = visits() })
        end
    end)
    editor.KeyPressed:Connect(function(key: string)
        local editId, editEp = editingEpoch()
        if editId and editEp then
            dispatch({ type = "key", key = key, epoch = editEp, visits = visits() })
        end
    end)

    -- Find in page state management (`find.luau`, matching `page-find.ts`).
    local findSeq = 0
    local function getFind(tabId: string): find.State
        return findStates[tabId] or find.initial()
    end

    local function runFind(tab: BrowserTab, query: string, forward: boolean, findNext: boolean)
        local tAny = tab :: any
        if type(tAny.Find) == "function" then
            tAny:Find(query, forward, findNext)
        end
    end

    local function stopFind(tab: BrowserTab)
        local tAny = tab :: any
        if type(tAny.StopFinding) == "function" then
            tAny:StopFinding(true)
        end
    end

    local function openFind(tab: BrowserTab)
        local cur = getFind(tab.Id)
        local wasOpen = cur.status ~= "closed"
        local next = find.step(cur, { type = "open" })
        findStates[tab.Id] = next
        activeFindTab = tab.Id
        if findEditor:Text() ~= next.query then
            findEditor:SetText(next.query)
        end
        findEditor:Focus()
        findEditor:SelectAll()
        if not wasOpen and next.query ~= "" then
            findSeq += 1
            -- CEF's `Find` forwards `findNext` to Chromium's `FindTabHelper::StartFinding(..., find_match)`,
            -- so `true` selects match #1 on a fresh search and advances on repeats.
            runFind(tab, next.query, true, true)
        end
        refresh()
    end

    local function closeFind(tab: BrowserTab, restoreFocus: boolean?)
        findSeq += 1
        findStates[tab.Id] = find.step(getFind(tab.Id), { type = "close" })
        if activeFindTab == tab.Id then
            activeFindTab = nil
        end
        stopFind(tab)
        findEditor:Blur()
        if restoreFocus ~= false then
            local tAny = tab :: any
            if type(tAny.Focus) == "function" then
                tAny:Focus()
            end
        end
        refresh()
    end

    local function stepFind(tab: BrowserTab, forward: boolean)
        findSeq += 1
        local cur = getFind(tab.Id)
        if cur.query == "" then
            return
        end
        if cur.status == "closed" then
            local opened = find.step(cur, { type = "open" })
            findStates[tab.Id] = opened
            activeFindTab = tab.Id
            if findEditor:Text() ~= opened.query then
                findEditor:SetText(opened.query)
            end
            findEditor:Focus()
            findEditor:SelectAll()
            refresh()
        end
        runFind(tab, cur.query, forward, true)
    end

    findEditor.Changed:Connect(function(value: string)
        local tabId = activeFindTab
        if tabId == nil then
            return
        end
        local cur = getFind(tabId)
        if cur.status == "closed" or cur.query == value then
            return
        end
        findSeq += 1
        local seq = findSeq
        findStates[tabId] = find.step(cur, { type = "query", query = value })
        local tab = Browser:Get(tabId)
        if tab then
            if value == "" then
                stopFind(tab)
            else
                task.delay(0.12, function()
                    if findSeq ~= seq or activeFindTab ~= tabId then
                        return
                    end
                    local live = Browser:Get(tabId)
                    local st = getFind(tabId)
                    if live and st.status ~= "closed" and st.query == value then
                        runFind(live, value, true, true)
                    end
                end)
            end
        end
        refresh()
    end)
    findEditor.Submitted:Connect(function()
        local tabId = activeFindTab
        if tabId == nil then
            return
        end
        local tab = Browser:Get(tabId)
        if tab then
            stepFind(tab, true)
        end
    end)
    findEditor.KeyPressed:Connect(function(key: string)
        if activeFindTab == nil then
            return
        end
        local tab = Browser:Get(activeFindTab)
        if tab == nil then
            return
        end
        if key == "Escape" then
            closeFind(tab, true)
        elseif key == "Shift+Enter" then
            stepFind(tab, false)
        end
    end)

    -- Syncs a tab's policy refusal into a timestamped `refusal.Notice` for the 500 ms guard.
    local function syncRefusal(tab: BrowserTab, navigated: boolean)
        local code = tab:Refusal()
        local rej = tab:Rejected()
        local existing = refusalNotices[tab.Id]
        if code == nil or code == "browser.controlled.inherited_permission" then
            if existing ~= nil and (navigated or string.sub(existing.token, 1, 4) ~= "app:") then
                refusalNotices[tab.Id] = nil
            end
            return
        end
        if dismissedRefusals[tab.Id] == rej then
            return
        end
        local token = `{tab.Id}:{rej}:{code}`
        if existing and existing.token == token then
            return
        end
        refusalNotices[tab.Id] = {
            token = token,
            origin = "",
            text = I18n:T(failure.refusalKey(code)),
            target = nil,
            action = nil,
            shownAtMs = System:Now(),
        }
    end

    -- A page that moved: its counts, refusal notice, activity dot, history once loaded, and pane.
    Browser.Changed:Connect(function(id: string)
        local tab = Browser:Get(id)
        if tab == nil then
            epochs[id] = nil
            recorded[id] = nil
            recordedUrl[id] = nil
            typedNavs[id] = nil
            findStates[id] = nil
            refusalNotices[id] = nil
            dismissedRefusals[id] = nil
            failureNotices[id] = nil
            unseenTabs[id] = nil
            revealUntil[id] = nil
            if activeFindTab == id then
                activeFindTab = nil
            end
            refresh()
            return
        end
        local prev = epochs[id]
        local prevUrl = if prev then prev.url else ""
        local curEpoch = epoch(tab)
        local url = pageUrl(tab)
        if state.edit.status == "editing" and state.edit.tab == id and not state.edit.typed then
            dispatch({ type = "page.followed", tab = id, epoch = curEpoch })
        end
        local navigated = prev ~= nil and url ~= prevUrl
        syncRefusal(tab, navigated)
        if navigated and findStates[id] ~= nil and findStates[id].status ~= "closed" then
            closeFind(tab, false)
        end
        -- Mark unseen activity dot when an existing background tab navigates while another tab is selected.
        local selected = Browser:Selected(nil)
        if prev ~= nil and url ~= "" and url ~= prevUrl and (selected == nil or selected.Id ~= id) then
            unseenTabs[id] = true
        end
        local tAny = tab :: any
        local isPrivate = type(tAny.IsPrivate) == "function" and tAny:IsPrivate() == true
        local previewTab = isPreviewTab(tab)
        -- Private/isolated preview tabs and internal server files NEVER write to history.
        if not isPrivate and not previewTab and not tab:Loading() and history.recordable(url) and not isServerFile(url) then
            local title = tab:Title()
            local liveFav: string? = if type(tAny.Favicon) == "function" then tAny:Favicon() else nil
            local visit = `{url}\n{title}`
            local visitWithIcon = `{visit}\n{liveFav or ""}`
            local keeper = recorder:Get()
            local bAny = Browser :: any
            if recorded[id] ~= visit then
                local sameUrl = recordedUrl[id] == url
                recorded[id] = visit
                recordedUrl[id] = url
                if keeper then
                    keeper.remember(url, title, liveFav)
                end
                ensureAdopted()
                if sameUrl then
                    if type(bAny.HistoryRetitle) == "function" then
                        bAny:HistoryRetitle(url, title, id)
                    end
                else
                    local typedTarget = typedNavs[id]
                    typedNavs[id] = nil
                    -- Only credit double-frecency `typed` when the settled host matches what the
                    -- user typed (stripping `www.`), so redirects cannot steal typed frecency.
                    local wasTyped = false
                    if typedTarget ~= nil then
                        local wantHost = string.gsub(string.lower(hostDisplay.displayHost(typedTarget)), "^www%.", "")
                        local gotHost = string.gsub(string.lower(hostDisplay.displayHost(url)), "^www%.", "")
                        wasTyped = wantHost ~= "" and wantHost == gotHost
                    end
                    if type(bAny.HistoryVisit) == "function" then
                        bAny:HistoryVisit(url, title, wasTyped, id)
                    end
                end
            elseif liveFav ~= nil and liveFav ~= "" and recorded[`icon:{id}`] ~= visitWithIcon then
                recorded[`icon:{id}`] = visitWithIcon
                if keeper then
                    keeper.remember(url, title, liveFav)
                end
            end
        end
        -- Sync saved tabs for immediate restore & loading hold (`UP-100` `#65067`).
        -- Isolated tabs (`Preview`, `Artifact`, `Private`, `RemoteServer`) NEVER persist to disk.
        local routed = Sessions:Routed()
        if routed ~= nil then
            local curSaved = savedTabs.normalize(savedTabsStore:Get())
            local reported: { savedTabs.Reported } = {}
            for _, info in Browser:List(routed.id, {}) do
                if info.open and history.recordable(info.url) then
                    local live = Browser:Get(info.id)
                    local liveAny = live :: any
                    local skip = live ~= nil
                        and (isPreviewTab(live) or (type(liveAny.IsPrivate) == "function" and liveAny:IsPrivate() == true))
                    if not skip then
                        table.insert(reported, {
                            id = info.id,
                            url = info.url,
                            title = if live then live:Title() else info.title,
                            loading = if live then live:Loading() else false,
                        })
                    end
                end
            end
            local nextSaved = savedTabs.save(curSaved, routed.id, reported)
            if nextSaved ~= nil then
                curSaved = nextSaved
                savedTabsStore:Set(curSaved :: any)
            end
            if not isPrivate and not previewTab then
                local fav = if type(tAny.Favicon) == "function" then tAny:Favicon() else nil
                if type(fav) == "string" and fav ~= "" then
                    local withIcon = savedTabs.saveIcon(curSaved, routed.id, id, fav)
                    if withIcon ~= nil then
                        savedTabsStore:Set(withIcon :: any)
                    end
                end
            end
        end
        refresh()
    end)

    -- Picked elements open the native spotlight + comment editor (`surfaces/browser/comment.rs`).

    -- A loopback address opens on the machine that runs the server; the system browser here
    -- reaches it only when the server runs here too (authoritative Rust policy via CanOpenExternally).
    local function reachable(url: string, tab: BrowserTab?): boolean
        local tAny = tab :: any
        if tAny ~= nil and type(tAny.CanOpenExternally) == "function" then
            return tAny:CanOpenExternally() == true
        end
        local scheme, host = address.parts(url)
        local lower = string.lower(scheme)
        if lower ~= "http://" and lower ~= "https://" then
            return false
        end
        if string.find(host, "@", 1, true) ~= nil then
            return false
        end
        local name = string.match(host, "^%[[^%]]*%]") or string.match(host, "^[^:]*") or host
        name = string.gsub(name, "%.+$", "")
        return Server:IsLocal() or not address.loopback(name)
    end

    -- Where the keyboard goes when a new blank tab opens.
    type Keyboard = "address" | "kept"
    local kept: { [string]: boolean } = {}
    local opening: Keyboard? = nil

    local function openNew(sid: string?, keyboard: Keyboard)
        local session = if sid then Sessions:Get(sid) else Sessions:Routed()
        if session == nil or not Browser:Available() then
            return
        end
        opening = keyboard
        local tab = Browser:OpenAsync({ url = "about:blank", sessionId = session.id, background = keyboard == "kept" })
        opening = nil
        if tab and keyboard == "address" then
            edit(tab)
        elseif tab then
            kept[tab.Id] = true
        end
    end

    -- Site information: the page it describes, and its two-view cookie/cache state machine.
    local siteState: site.State = site.initial
    local sitePage: { tab: string, url: string }? = nil
    local serverConnected = true
    if Server.Connected ~= nil then
        Server.Connected:Connect(function(up: boolean)
            serverConnected = up
            refresh()
        end)
    end

    local function syncSiteDetailed(detailed: boolean)
        local bAny = Browser :: any
        if type(bAny.SetSiteDetailed) == "function" then
            bAny:SetSiteDetailed(detailed)
        end
    end

    local function siteDispatch(event: site.Event)
        local nextState, effects = site.step(siteState, event)
        siteState = nextState
        syncSiteDetailed(siteState.detailed)
        for _, effect in effects do
            local described = sitePage
            if effect.type == "notify-clear-failed" then
                Toast:Info(I18n:T("site.clearFailed"))
            elseif described then
                local attempt = effect.attempt
                local tab = Browser:Get(described.tab)
                if effect.type == "load" then
                    if not site.isWeb(described.url) then
                        siteDispatch({ type = "load.succeeded", attempt = attempt, cookies = 0 })
                        continue
                    end
                    task.delay(site.LOAD_TIMEOUT, function()
                        siteDispatch({ type = "timeout", phase = "loading", attempt = attempt })
                    end)
                    task.spawn(function()
                        local ok, count = pcall(function()
                            return if tab then tab:SiteCookiesAsync() else nil
                        end)
                        siteDispatch(if ok and type(count) == "number" then { type = "load.succeeded", attempt = attempt, cookies = count } else { type = "load.failed", attempt = attempt })
                    end)
                elseif effect.type == "clear-cache" then
                    task.delay(site.CLEAR_TIMEOUT, function()
                        siteDispatch({ type = "timeout", phase = "clearing", attempt = attempt })
                    end)
                    task.spawn(function()
                        local ok = pcall(function()
                            local tAny = tab :: any
                            if tAny == nil or type(tAny.ClearCacheAsync) ~= "function" then
                                error("unsupported", 0)
                            end
                            tAny:ClearCacheAsync()
                        end)
                        siteDispatch(if ok then { type = "clear.succeeded", attempt = attempt } else { type = "clear.failed", attempt = attempt })
                    end)
                else
                    task.delay(site.CLEAR_TIMEOUT, function()
                        siteDispatch({ type = "timeout", phase = "clearing", attempt = attempt })
                    end)
                    task.spawn(function()
                        local ok = pcall(function()
                            if tab == nil then
                                error("gone", 0)
                            end
                            tab:ClearSiteAsync()
                        end)
                        siteDispatch(if ok then { type = "clear.succeeded", attempt = attempt } else { type = "clear.failed", attempt = attempt })
                    end)
                end
            end
        end
        refresh()
    end

    local siteEnv: site.Env = {
        I18n = I18n,
        state = function()
            return siteState
        end,
        dispatch = siteDispatch,
        server = function()
            if Server:IsLocal() then
                return nil
            end
            for _, s in Servers:List() do
                if s.current then
                    return s.name
                end
            end
            return nil
        end,
        canClearCache = function()
            return engineHas("clearCache")
        end,
    }

    local bSig = Browser :: any
    if bSig.SiteBack ~= nil then
        bSig.SiteBack:Connect(function()
            if siteState.detailed then
                siteDispatch({ type = "details", shown = false })
            end
        end)
    end

    ctx:AddPopover({
        id = "site",
        title = I18n:T("site.info"),
        render = function(view, props)
            local data: { [string]: any } = props.data or {}
            return site.render(siteEnv, view, if type(data.url) == "string" then data.url else "")
        end,
    })

    local function act(what: string, tab: BrowserTab?, arg: any?)
        if what == "settings" then
            Popover:Close()
            Layout:OpenSettings("browser")
            return
        end
        if tab == nil then
            return
        end
        if what == "device" and device then
            Popover:Close()
            device.toggle(tab)
        elseif what == "edit" then
            edit(tab)
        elseif what == "focus" then
            -- A press on the edited field's own box: the input takes the keyboard again, its text and
            -- selection as they are (`panel.tsx:1000-1006`; `edit` would select the whole address).
            editor:Focus()
        elseif what == "go" then
            dispatch({ type = "navigated", tab = tab.Id, url = arg :: string, epoch = epoch(tab) })
        elseif what == "forget" then
            dispatch({ type = "forget", url = arg :: string?, visits = visits() })
            editor:Focus()
        elseif what == "zoom" then
            Popover:Close()
            tab:SetZoom(arg :: any)
        elseif what == "page" then
            Popover:Close()
            local tAny = tab :: any
            if type(tAny.OpenNote) == "function" then
                tAny:OpenNote()
            else
                local url = pageUrl(tab)
                local shownHost = hostDisplay.displayHost(url)
                local title = tab:Title()
                local labelText = if title ~= "" then title elseif shownHost ~= "" then shownHost else url
                Composer:InsertPill(`browser:{url}`, `{labelText} ({url})`)
            end
        elseif what == "link.copy" then
            Popover:Close()
            local url = pageUrl(tab)
            if url ~= "" and not isServerFile(url) then
                System:CopyText(url)
                Toast:Info(I18n:T("common.copied"))
            end
        elseif what == "external" then
            Popover:Close()
            local url = pageUrl(tab)
            if reachable(url, tab) then
                System:OpenExternal(url)
            else
                Toast:Error(I18n:T("menu.failed"))
            end
        elseif what == "reveal" then
            Popover:Close()
            local path = localFilePath(pageUrl(tab))
            if path then
                System:Reveal(path)
            end
        elseif what == "view.source" then
            Popover:Close()
            local session = Sessions:Routed()
            local rel = workspaceSourceRel(pageUrl(tab), session)
            if rel and session then
                local wsAny = Workspace :: any
                if type(wsAny.OpenFileSource) == "function" then
                    wsAny:OpenFileSource(session.id, rel)
                else
                    Workspace:OpenFile(session.id, rel)
                end
            end
        elseif what == "inspect" then
            Popover:Close()
            tab:Inspect()
        elseif what == "hard.reload" then
            Popover:Close()
            local tAny = tab :: any
            if type(tAny.ReloadIgnoreCache) == "function" then
                tAny:ReloadIgnoreCache()
            else
                tab:Reload()
            end
        elseif what == "history.clear" then
            Popover:Close()
            local keeper = recorder:Get()
            if keeper then
                keeper.clear()
            end
            -- The list empties at once; a history the app could not clear is said, and the item
            -- stays to try again (`panel.tsx` `history.clear.failed`). The web has only the list.
            local bAny = Browser :: any
            if type(bAny.HistoryClear) == "function" and bAny:HistoryClear() == false and App.Platform ~= "web" then
                Toast:Error(I18n:T("history.clear.failed"))
            end
        elseif what == "find.open" then
            Popover:Close()
            openFind(tab)
        elseif what == "find.next" then
            stepFind(tab, true)
        elseif what == "find.previous" then
            stepFind(tab, false)
        elseif what == "find.close" then
            closeFind(tab, true)
        elseif what == "refusal.action" then
            local notice = arg :: refusal.Notice?
            if notice and notice.target then
                -- Plugin refusal notices are strictly copy-only; OS app launches occur exclusively
                -- through the native host-rendered review bar (`surfaces/browser/prompts/app_link.rs`).
                System:CopyText(notice.target)
                Toast:Info(I18n:T("common.copied"))
            end
            dismissedRefusals[tab.Id] = tab:Rejected()
            refusalNotices[tab.Id] = nil
            refresh()
        elseif what == "refusal.dismiss" then
            dismissedRefusals[tab.Id] = tab:Rejected()
            refusalNotices[tab.Id] = nil
            refresh()
        elseif what == "failure.dismiss" then
            failureNotices[tab.Id] = nil
            refresh()
        elseif what == "options" then
            local url = pageUrl(tab)
            local localPath = localFilePath(url)
            local session = Sessions:Routed()
            local canSource = workspaceSourceRel(url, session) ~= nil
            local eng = Browser:Engine() :: any
            local canFind = eng == nil or eng.state ~= "ready" or eng.find == true
            local menuData: options.MenuData = {
                tabId = tab.Id,
                url = url,
                zoom = tab:Zoom(),
                inspectable = url ~= "" and tab:LoadError() == nil,
                suspended = tab:Refusal() == "browser.proxy.required",
                failed = tab:LoadError() ~= nil,
                serverFile = isServerFile(url),
                canOpenExternal = reachable(url, tab),
                canRevealFile = localPath ~= nil and Server:IsLocal(),
                canViewSource = canSource,
                canFind = canFind,
                canHardReload = true,
                -- Always, as official's desktop (`historyClearable`): the app's history can hold
                -- pages this window's list does not, those imported from another browser.
                canClearHistory = App.Platform ~= "web" or #visits() > 0,
                canToggleToolbar = device ~= nil,
                paneWidth = if type(arg) == "number" then arg else nil,
            }
            Popover:Toggle({
                id = "options",
                anchorKey = "browser-options",
                side = "bottom",
                align = "end",
                gap = 4,
                data = menuData :: any,
            })
        elseif what == "extensions" then
            Popover:Toggle({
                id = "extensions",
                anchorKey = "browser-extensions",
                side = "bottom",
                align = "end",
                gap = 4,
                data = { tabId = tab.Id } :: any,
            })
        elseif what == "site" then
            local url = pageUrl(tab)
            if sitePage == nil or sitePage.tab ~= tab.Id or sitePage.url ~= url then
                siteState = site.initial
            end
            sitePage = { tab = tab.Id, url = url }
            if not Popover:IsOpen("site") then
                siteDispatch({ type = "opened" })
            else
                siteDispatch({ type = "closed" })
            end
            Popover:Toggle({ id = "site", anchorKey = "browser-site-anchor", side = "bottom", align = "start", gap = 8, data = { url = url, tab = tab.Id } })
        end
    end

    local optionsCb: options.Callbacks = {
        I18n = I18n,
        act = function(action: string, tabId: string, arg: any?)
            act(action, Browser:Get(tabId), arg)
        end,
    }

    ctx:AddPopover({
        id = "options",
        title = I18n:T("options"),
        render = function(view, props)
            return options.renderMain(optionsCb, view, (props.data or {}) :: any)
        end,
    })

    ctx:AddPopover({
        id = "more-tools",
        title = I18n:T("menu.more"),
        render = function(view, props)
            return options.renderMore(optionsCb, view, (props.data or {}) :: any)
        end,
    })

    local function enabledExtActions(): { extensions.ToolbarAction }
        if not Server:IsLocal() then
            return {}
        end
        local bAny = Browser :: any
        if type(bAny.ExtActions) ~= "function" then
            return {}
        end
        local raw = extensionsStore:Get()
        local stored = if type(raw) == "table" then raw else {}
        local ok, res = pcall(function()
            return bAny:ExtActions(stored)
        end)
        if ok and type(res) == "table" then
            return res :: any
        end
        return {}
    end

    ctx:AddPopover({
        id = "extensions",
        title = I18n:T("settings.extensions.title"),
        render = function(view, props)
            local tabId = if type(props.data) == "table" then (props.data :: any).tabId else nil
            return extensions.renderToolbarMenu(
                enabledExtActions(),
                I18n,
                view,
                function(item)
                    Popover:Close("extensions")
                    local bAny = Browser :: any
                    if type(bAny.ExtClick) == "function" then
                        local optUrl = if item.options and item.options ~= "" and (item.popup == nil or item.popup == "")
                            then (if string.sub(item.options, 1, 19) == "chrome-extension://" then item.options else `chrome-extension://{item.id}/{item.options}`)
                            else nil
                        bAny:ExtClick(item.id, tabId, optUrl)
                    end
                end,
                function()
                    Popover:Close("extensions")
                    act("settings", if tabId then Browser:Get(tabId) else nil)
                end
            )
        end,
    })

    -- The pane's sign-in offer (`signins.luau`), set once its jobs exist below.
    local signInOffer: ((view: View, tab: BrowserTab, variant: string) -> ui.Node?)? = nil

    local env: panel.Env = {
        device = if device then device.render else nil,
        deviceOpen = if device then device.isOpen else nil,
        I18n = I18n,
        Browser = Browser,
        Server = Server,
        System = System,
        menuTrigger = true,
        editor = editor,
        findEditor = findEditor,
        state = function()
            return state
        end,
        epoch = epoch,
        visits = visits,
        reachable = reachable,
        siteOpen = function()
            return Popover:IsOpen("site")
        end,
        optionsOpen = function()
            return Popover:IsOpen("options")
        end,
        hasExtensions = function()
            return #enabledExtActions() > 0
        end,
        extMenuOpen = function()
            return Popover:IsOpen("extensions")
        end,
        serverConnected = function()
            return serverConnected
        end,
        refusalNotice = function(tab: BrowserTab)
            return refusalNotices[tab.Id]
        end,
        failureNotice = function(tab: BrowserTab)
            return failureNotices[tab.Id]
        end,
        findView = function(tab: BrowserTab)
            local v = find.view(getFind(tab.Id))
            if v == nil then
                return nil
            end
            local tAny = tab :: any
            if v.query ~= "" and type(tAny.FindResult) == "function" then
                local res = tAny:FindResult()
                if type(res) == "table" and (res.query == nil or res.query == v.query) and type(res.active) == "number" and type(res.total) == "number" then
                    return { query = v.query, result = { active = res.active, total = res.total } }
                end
            end
            return v
        end,
        revealGuarded = function(tab: BrowserTab)
            local untilMs = revealUntil[tab.Id]
            return untilMs ~= nil and System:Now() < untilMs
        end,
        act = act,
        signIns = function(view: View, tab: BrowserTab, variant: string): ui.Node?
            return if signInOffer then signInOffer(view, tab, variant) else nil
        end,
    }

    -- A tab's label: "New tab" for a blank or unnamed link-opened page, else live/saved title, else host/URL.
    local function label(sid: string, info: BrowserTabInfo): string
        if info.url == "" then
            return I18n:T("tab.new")
        end
        local saved = savedTabs.find(savedTabs.normalize(savedTabsStore:Get()), sid, info.id)
        local live = Browser:Get(info.id)
        if live ~= nil then
            local liveAny = live :: any
            if type(liveAny.Unnamed) == "function" and liveAny:Unnamed() == true then
                return I18n:T("tab.new")
            end
        end
        local held = savedTabs.tabTitle(info.url, info.title, if live then live:Loading() else true, saved)
        if held ~= nil then
            return held
        end
        local host = hostDisplay.displayHost(info.url)
        return if host ~= "" then host else info.url
    end

    local function tabText(sid: string, tab: BrowserTab): string
        local url = pageUrl(tab)
        local tAny = tab :: any
        if url == "" or (type(tAny.Unnamed) == "function" and tAny:Unnamed() == true) then
            return I18n:T("tab.new")
        end
        local saved = savedTabs.find(savedTabs.normalize(savedTabsStore:Get()), sid, tab.Id)
        local held = savedTabs.tabTitle(url, tab:Title(), tab:Loading(), saved)
        if held ~= nil then
            return held
        end
        local host = hostDisplay.displayHost(url)
        return if host ~= "" then host else url
    end

    panelHandle = ctx:AddPanel({
        id = PANEL,
        title = I18n:T("tab.title"),
        icon = "globe",
        launcher = false,
        transient = true,
        list = function(sid: string, open: { string }): { PanelTab }
            local out: { PanelTab } = {}
            for _, info in Browser:List(sid, open) do
                if info.open then
                    table.insert(out, { instance = info.id, title = label(sid, info), icon = "globe", fallback = true })
                end
            end
            return out
        end,
        label = function(_view, props)
            local id = props.instance or ""
            local tab = if id ~= "" then Browser:Get(id) else nil
            if tab == nil then
                return nil
            end
            local sid = props.sessionId or ""
            local title = tabText(sid, tab)
            local labelColor: Color = if props.active then "text-chrome" else "text-muted"
            local iconNode: ui.Node
            local tAny = tab :: any
            local liveFav = if type(tAny.Favicon) == "function" then tAny:Favicon() else nil
            local saved = savedTabs.find(savedTabs.normalize(savedTabsStore:Get()), sid, id)
            local fav = savedTabs.tabIcon(pageUrl(tab), liveFav, saved)
            if fav ~= nil then
                iconNode = ui.image({ src = fav, w = 16, h = 16 }, {
                    ui.icon("globe", { size = 16, color = labelColor }),
                })
            else
                iconNode = ui.icon("globe", { size = 16, color = labelColor })
            end
            local children: { ui.Node } = {
                iconNode,
                ui.text(title, {
                    size = 13,
                    weight = "regular",
                    lineHeight = 16,
                    tracking = -0.04,
                    tabular = true,
                    noWrap = true,
                    color = labelColor,
                    truncate = true,
                    shrink = true,
                    minW = 0,
                    maxW = 160,
                }),
            }
            local asking = type(tAny.Asking) == "function" and tAny:Asking() == true
            local unseen = not asking
                and (unseenTabs[id] == true or (type(tAny.Unseen) == "function" and tAny:Unseen() == true))
            -- Official `renderer.tsx:536-538`: amber `prompt.tab` ("Waiting for you") takes precedence
            -- over blue `tab.activity` ("The agent used this tab"), both 6×6 rounded dots.
            if asking then
                table.insert(children, ui.box({
                    key = `browser-tab-dot-{id}`,
                    role = "img",
                    label = I18n:T("prompt.tab"),
                    w = 6,
                    h = 6,
                    radius = 3,
                    fill = "status-warning",
                }))
            elseif unseen then
                table.insert(children, ui.box({
                    key = `browser-tab-dot-{id}`,
                    role = "img",
                    label = I18n:T("tab.activity"),
                    w = 6,
                    h = 6,
                    radius = 3,
                    fill = "status-info",
                }))
            end
            return ui.row({ gap = 6, align = "center", minW = 0, shrink = true }, children)
        end,
        render = function(view, props)
            return panel.render(env, view, props)
        end,
        -- A blank tab starts in its address field, like a browser's new tab; focusing clears unseen dot.
        onFocus = function(props)
            local tab = if props.instance then Browser:Get(props.instance) else nil
            if tab == nil then
                return
            end
            if unseenTabs[tab.Id] then
                unseenTabs[tab.Id] = nil
                refresh()
            end
            if opening == "kept" or kept[tab.Id] then
                kept[tab.Id] = nil
                return
            end
            if props.restored ~= true and pageUrl(tab) == "" then
                edit(tab)
            end
        end,
        onClose = function(props)
            local tab = if props.instance then Browser:Get(props.instance) else nil
            if tab then
                tab:Close()
            end
        end,
        closedTab = function(props): ClosedTab?
            local tab = if props.instance then Browser:Get(props.instance) else nil
            if tab == nil or isPreviewTab(tab) then
                return nil
            end
            local tAny = tab :: any
            if type(tAny.IsPrivate) == "function" and tAny:IsPrivate() == true then
                return nil
            end
            local url = pageUrl(tab)
            if url == "" or not history.recordable(url) then
                return nil
            end
            local host = hostDisplay.displayHost(url)
            if host == "" then
                local _, raw = address.parts(url)
                host = raw
            end
            return { icon = "globe", detail = host, data = url }
        end,
        restore = function(props, data: string?)
            if data and data ~= "" then
                Browser:OpenAsync({ url = data, sessionId = props.sessionId })
            end
        end,
    })

    ctx:AddCommand({
        id = "open",
        title = I18n:T("command.open"),
        group = I18n:T("command.category.view"),
        bind = if App.Platform == "macos" then "ctrl+mod+b" else "ctrl+alt+b",
        editable = true,
        requiresSession = true,
        run = function()
            if not Browser:Available() then
                return false
            end
            openNew(nil, if Layout:RegionFocused() then "address" else "kept")
            return nil
        end,
    })

    -- The routed session's page on screen, for F5.
    ctx:AddCommand({
        id = "reload",
        title = I18n:T("command.reload"),
        group = I18n:T("command.category.view"),
        bind = "f5",
        editable = true,
        requiresSession = true,
        run = function()
            local tab = Browser:Selected(nil)
            if tab == nil or pageUrl(tab) == "" then
                return false
            end
            tab:Reload()
            return nil
        end,
    })

    local function scoped(id: string, bind: string, hidden: boolean, title: string, handler: (tab: BrowserTab) -> boolean, follows: string?)
        ctx:AddCommand({
            id = id,
            title = title,
            group = I18n:T("command.category.view"),
            bind = bind,
            follows = follows,
            hidden = hidden,
            editable = true,
            requiresSession = true,
            run = function()
                local tab = Browser:Focused()
                if tab == nil or not handler(tab) then
                    return false
                end
                return nil
            end,
        })
    end
    scoped("reloadPage", "mod+r", true, I18n:T("command.reload"), function(tab)
        tab:Reload()
        return true
    end)
    scoped("hardReload", if App.Platform == "macos" then "mod+alt+r" else "ctrl+f5", false, I18n:T("menu.hardReload"), function(tab)
        if pageUrl(tab) == "" then
            return false
        end
        act("hard.reload", tab)
        return true
    end)
    scoped("find", "mod+f", false, I18n:T("find.open"), function(tab)
        if pageUrl(tab) == "" or tab:LoadError() ~= nil then
            return false
        end
        openFind(tab)
        return true
    end)
    scoped("find.next", if App.Platform == "macos" then "mod+g" else "mod+g,f3", false, I18n:T("find.next"), function(tab)
        if pageUrl(tab) == "" or tab:LoadError() ~= nil then
            return false
        end
        act("find.next", tab)
        return true
    end)
    scoped("find.previous", if App.Platform == "macos" then "mod+shift+g" else "mod+shift+g,shift+f3", false, I18n:T("find.previous"), function(tab)
        if pageUrl(tab) == "" or tab:LoadError() ~= nil then
            return false
        end
        act("find.previous", tab)
        return true
    end)
    if device then
        scoped("toggleToolbar", "mod+shift+m", false, I18n:T("command.toggleToolbar"), function(tab)
            if pageUrl(tab) == "" or tab:LoadError() ~= nil then
                return false
            end
            device.toggle(tab)
            -- The toggle is an item of the Options menu, so the shortcut leaves the keyboard where choosing that item
            -- does: on the menu's button (`panel.tsx` `toggleToolbar`).
            Layout:Focus("browser-options")
            return true
        end)
    end
    scoped("address", "mod+l", false, I18n:T("command.address"), function(tab)
        edit(tab)
        return true
    end)
    -- Ctrl+W is the shell's `tab.close`: Settings > Shortcuts lists that one (Desktop's `follows`).
    scoped("close", "mod+w", true, I18n:T("command.close"), function(tab)
        tab:Close()
        return true
    end, "tab.close")
    scoped("inspect", "mod+shift+c", false, I18n:T("command.inspect"), function(tab)
        if pageUrl(tab) == "" then
            return false
        end
        tab:Inspect()
        return true
    end)

    -- Side panel New tab: browser address bar across the top and recent pages below launchers
    -- (`packages/gui-plugins/src/browser/renderer.tsx:409-447`, opencode#66818).
    local function openFromNewTab(sid: string?, pane: string?, url: string)
        newTabTarget = { sessionId = sid, pane = pane }
        dispatch({ type = "navigated", tab = address.NEW_TAB_ID, url = url, epoch = address.BLANK_EPOCH })
    end

    newTabBarHandle = ctx:AddSlot({
        id = "new-tab-bar",
        at = "session.panel.new",
        render = function(view, props)
            if not Browser:Available() then
                return nil
            end
            local sid = props.sessionId
            local pane = props.pane
            local width = props.width or 480
            if props.focusToken ~= nil and props.focusToken ~= lastFocusToken then
                lastFocusToken = props.focusToken
                task.delay(0, function()
                    editNewTab(sid, pane)
                end)
            end
            return panel.newTabBar(
                env,
                view,
                width,
                function()
                    editNewTab(sid, pane)
                end,
                function()
                    editor:Focus()
                end,
                function(url: string)
                    openFromNewTab(sid, pane, url)
                end,
                function(url: string)
                    dispatch({ type = "forget", url = url, visits = visits() })
                    editor:Focus()
                end
            )
        end,
    })

    newTabRecentHandle = ctx:AddSlot({
        id = "new-tab-recent",
        at = "session.panel.new.list",
        render = function(view, props)
            if not Browser:Available() then
                return nil
            end
            local sid = props.sessionId
            local pane = props.pane
            local width = props.width or 480
            return panel.newTabRecent(env, view, width, function(url: string)
                openFromNewTab(sid, pane, url)
            end)
        end,
    })

    -- Expose an internal contract so agent tools or link routing can trigger a reveal click guard
    -- or show an external app-scheme refusal notice inside the browser pane.
    Contracts:Provide("browser.chrome", {
        markAgentActivity = function(tabId: string, revealed: boolean?)
            local selected = Browser:Selected(nil)
            if selected == nil or selected.Id ~= tabId then
                unseenTabs[tabId] = true
            end
            if revealed then
                revealUntil[tabId] = System:Now() + REVEAL_GUARD_MS
                task.delay(REVEAL_GUARD_MS / 1000 + 0.02, function()
                    refresh()
                end)
            end
            refresh()
        end,
        showFailure = function(tabId: string, text: string, lasting: boolean?)
            showFailure(tabId, text, lasting == true, nil)
        end,
        showExternalNotice = function(tabId: string, target: string, source: refusal.Source?)
            local action = refusal.externalTarget(target, source)
            if action == nil then
                return false
            end
            local origin = if source == "conversation" then I18n:T("notice.conversation") else I18n:T("notice.page")
            nextNoticeId += 1
            -- External contract notices are copy-only: OS app launches require a single-use
            -- host-minted review token from `Browser:TakeAppLinkReview()` (never a plugin call).
            refusalNotices[tabId] = {
                token = `ext:{tabId}:{nextNoticeId}`,
                origin = origin,
                text = I18n:T("refused.scheme"),
                target = target,
                action = "copy",
                shownAtMs = System:Now(),
            }
            refresh()
            return true
        end,
    })

    local function preferences(): settings.Preferences
        return settings.normalizePreferences(prefsStore:Get())
    end

    local function updatePreferences(patch: { [string]: any })
        local nextPrefs: { [string]: any } = table.clone(preferences())
        for k, v in patch do
            nextPrefs[k] = v
        end
        prefsStore:Set(nextPrefs)
    end

    local function sitePermissionsList(): { settings.SiteDecision }
        local raw = sitePermsStore:Get()
        if type(raw) ~= "table" then
            return {}
        end
        local out: { settings.SiteDecision } = {}
        for _, item in raw :: { any } do
            if type(item) == "table" and type(item.origin) == "string" and string.match(string.lower(item.origin), "^https?://") ~= nil and type(item.permission) == "string" and (item.setting == "allow" or item.setting == "block") then
                table.insert(out, {
                    origin = item.origin,
                    permission = item.permission,
                    setting = item.setting,
                })
            end
        end
        return out
    end

    local function setSitePermission(origin: string, permission: string, setting: "allow" | "block")
        if string.match(string.lower(origin), "^https?://") == nil then
            return
        end
        local list = sitePermissionsList()
        local updated = false
        for _, d in list do
            if d.origin == origin and d.permission == permission then
                d.setting = setting
                updated = true
                break
            end
        end
        if not updated then
            table.insert(list, { origin = origin, permission = permission, setting = setting })
        end
        sitePermsStore:Set(list :: any)
    end

    local function removeSitePermission(origin: string, permission: string)
        local list = sitePermissionsList()
        local nextList: { settings.SiteDecision } = {}
        for _, d in list do
            if not (d.origin == origin and d.permission == permission) then
                table.insert(nextList, d)
            end
        end
        sitePermsStore:Set(nextList :: any)
    end

    local function storedExtensions(): { any }
        local raw = extensionsStore:Get()
        return if type(raw) == "table" then raw :: { any } else {}
    end

    local function extNativeAvailable(): boolean
        local bAny = Browser :: any
        return type(bAny.ExtAvailable) == "function" and bAny:ExtAvailable() == true
    end

    -- Sync verified on-disk startup extension specs once on window boot (never discovers other browsers).
    if extNativeAvailable() then
        local bAny = Browser :: any
        if type(bAny.ExtSync) == "function" then
            bAny:ExtSync(storedExtensions(), false)
        end
    end

    local function settingsCapabilities(): settings.Capabilities
        local prof = profileContract:Get()
        local ext = extContract:Get()
        local avail = Browser:Available()
        return {
            available = avail,
            import = avail,
            extensions = extNativeAvailable() or (type(ext) == "table" and type(ext.renderSettings) == "function"),
            remoteSignIns = avail and type((Browser :: any).ShareSignIns) == "function",
            cookieFile = avail and type((Browser :: any).ImportCookieFileAsync) == "function",
        }
    end

    local settingsHandle: Contribution? = nil
    local importDialogHandle: Contribution? = nil
    local extDialogHandle: Contribution? = nil
    local clearNoticeText: string? = nil
    local clearing = false
    local settingsDrawnAt: number? = nil
    local clearNoticeDrawn = false
    -- Browsers are listed only when asked (never at startup), by the host off the UI thread, again once a
    -- listing is a few seconds old, so a visit sees browsers installed since the last one.
    local importSources: { settings.ImportBrowser } = {}
    -- The profile the import dialog is for (`badge`: its name, for a browser that has several), what
    -- its last import said, and each row's last outcome.
    local importTarget: { browser: string, name: string, profile: string, label: string, badge: string? }? = nil
    -- Set while the dialog was opened from the pane's offer: its import is then the offer's, which says
    -- how it went and loads the pane's page again (`import-prompt.tsx:87-92`).
    local importOffer: { tab: BrowserTab? }? = nil
    -- An import of the offer's runs on when its dialog is closed, and the offer waits for it.
    local offerImporting = false
    -- Set while the dialog was opened by a page's passkey notice (`site-chooser.tsx:43-47`): the page's
    -- site, chosen already only in a tab of the person's; the tab, which loads again once cookies came
    -- in; and whether the dialog still moves on through the profiles for one that lists the site.
    -- A notice that names no site opens the whole dialog (`panel.tsx:607`).
    local importSite: { name: string?, choose: boolean, tab: string, seeking: boolean }? = nil
    local importFailure: settings.ImportMessage? = nil
    -- What the dialog brings, both until unticked; each opening starts from both (`site-chooser.tsx:86-87`).
    local importBoxes: settings.ImportBoxes = { signins = true, history = true }
    local importRunning = false
    -- The dialog's sites (`site-chooser.tsx:60-131`): each profile's list, read once while the dialog is
    -- open (a failure is not kept, so Try again reads again); `scope` is "All sites" or "Only sites
    -- I choose", under which the ticked sites are one profile's and start with none.
    local siteLists: { [string]: { settings.ProfileSite } } = {}
    local siteFailures: { [string]: string } = {}
    local siteLoading: { [string]: boolean } = {}
    local siteChoice = { scope = "all", profile = "", chosen = {} :: { [string]: boolean }, query = "", version = 0 }
    local siteSearch = ctx:CreateEditor("")
    -- Set below, once the outcome text it needs is defined.
    local runDialogImport: () -> () = function() end
    local importResults: { [string]: settings.ImportMessage } = {}
    local importIcons: { [string]: ImageId | false } = {}
    local extDialogState: extensions.DialogState = {
        busy = nil,
        installingId = nil,
        choices = {},
        selectedKey = nil,
        candidates = {},
        candidatesLoaded = false,
        review = nil,
        showAllSites = false,
        approving = false,
        message = nil,
    }

    -- Whether the host's first listing is in: before it, the page says nothing of other browsers
    -- (`settings.tsx:151`, `jobs.ts:313` `sourcesLoaded`).
    local importSourcesLoaded = false

    local function listImportSources(): { settings.ImportBrowser }
        -- The host lists them off the UI thread and keeps the last listing, so asking is cheap.
        local bAny = Browser :: any
        -- Nothing until the first listing is in; a host without the method has none to list.
        local listed: any = if type(bAny.HistoryImportSources) == "function" then bAny:HistoryImportSources() else {}
        importSourcesLoaded = listed ~= nil
        importSources = listed or {}
        return importSources
    end

    local function refreshImportDialog()
        if importDialogHandle then
            importDialogHandle:Refresh()
        end
    end

    local function profileKey(target: { browser: string, profile: string }): string
        return `{target.browser}\n{target.profile}`
    end

    -- Set below: the passkey dialog's move to the next profile once a profile's list came in.
    local seekSite: (target: { browser: string, profile: string }, list: { settings.ProfileSite }?) -> () = function() end

    -- Reads the profile's sites off the UI thread, once while the dialog is open (`site-chooser.tsx:105-118`).
    local function readSites(target: { browser: string, profile: string })
        local key = profileKey(target)
        local bAny = Browser :: any
        if siteLists[key] ~= nil or siteLoading[key] or type(bAny.ListSourceSitesAsync) ~= "function" then
            return
        end
        siteLoading[key] = true
        siteFailures[key] = nil
        -- The read belongs to this opening of the dialog: closing or reopening it replaces the three
        -- tables, so a read still running from before lands in the old ones and never fills the new
        -- dialog (official aborts it: `site-chooser.tsx:107-114`).
        local lists, failures, loading = siteLists, siteFailures, siteLoading
        task.spawn(function()
            local ok, res = pcall(function()
                return bAny:ListSourceSitesAsync(target.browser, target.profile)
            end)
            loading[key] = nil
            if lists ~= siteLists then
                return
            end
            if ok and type(res) == "table" and type(res.sites) == "table" then
                lists[key] = res.sites
            else
                -- A thrown read is `listed.error`: "Couldn't read the sites in this browser."
                failures[key] = if ok and type(res) == "table" and type(res.error) == "string" then res.error else "thrown"
            end
            if importSite ~= nil and importSite.seeking and importTarget ~= nil and profileKey(importTarget) == key then
                seekSite(target, lists[key])
            end
            siteChoice.version += 1
            refreshImportDialog()
        end)
    end

    -- Moves the dialog to another browser profile, as its profile menu does.
    local function setImportTarget(choice: signins.Choice)
        importTarget = { browser = choice.sourceId, name = choice.sourceName, profile = choice.profileId, label = choice.label, badge = choice.profileName }
    end

    -- Opened for a site, the dialog moves on to the next browser profile until one lists exactly that
    -- site, and back to the first if none does; there the site is chosen already where the person
    -- asked (`site-chooser.tsx:215-229`, `site-search.ts` `exactSite`).
    seekSite = function(target: { browser: string, profile: string }, list: { settings.ProfileSite }?)
        local site = importSite
        if site == nil or site.name == nil then
            return
        end
        local name = site.name :: string
        for _, item in list or {} do
            if item.site == name then
                site.seeking = false
                siteChoice.scope = "some"
                siteChoice.profile = profileKey(target)
                siteChoice.chosen = if site.choose then { [name] = true } else {}
                return
            end
        end
        local choices = signins.choices(importSources)
        local at = 0
        for i, choice in choices do
            if choice.sourceId == target.browser and choice.profileId == target.profile then
                at = i
            end
        end
        local nextChoice = choices[at + 1]
        if nextChoice == nil then
            site.seeking = false
            if choices[1] ~= nil then
                setImportTarget(choices[1])
            end
            return
        end
        setImportTarget(nextChoice)
        readSites(importTarget :: any)
    end

    -- `site-search.ts` `matchSites`: a part of a site's name, or the host of a pasted address, which also
    -- matches the site it is a subdomain of; only the parsed host is matched so, never the rest of an
    -- address. An empty search keeps every site.
    local function matchSites(sites: { settings.ProfileSite }, query: string): { settings.ProfileSite }
        local text = string.lower(string.match(query, "^%s*(.-)%s*$") or "")
        if text == "" then
            return sites
        end
        local rest = string.gsub(text, "^[%a][%w+.-]*://", "")
        rest = string.match(rest, "^[^/?#]*") or ""
        rest = string.gsub(rest, "^.*@", "")
        local host = string.match(rest, "^%[[^%]]*%]") or string.match(rest, "^[^:]*") or ""
        local out = {}
        for _, item in sites do
            local site = item.site
            if string.find(site, text, 1, true)
                or (host ~= "" and (string.find(site, host, 1, true) ~= nil or string.sub(host, -(#site + 1)) == "." .. site))
            then
                table.insert(out, item)
            end
        end
        return out
    end

    -- The open dialog's sites (`site-chooser.tsx:96-163`, `canImport` at `:243-248`).
    local function siteChooserFor(target: { browser: string, name: string, profile: string }, busy: boolean): settings.SiteChooser?
        local bAny = Browser :: any
        if type(bAny.ListSourceSitesAsync) ~= "function" then
            return nil
        end
        local key = profileKey(target)
        local list = siteLists[key]
        local all = list or {}
        -- "All sites" brings every one. Under "Only sites I choose" the ticked sites are one
        -- profile's: under another profile nothing is chosen until the person chooses there
        -- (`site-chooser.tsx:96-100`).
        local some = siteChoice.scope == "some"
        local function isChosen(site: string): boolean
            return not some or (siteChoice.profile == key and siteChoice.chosen[site] == true)
        end
        local chosen = 0
        for _, item in all do
            if isChosen(item.site) then
                chosen += 1
            end
        end
        local shown = matchSites(all, siteChoice.query)
        local shownChosen = 0
        for _, item in shown do
            if isChosen(item.site) then
                shownChosen += 1
            end
        end
        local failure = siteFailures[key]
        -- Searched for the passkey page's site, which this profile has no cookies for: the list says so,
        -- and chooses nothing (`site-chooser.tsx:138-145`).
        local missing: string? = nil
        local siteName = if importSite ~= nil then importSite.name else nil
        if siteName ~= nil and list ~= nil and string.lower(string.match(siteChoice.query, "^%s*(.-)%s*$") or "") == siteName then
            local exact = false
            for _, item in all do
                exact = exact or item.site == siteName
            end
            if not exact then
                missing = I18n:T("sites.missing", { site = siteName, browser = target.name })
            end
        end
        local status: { text: string, error: boolean, loading: boolean?, retry: boolean? }? = nil
        if importSite ~= nil and importSite.seeking then
            status = { text = I18n:T("sites.loading", { browser = target.name }), error = false, loading = true }
        elseif failure == "thrown" then
            status = { text = I18n:T("sites.failed"), error = true, retry = true }
        elseif failure == "locked" then
            status = { text = I18n:T("signins.failed.locked.named", { browser = target.name }), error = true, retry = true }
        elseif failure ~= nil then
            local reasonKey = `signins.failed.{failure}`
            status = { text = I18n:T(if I18n:Has(reasonKey) then reasonKey else "sites.failed"), error = true, retry = true }
        elseif list == nil then
            status = { text = I18n:T("sites.loading", { browser = target.name }), error = false, loading = true }
        elseif #all == 0 then
            status = { text = I18n:T("sites.empty"), error = false }
        elseif #shown == 0 then
            status = { text = missing or I18n:T("sites.unmatched"), error = false }
        end
        -- The "From" picker only where the dialog has several profiles to offer, as the pane's offer
        -- and a passkey notice do; a Settings row has picked its profile already
        -- (`settings.tsx:72` `choices={[choice]}`, `site-chooser.tsx:380`).
        local choices = signins.choices(importSources)
        local profiles: { { value: string, label: string } }? = nil
        if #choices > 1 and (importOffer ~= nil or importSite ~= nil) then
            local options = {}
            for _, choice in choices do
                table.insert(options, { value = `{choice.sourceId}\n{choice.profileId}`, label = choice.label })
            end
            profiles = options
        end
        -- Changes the dialog's ticked sites, which are this profile's own (`site-chooser.tsx:231-235`).
        local function choose(change: ({ [string]: boolean }) -> ())
            local next: { [string]: boolean } = {}
            for _, item in all do
                if isChosen(item.site) then
                    next[item.site] = true
                end
            end
            change(next)
            siteChoice.profile = key
            siteChoice.chosen = next
            siteChoice.version += 1
            refreshImportDialog()
        end
        return {
            scope = siteChoice.scope,
            everything = not some or (#all > 0 and chosen == #all),
            chosen = chosen,
            total = #all,
            shown = shown,
            shownChosen = shownChosen,
            isChosen = isChosen,
            query = siteChoice.query,
            editor = siteSearch,
            ready = list ~= nil and #all > 0,
            status = status,
            profiles = profiles,
            profile = key,
            version = `{key}:{siteChoice.version}`,
            -- Importing everything needs no list: it works while the list is still being read.
            canImport = if some then chosen > 0 else (list == nil or #all > 0),
            Menu = Menu,
            -- `site-chooser.tsx:454`: the ticked sites stay as they are while "All sites" is picked.
            onScope = function(scope: string)
                if busy then
                    return
                end
                siteChoice.scope = if scope == "some" then "some" else "all"
                siteChoice.version += 1
                refreshImportDialog()
            end,
            onToggle = function(site: string)
                if busy then
                    return
                end
                choose(function(next)
                    next[site] = if next[site] then nil else true
                end)
            end,
            -- Both act on the sites the search shows, which is every site without a search.
            onSelectShown = function()
                choose(function(next)
                    for _, item in shown do
                        next[item.site] = true
                    end
                end)
            end,
            onClearShown = function()
                choose(function(next)
                    for _, item in shown do
                        next[item.site] = nil
                    end
                end)
            end,
            onRetry = function()
                siteFailures[key] = nil
                readSites(target)
                refreshImportDialog()
            end,
            single = importSite ~= nil and importSite.name ~= nil,
            missing = missing,
            onProfile = function(value: string)
                for _, choice in choices do
                    if `{choice.sourceId}\n{choice.profileId}` == value then
                        setImportTarget(choice)
                        -- The person's pick ends the passkey dialog's own search through the profiles
                        -- (`site-chooser.tsx:208-211`); the choice between all and some stays.
                        if importSite ~= nil then
                            importSite.seeking = false
                        end
                        siteChoice.version += 1
                        readSites(importTarget :: any)
                        refreshImportDialog()
                        return
                    end
                end
            end,
        }
    end

    siteSearch.Changed:Connect(function(value: string)
        siteChoice.query = value
        siteChoice.version += 1
        refreshImportDialog()
    end)
    -- With text in it, Escape clears the search; once empty, it closes the dialog (`site-chooser.tsx:488-505`).
    siteSearch.KeyPressed:Connect(function(key: string)
        if key == "Escape" and siteSearch:Text() ~= "" then
            siteSearch:SetText("")
        end
    end)

    -- Settings > Browser's cookie-file import: running, and how the last one ended.
    local cookieFileBusy = false
    local cookieFileMessage: settings.ImportMessage? = nil

    -- `jobs.ts:186-276` for an `import-file` job: a file names no browser and its cookies are counted.
    local function describeCookieFile(res: any): settings.ImportMessage
        if res == nil or type(res.error) == "string" then
            local reason = if res then res.error else nil
            local key = if reason == "read" then "signins.failed.read.file"
                elseif reason == "format" then "signins.failed.format"
                elseif reason == "storage" then "signins.failed.storage"
                else "settings.failed.import"
            return { text = I18n:T(key), error = true, live = true }
        end
        local imported = tonumber(res.imported) or 0
        local lost = (tonumber(res.failed) or 0) + (tonumber(res.protected) or 0)
        if lost > 0 then
            return { text = I18n:T("signins.imported.incomplete", { count = imported, omitted = lost }), error = imported == 0, live = true }
        end
        return { text = I18n:T("signins.imported", { count = imported }), error = imported == 0, live = true }
    end

    -- Each browser's icon is kept once a window, and one without an icon is not asked again.
    local function importIcon(browserId: string): ImageId?
        local known = importIcons[browserId]
        if known ~= nil then
            return if known == false then nil else known :: ImageId
        end
        local bAny = Browser :: any
        local image: ImageId? = nil
        if type(bAny.HistoryImportIcon) == "function" then
            local ok, kind, data, width, height = pcall(function()
                return bAny:HistoryImportIcon(browserId)
            end)
            if ok and kind == "rgba" then
                image = Images:FromRgba(data, width, height)
            elseif ok and kind == "png" then
                image = Images:FromEncoded(data)
            elseif not ok or kind == nil then
                -- Still being read off the UI thread: the page draws again when it is.
                return nil
            end
        end
        importIcons[browserId] = if image ~= nil then image else false
        return image
    end

    local function refreshSettings()
        if settingsHandle then
            settingsHandle:Refresh()
        end
    end

    local function refreshExtDialog()
        if extDialogHandle then
            extDialogHandle:Refresh()
        end
    end

    extensionsStore.Changed:Connect(function()
        refreshSettings()
    end)

    extLinkEditor.Changed:Connect(function()
        refreshExtDialog()
    end)

    local function reloadExtCandidates()
        local choice = extensions.findChoice(extDialogState.choices, extDialogState.selectedKey)
        if choice == nil then
            extDialogState.candidates = {}
            extDialogState.candidatesLoaded = true
            return
        end
        local bAny = Browser :: any
        local list = if type(bAny.ExtBrowserList) == "function"
            then bAny:ExtBrowserList(choice.sourceId, choice.profileId, storedExtensions())
            else {}
        extDialogState.candidates = if type(list) == "table" then list :: any else {}
        extDialogState.candidatesLoaded = true
    end

    local function openAddExtensionDialog()
        -- The browsers and profiles of the page's import rows, the one used last first, the
        -- Chromium ones only (`settings.tsx:90`, `:474`; `profile/sources.ts:149-165`): the page
        -- that opens the dialog already lists them. A profile without extensions says so.
        local sources: { extensions.BrowserSource } = {}
        for _, browser in listImportSources() do
            if browser.engine == "chromium" then
                table.insert(sources, { id = browser.id, name = browser.name, profiles = browser.profiles })
            end
        end
        local choices = extensions.flattenChoices(sources)
        extDialogState = {
            busy = nil,
            installingId = nil,
            choices = choices,
            selectedKey = if #choices > 0 then extensions.choiceKey(choices[1]) else nil,
            candidates = {},
            candidatesLoaded = false,
            review = nil,
            showAllSites = false,
            approving = false,
            message = nil,
        }
        extLinkEditor:SetText("")
        reloadExtCandidates()
        Dialog:Open("add-extension")
        extLinkEditor:Focus()
    end

    local function handleStageResult(res: any)
        extDialogState.busy = nil
        extDialogState.installingId = nil
        if type(res) == "table" and res.type == "review" and type(res.review) == "table" then
            extDialogState.review = res.review :: any
            extDialogState.showAllSites = false
            extDialogState.message = nil
        elseif type(res) == "table" and res.type == "cancelled" then
            -- User dismissed folder dialog or superseded an older stage.
        elseif type(res) == "table" and res.type == "failed" then
            extDialogState.message = {
                text = extensions.failureMessage(I18n, res.reason),
                error = true,
                compatibility = nil,
            }
        else
            extDialogState.message = {
                text = I18n:T("settings.failed.install"),
                error = true,
                compatibility = nil,
            }
        end
        refreshExtDialog()
    end

    local function startStoreInstall()
        if extDialogState.busy ~= nil or extDialogState.approving then
            return
        end
        local url = string.match(extLinkEditor:Text(), "^%s*(.-)%s*$") or ""
        if url == "" then
            return
        end
        extDialogState.busy = "install-store"
        extDialogState.installingId = nil
        extDialogState.message = nil
        extLinkEditor:SetText("")
        refreshExtDialog()
        task.spawn(function()
            local bAny = Browser :: any
            local ok, res = pcall(function()
                return bAny:ExtStageStoreAsync(url, storedExtensions())
            end)
            handleStageResult(if ok then res else nil)
        end)
    end

    local function startBrowserInstall(sourceId: string, profileId: string, extId: string)
        if extDialogState.busy ~= nil or extDialogState.approving then
            return
        end
        extDialogState.busy = "install-browser"
        extDialogState.installingId = extId
        extDialogState.message = nil
        refreshExtDialog()
        task.spawn(function()
            local bAny = Browser :: any
            local ok, res = pcall(function()
                return bAny:ExtStageBrowserAsync(sourceId, profileId, extId, storedExtensions())
            end)
            handleStageResult(if ok then res else nil)
        end)
    end

    local function startFolderInstall()
        if extDialogState.busy ~= nil or extDialogState.approving then
            return
        end
        extDialogState.busy = "install-folder"
        extDialogState.installingId = nil
        extDialogState.message = nil
        refreshExtDialog()
        local pickTitle = I18n:T("settings.extensions.folder.pick")
        task.spawn(function()
            local bAny = Browser :: any
            local ok, res = pcall(function()
                return bAny:ExtStageFolderAsync(pickTitle, storedExtensions())
            end)
            handleStageResult(if ok then res else nil)
        end)
    end

    local function approveExtReview(token: string)
        if extDialogState.approving then
            return
        end
        extDialogState.approving = true
        refreshExtDialog()
        task.spawn(function()
            local bAny = Browser :: any
            local ok, res = pcall(function()
                return bAny:ExtApproveAsync(token, storedExtensions())
            end)
            extDialogState.approving = false
            extDialogState.review = nil
            extDialogState.showAllSites = false
            if ok and type(res) == "table" and res.type == "installed" and type(res.records) == "table" then
                extensionsStore:Set(res.records)
                if type(bAny.ExtSync) == "function" then
                    bAny:ExtSync(res.records, true)
                end
                if type(res.extension) == "table" then
                    extDialogState.message = extensions.installedMessage(I18n, res.extension :: any)
                end
                reloadExtCandidates()
                refreshSettings()
            else
                local reason = if ok and type(res) == "table" then res.reason else nil
                extDialogState.message = {
                    text = extensions.failureMessage(I18n, reason),
                    error = true,
                    compatibility = nil,
                }
            end
            refreshExtDialog()
        end)
    end

    local function cancelExtReview()
        local bAny = Browser :: any
        if type(bAny.ExtCancelReview) == "function" then
            bAny:ExtCancelReview()
        end
        extDialogState.review = nil
        extDialogState.showAllSites = false
        refreshExtDialog()
    end

    ctx:AddPopover({
        id = "ext-browser-profile",
        title = I18n:T("settings.extensions.browser.select"),
        render = function(view)
            return extensions.renderProfileMenu(extDialogState.choices, extDialogState.selectedKey, view, function(key: string)
                Popover:Close()
                extDialogState.selectedKey = key
                reloadExtCandidates()
                refreshExtDialog()
            end)
        end,
    })

    extDialogHandle = ctx:AddDialog({
        id = "add-extension",
        title = I18n:T("settings.extensions.add.title"),
        size = "normal",
        render = function(view)
            return extensions.renderAddDialog({
                I18n = I18n,
                linkEditor = extLinkEditor,
                state = extDialogState,
                onAddLink = startStoreInstall,
                onSelectProfile = function(key: string)
                    extDialogState.selectedKey = key
                    reloadExtCandidates()
                    refreshExtDialog()
                end,
                onOpenProfileMenu = function()
                    Popover:Toggle({
                        id = "ext-browser-profile",
                        anchorKey = "ext-browser-profile-btn",
                        -- `settings.tsx:692` `placement="bottom-start"`; the select's gutter is 6
                        -- (`runtime/plugin/ui.tsx:208`).
                        side = "bottom",
                        align = "start",
                        gap = 6,
                    })
                end,
                onAddFromBrowser = startBrowserInstall,

                onPickFolder = startFolderInstall,
                onToggleSites = function()
                    extDialogState.showAllSites = not extDialogState.showAllSites
                    refreshExtDialog()
                end,
                onApprove = approveExtReview,
                onCancelReview = cancelExtReview,
                onClose = function()
                    Dialog:Close()
                end,
            }, view)
        end,
        onDismiss = function()
            Popover:Close()
            local bAny = Browser :: any
            if type(bAny.ExtCancelReview) == "function" then
                bAny:ExtCancelReview()
            end
            extDialogState.review = nil
            extDialogState.showAllSites = false
            extDialogState.approving = false
            extDialogState.busy = nil
            extDialogState.installingId = nil
            extDialogState.message = nil
            extLinkEditor:SetText("")
        end,
    })

    -- `main.ts:303-320` `clear`: the sign-out runs first (every cookie and the sites' stored data, in
    -- the engine) and is whole whatever the history store does; the history goes either way.
    local function runClearBrowsingData()
        if clearing then
            return
        end
        clearing = true
        clearNoticeText = nil
        refreshSettings()
        local bAny = Browser :: any
        local profOk = true
        if type(bAny.ClearProfileDataAsync) == "function" then
            local ok, res = pcall(function()
                return bAny:ClearProfileDataAsync()
            end)
            profOk = ok and res == true
        else
            local prof = profileContract:Get()
            if type(prof) == "table" and type(prof.clearData) == "function" then
                profOk = pcall(prof.clearData)
            end
        end
        local histOk = true
        if type(bAny.HistoryClear) == "function" then
            histOk = bAny:HistoryClear() == true
        end
        local keeper = recorder:Get()
        if keeper then
            keeper.clear()
        end
        clearing = false
        if not profOk then
            clearNoticeText = I18n:T("settings.failed.clear")
        elseif not histOk then
            clearNoticeText = I18n:T("settings.cleared.history")
        else
            clearNoticeText = I18n:T("settings.cleared")
        end
        refreshSettings()
        refresh()
    end

    ctx:AddDialog({
        id = "clear-data",
        title = I18n:T("settings.clear.confirm.title"),
        render = function(view)
            return settings.renderClearConfirm(I18n, view, function()
                Dialog:Close()
            end, function()
                Dialog:Close()
                runClearBrowsingData()
            end)
        end,
    })

    local function renderImport(view: View): ui.Node?
        local target = importTarget
        if target == nil then
            return nil
        end
        local chooser = siteChooserFor(target, importRunning)
        -- `site-chooser.tsx:358-373`: with a profile picker, which names the browser, the title does
        -- not repeat it; else the browser, and its profile as a badge when it has several.
        local title: settings.ImportTitle = if chooser ~= nil and chooser.profiles ~= nil
            then { text = I18n:T("settings.import.title") }
            else { text = I18n:T("signins.import", { browser = target.name }), badge = target.badge }
        return settings.renderImportDialog(I18n, view, title, importFailure, importBoxes, importRunning, function(which: string, on: boolean)
            importBoxes = if which == "signins"
                then { signins = on, history = importBoxes.history }
                else { signins = importBoxes.signins, history = on }
            refreshImportDialog()
        end, function()
            runDialogImport()
        end, function()
            Dialog:Close()
        end, chooser)
    end
    -- Lists and the choice last only while the dialog is open (`site-chooser.tsx:38-39`).
    local function dismissImport()
        importTarget = nil
        importOffer = nil
        importSite = nil
        importFailure = nil
        siteLists = {}
        siteFailures = {}
        siteLoading = {}
        siteChoice = { scope = "all", profile = "", chosen = {}, query = "", version = siteChoice.version + 1 }
        if siteSearch:Text() ~= "" then
            siteSearch:SetText("")
        end
    end
    -- One dialog, `tall` (`site-chooser.tsx:356`): its top edge stays where it is, and "Only sites I
    -- choose" grows it downward.
    importDialogHandle = ctx:AddDialog({
        id = "import-history",
        title = I18n:T("settings.import.title"),
        size = "tall",
        render = renderImport,
        onDismiss = dismissImport,
    })
    -- Opens the dialog for one browser profile: from Settings > Browser on "All sites", for that
    -- profile alone; from the pane's offer on "Only sites I choose" with the list shown, and every
    -- profile to pick from (`import-prompt.tsx:90-93` `reveal`).
    local function openImport(browserId: string, profileId: string, offer: { tab: BrowserTab? }?)
        local reveal = offer ~= nil
        for _, browser in importSources do
            if browser.id == browserId then
                for _, prof in browser.profiles do
                    if prof.id == profileId then
                        -- `choices.tsx` `choiceLabel`: the profile's name only for a browser with several.
                        importTarget = {
                            browser = browserId,
                            name = browser.name,
                            profile = profileId,
                            label = if #browser.profiles > 1 then `{browser.name} · {prof.name}` else browser.name,
                            badge = if #browser.profiles > 1 then prof.name else nil,
                        }
                        importOffer = offer
                        importSite = nil
                        importFailure = nil
                        importBoxes = { signins = true, history = true }
                        -- Each opening starts with nothing ticked and reads the lists again.
                        siteLists = {}
                        siteFailures = {}
                        siteLoading = {}
                        siteChoice = { scope = if reveal then "some" else "all", profile = "", chosen = {}, query = "", version = siteChoice.version + 1 }
                        if siteSearch:Text() ~= "" then
                            siteSearch:SetText("")
                        end
                        readSites(importTarget :: any)
                        Dialog:Open("import-history")
                        return
                    end
                end
            end
        end
    end

    -- A page's passkey notice (`panel.tsx:603-615`, `SiteChooserDialog`): the dialog opens on the first
    -- browser profile with the list shown and searched for the page's site, and moves on to the first
    -- profile that lists exactly that site; it brings that site's sign-in only. A notice that names
    -- no site opens the whole dialog, on "All sites" with both boxes (`panel.tsx:607`: no `site`).
    -- Without another browser, Settings > Browser says so.
    local function openImportSite(name: string?, choose: boolean, tab: string)
        -- The host lists the browsers off the UI thread, and the first ask in a run only starts that:
        -- the dialog waits for the listing as official's does (`site-chooser.tsx:758`), up to 5 s.
        local choices = signins.choices(listImportSources())
        for _ = 1, 50 do
            if choices[1] ~= nil then
                break
            end
            task.wait(0.1)
            choices = signins.choices(listImportSources())
        end
        local first = choices[1]
        if first == nil then
            Layout:OpenSettings("browser")
            return
        end
        setImportTarget(first)
        importOffer = nil
        importSite = { name = name, choose = choose, tab = tab, seeking = name ~= nil }
        importFailure = nil
        importBoxes = { signins = true, history = name == nil }
        siteLists = {}
        siteFailures = {}
        siteLoading = {}
        -- For a site: the list, with nothing chosen until the site is found where the person asked
        -- (`site-chooser.tsx:85`).
        siteChoice = { scope = if name ~= nil then "some" else "all", profile = profileKey(importTarget :: any), chosen = {}, query = name or "", version = siteChoice.version + 1 }
        if siteSearch:Text() ~= (name or "") then
            siteSearch:SetText(name or "")
        end
        readSites(importTarget :: any)
        Dialog:Open("import-history")
    end
    local importSiteSignal = (Browser :: any).ImportSite
    if importSiteSignal ~= nil then
        importSiteSignal:Connect(function(request: any)
            if type(request) == "table" and type(request.tab) == "string" then
                task.spawn(openImportSite, if type(request.site) == "string" then request.site else nil, request.choose == true, request.tab)
            end
        end)
    end

    local settingsPage = settings.create({
        I18n = I18n,
        preferences = preferences,
        updatePreferences = function(patch)
            updatePreferences(patch)
            refreshSettings()
        end,
        capabilities = settingsCapabilities,
        sitePermissions = sitePermissionsList,
        permissionRestartNote = function(): boolean
            local eng = Browser:Engine() :: any
            return eng ~= nil and eng.state == "ready" and eng.permissionSync ~= true
        end,
        setSitePermission = function(origin, permission, setting)
            setSitePermission(origin, permission, setting)
            refreshSettings()
        end,
        removeSitePermission = function(origin, permission)
            removeSitePermission(origin, permission)
            refreshSettings()
        end,
        clearNotice = function(): string?
            return clearNoticeText
        end,
        cookieCount = function(): number?
            local bAny = Browser :: any
            if type(bAny.CookieCount) ~= "function" then
                return nil
            end
            local ok, count = pcall(function()
                return bAny:CookieCount()
            end)
            return if ok and type(count) == "number" then count else nil
        end,
        clearing = function(): boolean
            return clearing
        end,
        requestClearBrowsingData = function()
            Dialog:Open("clear-data")
        end,
        importSources = listImportSources,
        importSourcesLoaded = function(): boolean
            return importSourcesLoaded
        end,
        importIcon = importIcon,
        importMessage = function(browserId: string, profileId: string): settings.ImportMessage?
            return importResults[`{browserId}\n{profileId}`]
        end,
        openImport = function(browserId: string, profileId: string)
            openImport(browserId, profileId, nil)
        end,
        Menu = Menu,
        renderImport = function(view)
            local prof = profileContract:Get()
            if type(prof) == "table" and type(prof.renderImport) == "function" then
                return prof.renderImport(view)
            end
            return nil
        end,
        renderExtensions = function(view)
            local ext = extContract:Get()
            if type(ext) == "table" and type(ext.renderSettings) == "function" then
                return ext.renderSettings(view)
            end
            if not extNativeAvailable() then
                return nil
            end
            local bAny = Browser :: any
            local rawList = if type(bAny.ExtList) == "function" then bAny:ExtList(storedExtensions()) else {}
            local list: { extensions.ProfileExtension } = if type(rawList) == "table" then rawList :: any else {}
            return extensions.renderSettingsSection({
                I18n = I18n,
                available = Browser:Available(),
                busy = extDialogState.busy ~= nil or extDialogState.approving,
                list = list,
                onOpenAdd = openAddExtensionDialog,
                onToggle = function(id: string, enabled: boolean)
                    local res = if type(bAny.ExtSetEnabled) == "function"
                        then bAny:ExtSetEnabled(id, enabled, storedExtensions())
                        else nil
                    if type(res) == "table" and res.ok == true and type(res.records) == "table" then
                        extensionsStore:Set(res.records)
                    else
                        Toast:Error(I18n:T("settings.failed.extension"))
                    end
                    refreshSettings()
                end,
                onRemove = function(id: string)
                    local res = if type(bAny.ExtRemove) == "function"
                        then bAny:ExtRemove(id, storedExtensions())
                        else nil
                    if type(res) == "table" and res.ok == true and type(res.records) == "table" then
                        extensionsStore:Set(res.records)
                    else
                        Toast:Error(I18n:T("settings.failed.extension"))
                    end
                    refreshSettings()
                end,
            }, view)
        end,
        remoteSignInsEnabled = function(): boolean?
            local bAny = Browser :: any
            if type(bAny.ShareSignIns) ~= "function" or bAny:ShareSignIns() == nil then
                return nil
            end
            return preferences().remoteSignIns
        end,
        setRemoteSignIns = function(enabled: boolean)
            -- Kept across restarts as official keeps `remote-sign-ins` (`profile/index.ts:118,657-661`).
            local bAny = Browser :: any
            updatePreferences({ remoteSignIns = enabled })
            bAny:SetShareSignIns(enabled)
            refreshSettings()
        end,
        pickCookieFile = function()
            if cookieFileBusy then
                return
            end
            cookieFileBusy = true
            refreshSettings()
            local ok, res = pcall(function()
                return (Browser :: any):ImportCookieFileAsync(I18n:T("settings.import.file.pick"))
            end)
            cookieFileBusy = false
            if ok and type(res) == "table" and res.cancelled == true then
                refreshSettings()
                return
            end
            cookieFileMessage = describeCookieFile(if ok and type(res) == "table" then res else nil)
            refreshSettings()
        end,
        cookieFileMessage = function(): settings.ImportMessage?
            return cookieFileMessage
        end,
        cookieFileBusy = function(): boolean
            return cookieFileBusy
        end,
    }, refreshSettings)

    -- The host re-sends it to every engine that starts with a cookie store (`#286`), so setting it
    -- once here carries the person's choice across restarts without starting the engine.
    local function syncRemoteSignIns()
        local bAny = Browser :: any
        if type(bAny.SetShareSignIns) == "function" then
            bAny:SetShareSignIns(preferences().remoteSignIns)
        end
    end
    if preferences().remoteSignIns then
        syncRemoteSignIns()
    end
    prefsStore.Changed:Connect(function()
        syncRemoteSignIns()
        refreshSettings()
    end)

    -- The pane's sign-in offer (`import-prompt.tsx`): one import at a time, shared by the bar and the
    -- card; what it said, and the confirmation that replaces it for 5 s (`CONFIRMATION_MS`).
    local signInRunning: signins.Choice? = nil
    local signInMessage: signins.Message? = nil
    local signInConfirmed: { choice: signins.Choice?, text: string }? = nil
    local signInConfirmToken = 0

    -- `jobs.ts:186-276` for an `import-browser` job: names the browser where the person has to act.
    local function describeBrowserImport(choice: signins.Choice, res: any, pages: number): (signins.Message, boolean)
        local browser = choice.sourceName
        if res == nil or type(res.error) == "string" then
            local reason = if res then res.error else nil
            if reason == "locked" then
                return { text = I18n:T("signins.failed.locked.named", { browser = browser }), error = true }, false
            elseif reason == "key" and App.Platform == "macos" then
                return { text = I18n:T("signins.failed.keychain", { browser = browser }), error = true }, false
            elseif reason == "key" or reason == "access" or reason == "read" or reason == "storage" then
                return { text = I18n:T(`signins.failed.{reason}`), error = true }, false
            end
            return { text = I18n:T("settings.failed.import"), error = true }, false
        end
        local imported = tonumber(res.imported) or 0
        local failed = tonumber(res.failed) or 0
        local protected = tonumber(res.protected) or 0
        local loss = if failed > 0 then "other" elseif protected > 0 then "protected" else nil
        if imported == 0 and loss == "protected" then
            return { text = I18n:T("signins.protected.named", { browser = browser }), error = true }, false
        end
        if imported == 0 and loss ~= nil and pages == 0 then
            return { text = I18n:T("signins.imported.none.from", { browser = browser }), error = true }, false
        end
        if loss ~= nil then
            return { text = I18n:T("signins.imported.partial.named", { browser = browser }), error = false }, imported > 0
        end
        if imported > 0 then
            local key = if pages > 0 then "signins.imported.history.from" else "signins.imported.from"
            return { text = I18n:T(key, { browser = browser }), error = false }, true
        end
        if pages > 0 then
            return { text = I18n:T("signins.imported.history.only.from", { browser = browser }), error = false }, true
        end
        return { text = I18n:T("signins.imported.none.from", { browser = browser }), error = true }, false
    end

    local function closeSignInConfirmation()
        signInConfirmed = nil
        signInMessage = nil
        refresh()
    end

    -- How the offer's import went: what it brought, in place of the offer for 5 s or until closed, or
    -- why it brought nothing, under the offer (`import-prompt.tsx:43-79`). Cookies that came in load
    -- the pane's page again (`panel.tsx:594-598`) and end the offer for good (`jobs.ts:114-115`).
    local function signInOutcome(choice: signins.Choice, message: signins.Message, confirmed: boolean, cookies: number, tab: BrowserTab?)
        if cookies > 0 then
            updatePreferences({ welcomed = true })
            if tab ~= nil and tab:Url() ~= "" and tab:Url() ~= "about:blank" then
                tab:Reload()
            end
        end
        if confirmed then
            signInConfirmed = { choice = choice, text = message.text }
            signInConfirmToken += 1
            local token = signInConfirmToken
            task.delay(5, function()
                if signInConfirmToken == token and signInConfirmed ~= nil then
                    closeSignInConfirmation()
                end
            end)
        else
            signInMessage = message
        end
    end

    -- The dialog's Import (`site-chooser.tsx:250-266`): the ticked parts, sign-ins then history, as one
    -- job. A clean import closes the dialog and its row says so; a partial or failed one stays open
    -- and says why (`:164-198`). Cookies that came in end the pane's offer (`jobs.ts:114-115`).
    runDialogImport = function()
        local target = importTarget
        local site = importSite
        -- Opened for a page's site (`single`), the import is that site's sign-in and no history
        -- (`site-chooser.tsx:259-261`).
        local single = site ~= nil and site.name ~= nil
        if target == nil or importRunning or not (single or importBoxes.signins or importBoxes.history) then
            return
        end
        local offer = importOffer
        local boxes = if single then { signins = true, history = false } else importBoxes
        -- Every site under "All sites", or with every one ticked; else the ticked ones, in the list's order.
        local chooser = siteChooserFor(target, false)
        if chooser and not chooser.canImport then
            return
        end
        local sites: { string }? = nil
        if chooser and not chooser.everything then
            local picked = {}
            for _, item in siteLists[profileKey(target)] or {} do
                if chooser.isChosen(item.site) then
                    table.insert(picked, item.site)
                end
            end
            sites = picked
        end
        importRunning = true
        importFailure = nil
        refreshImportDialog()
        -- The offer's buttons wait while its import runs (`import-prompt.tsx:40`).
        if offer then
            offerImporting = true
            signInMessage = nil
            refresh()
        end
        local bAny = Browser :: any
        local res: any = { imported = 0, failed = 0, protected = 0 }
        if boxes.signins then
            local ok, out = pcall(function()
                return bAny:ImportSignInsAsync(target.browser, target.profile, sites)
            end)
            res = if ok and type(out) == "table" then out else nil
        end
        -- `main.ts:285-296`: history comes only after sign-ins that did not fail, and never fails the
        -- import: history that cannot be read is no pages.
        local pages = 0
        local signedIn = res ~= nil and type(res.error) ~= "string"
        if boxes.history and signedIn then
            local historyImport = if type(bAny.HistoryImportProfileAsync) == "function" then "HistoryImportProfileAsync"
                elseif type(bAny.HistoryImportProfile) == "function" then "HistoryImportProfile"
                else nil
            if historyImport ~= nil then
                local hok, hres = pcall(function()
                    return bAny[historyImport](bAny, target.browser, target.profile)
                end)
                if hok and type(hres) == "table" and type(hres.imported) == "number" then
                    pages = hres.imported
                end
            end
        end
        importRunning = false
        offerImporting = false
        local choice = { sourceId = target.browser, sourceName = target.name, profileId = target.profile, label = target.label }
        local message, confirmed = describeBrowserImport(choice, res, pages)
        local cookies = if res and type(res.imported) == "number" then res.imported else 0
        local loss = res ~= nil and ((tonumber(res.failed) or 0) > 0 or (tonumber(res.protected) or 0) > 0)
        local clean = signedIn and not loss and (cookies > 0 or pages > 0)
        if offer then
            signInOutcome(choice, message, confirmed, cookies, offer.tab)
        elseif site ~= nil and cookies > 0 then
            -- The passkey notice goes and its page loads again, now signed in (`panel.tsx:609-615`).
            pcall(function()
                bAny:EndPasskeyNotice(site.tab)
            end)
            local tab = Browser:Get(site.tab)
            if tab ~= nil and tab:Url() ~= "" and tab:Url() ~= "about:blank" then
                tab:Reload()
            end
            if not preferences().welcomed then
                updatePreferences({ welcomed = true })
            end
        elseif cookies > 0 and not preferences().welcomed then
            updatePreferences({ welcomed = true })
        end
        -- The dialog may have closed while it ran; the row still says how it went.
        local open = importTarget == target
        if clean or not open then
            importResults[`{target.browser}\n{target.profile}`] = { text = message.text, error = message.error, live = true }
        end
        if clean and open then
            Dialog:Close()
        elseif open then
            importFailure = { text = message.text, error = message.error, live = true }
            refreshImportDialog()
        end
        refresh()
        refreshSettings()
    end

    -- Brings the profile's sign-ins and its history (`import-prompt.tsx:84-85`, both ticked); cookies
    -- that came in load the pane's page again (`panel.tsx:594-598`) and end the offer for good.
    local function runSignIns(choice: signins.Choice, tab: BrowserTab?)
        if signInRunning ~= nil then
            return
        end
        signInRunning = choice
        signInMessage = nil
        refresh()
        local bAny = Browser :: any
        local ok, res = pcall(function()
            return bAny:ImportSignInsAsync(choice.sourceId, choice.profileId)
        end)
        local pages = 0
        -- The history read runs off the UI thread (`HistoryImportProfileAsync`) where the host has it,
        -- and only after sign-ins that did not fail (`main.ts:285-296`).
        local historyImport = if type(bAny.HistoryImportProfileAsync) == "function" then "HistoryImportProfileAsync"
            elseif type(bAny.HistoryImportProfile) == "function" then "HistoryImportProfile"
            else nil
        if historyImport ~= nil and ok and type(res) == "table" and type(res.error) ~= "string" then
            local hok, hres = pcall(function()
                return bAny[historyImport](bAny, choice.sourceId, choice.profileId)
            end)
            if hok and type(hres) == "table" and type(hres.imported) == "number" then
                pages = hres.imported
            end
        end
        signInRunning = nil
        local result = if ok and type(res) == "table" then res else nil
        local message, confirmed = describeBrowserImport(choice, result, pages)
        local cookies = if result and type(result.imported) == "number" then result.imported else 0
        signInOutcome(choice, message, confirmed, cookies, tab)
        refresh()
        refreshSettings()
    end

    signInOffer = function(view: View, tab: BrowserTab, variant: string): ui.Node?
        local bAny = Browser :: any
        if type(bAny.ImportSignInsAsync) ~= "function" or not Browser:Available() then
            return nil
        end
        local offering = not preferences().welcomed
        if not offering and signInConfirmed == nil then
            return nil
        end
        return signins.render({
            I18n = I18n,
            Menu = Menu,
            choices = function(): { signins.Choice }
                return signins.choices(listImportSources())
            end,
            icon = importIcon,
            blocked = function(): boolean
                return signInRunning ~= nil or offerImporting
            end,
            importing = function(choice: signins.Choice): boolean
                return signInRunning ~= nil and signInRunning.sourceId == choice.sourceId and signInRunning.profileId == choice.profileId
            end,
            message = function(): signins.Message?
                return signInMessage
            end,
            confirmed = function()
                return signInConfirmed
            end,
            run = function(choice: signins.Choice)
                runSignIns(choice, tab)
            end,
            choose = function(choice: signins.Choice)
                openImport(choice.sourceId, choice.profileId, { tab = tab })
            end,
            decline = function()
                updatePreferences({ welcomed = true })
                refresh()
            end,
            more = function()
                Layout:OpenSettings("browser")
            end,
            close = closeSignInConfirmation,
        }, view, variant :: any, offering)
    end

    settingsHandle = ctx:AddSettingsPage({
        id = "browser",
        icon = "globe",
        title = I18n:T("settings.title"),
        entries = settings.entries(I18n, settingsCapabilities()),
        render = function(view, props)
            -- Official's jobs live with the page (`settings.tsx:54`): a visit after leaving it shows
            -- the count again, not the last clear's outcome. A page has no "left" signal, so a render
            -- two seconds or more after the last one counts as a new visit (`os.time`: reading
            -- `System:Now` would draw the page again on every tick).
            local now = os.time()
            if clearNoticeDrawn and settingsDrawnAt ~= nil and now - settingsDrawnAt >= 2 and not clearing then
                clearNoticeText = nil
            end
            settingsDrawnAt = now
            clearNoticeDrawn = clearNoticeText ~= nil
            return settingsPage.render(view, props)
        end,
    })

    mcp.add(ctx, Browser, Layout)
end)
