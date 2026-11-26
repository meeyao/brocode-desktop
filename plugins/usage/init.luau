--!strict
-- View usage (`composer/commands.tsx:90-130`, `usage/dialog.tsx`, `usage/machine.ts`): the
-- location's Brocode Go allowance and Console budgets, read from the server each time the
-- dialog opens. Limits read before show while the new read runs; a read that fails, or takes
-- longer than 15 s, offers Try again; a server not signed in to Console says so.

local plugin = require("@opencode/plugin")
local limits = require("@self/limits")
local machine = require("@self/machine")
local view = require("@self/view")

-- Canonical palette slot matching the former `commands::order::USAGE_VIEW`.
local PALETTE_ORDER = 560

return plugin.defineWindow(function(ctx)
    local I18n = ctx:GetService("I18n")
    local Composer = ctx:GetService("Composer")
    local Models = ctx:GetService("Models")
    local Sessions = ctx:GetService("Sessions")
    local Server = ctx:GetService("Server")
    local Servers = ctx:GetService("Servers")
    local Dialog = ctx:GetService("Dialog")
    local Commands = ctx:GetService("Commands")
    local System = ctx:GetService("System")
    local Theme = ctx:GetService("Theme")

    local changed = ctx:CreateSignal()
    local cache: { [string]: limits.Info } = {}
    local badgesPublished = false
    local openState: machine.OpenState? = nil
    local timeoutThread: thread? = nil
    local tickThread: thread? = nil
    local tickDueMs: number? = nil

    local function cacheKey(directory: string): string
        return `{Servers:Current() or ""}\n{directory}`
    end

    local function publishBadges()
        local info = cache[cacheKey(Composer:Directory())]
        if info == nil then
            if badgesPublished then
                badgesPublished = false
                Models:SetBadges({})
            end
            return
        end
        local badges: { ModelBadge } = {}
        for _, m in Models:List() do
            if not m.free then
                local served = m.providerId ~= "opencode-go"
                    and (m.providerId == "opencode" or m.integration == "opencode")
                local left = limits.modelLeft(info, m.providerId, m.modelId, served)
                if left ~= nil then
                    local reached = left <= 0
                    local label = if reached
                        then I18n:T("usage.budget.reached")
                        else I18n:T("usage.budget.left", { amount = view.money(I18n, left, false) })
                    table.insert(badges, {
                        providerId = m.providerId,
                        modelId = m.modelId,
                        label = label,
                        critical = reached,
                    })
                end
            end
        end
        if #badges > 0 or badgesPublished then
            badgesPublished = #badges > 0
            Models:SetBadges(badges)
        end
    end

    local function refreshForPicker()
        if not Server:IsLive() then
            return
        end
        local directory = Composer:Directory()
        local key = cacheKey(directory)
        task.spawn(function()
            local path = `/api/experimental/usage/limits?location%5Bdirectory%5D={Server:EncodeUri(directory)}`
            local res = Server:RequestAsync("GET", path)
            if not res.ok then
                return
            end
            local parsed = limits.parseInfo(res.json)
            if parsed ~= nil then
                cache[key] = parsed
            else
                cache[key] = nil
            end
            publishBadges()
        end)
    end

    local function cancelThread(th: thread?)
        if th and th ~= coroutine.running() and coroutine.status(th) == "suspended" then
            task.cancel(th)
        end
    end

    local function clearTimeout()
        cancelThread(timeoutThread)
        timeoutThread = nil
    end

    local function clearTicker()
        cancelThread(tickThread)
        tickThread = nil
        tickDueMs = nil
    end

    local function syncUsage(effect: machine.Effect)
        local current = openState
        if current == nil then
            return
        end
        local directory = current.directory
        local attempt = effect.attempt
        clearTimeout()

        -- Offline / fixture window: no live Console server to query, so show signed out at once.
        if not Server:IsLive() then
            machine.step(current, { type = "loaded", attempt = attempt, info = nil })
            changed:Fire()
            return
        end

        timeoutThread = task.delay(machine.TIMEOUT_S, function()
            timeoutThread = nil
            local cur = openState
            if cur == nil then
                return
            end
            local before = cur.load.status
            machine.step(cur, { type = "timed-out", attempt = attempt })
            if cur.load.status ~= before then
                changed:Fire()
            end
        end)

        local key = cacheKey(directory)
        task.spawn(function()
            local path = `/api/experimental/usage/limits?location%5Bdirectory%5D={Server:EncodeUri(directory)}`
            local res = Server:RequestAsync("GET", path)
            local parsed: limits.Info? = if res.ok then limits.parseInfo(res.json) else nil
            if parsed ~= nil then
                cache[key] = parsed
                publishBadges()
            elseif res.ok and limits.isSignedOutResponse(res) then
                cache[key] = nil
                publishBadges()
            end
            local cur = openState
            if cur == nil or cur.directory ~= directory then
                return
            end
            local event: machine.Event
            if res.ok and limits.isSignedOutResponse(res) then
                event = { type = "loaded", attempt = attempt, info = nil }
            elseif parsed ~= nil then
                event = { type = "loaded", attempt = attempt, info = parsed }
            else
                event = { type = "failed", attempt = attempt }
            end
            local before = cur.load
            machine.step(cur, event)
            if cur.load ~= before then
                clearTimeout()
                changed:Fire()
            end
        end)
    end

    local function closeState()
        openState = nil
        clearTimeout()
        clearTicker()
    end

    local function openDialog()
        local directory = Composer:Directory()
        local cm = Composer:Model()
        local model: limits.CurrentModel? = if cm ~= nil
            then { provider = cm.providerId, model = cm.modelId, name = cm.name }
            else nil
        local session = Sessions:Routed()
        local credits = session ~= nil and session.lastErrorStatus == 402
        local cached = cache[cacheKey(directory)]
        local state, effect = machine.open(directory, model, credits, cached)
        openState = state
        changed:Fire()
        Dialog:Open("main")
        syncUsage(effect)
    end

    local function retry()
        local cur = openState
        if cur == nil then
            openDialog()
            return
        end
        local effect = machine.step(cur, { type = "retry" })
        changed:Fire()
        if effect ~= nil then
            syncUsage(effect)
        end
    end

    -- Schedules a single wake at the next minute boundary where a Go countdown text changes.
    local function scheduleCountdown(go: limits.Go?, nowMs: number)
        if go == nil then
            clearTicker()
            return
        end
        local earliest: number? = nil
        for _, m in { go.rolling, go.weekly, go.monthly } do
            local diff = m.resetsAt - nowMs
            if diff > 60000 then
                local rem = diff % 60000
                -- Wait for the next minute boundary so open dialogs tick down without extra
                -- frame wakeups right after opening on an exact minute.
                local waitMs = if rem == 0 then 60000 else rem + 1
                local due = nowMs + waitMs
                if earliest == nil or due < earliest then
                    earliest = due
                end
            end
        end
        if earliest == nil then
            clearTicker()
            return
        end
        if tickThread ~= nil and tickDueMs == earliest then
            return
        end
        clearTicker()
        tickDueMs = earliest
        local delayS = math.max(0.01, (earliest - nowMs) / 1000)
        tickThread = task.delay(delayS, function()
            tickThread = nil
            tickDueMs = nil
            if Dialog:IsOpen("main") then
                changed:Fire()
            end
        end)
    end

    -- Resolves preview fixture data passed directly to `OpenDialog.data` (stories and scenes).
    local function previewState(data: any): (machine.Load?, limits.CurrentModel?, boolean)
        if type(data) ~= "table" or type(data.load) ~= "string" then
            return nil, nil, false
        end
        local model: limits.CurrentModel? = nil
        if
            type(data.model) == "table"
            and type(data.model.provider) == "string"
            and type(data.model.model) == "string"
            and type(data.model.name) == "string"
        then
            model = { provider = data.model.provider, model = data.model.model, name = data.model.name }
        end
        local credits = data.credits == true
        local kind = data.load
        if kind == "ready" then
            local info = limits.parseInfo(data.info)
            if info ~= nil then
                return { status = "ready", info = info }, model, credits
            end
            return { status = "failed" }, model, credits
        elseif kind == "signed-out" then
            return { status = "signed-out" }, model, credits
        elseif kind == "loading" then
            return { status = "loading", attempt = 0 }, model, credits
        else
            return { status = "failed" }, model, credits
        end
    end

    local commandReg: Contribution? = nil
    local dialogReg: Contribution? = nil

    local function register()
        if commandReg ~= nil then
            commandReg:Withdraw()
        end
        if dialogReg ~= nil then
            dialogReg:Withdraw()
        end

        commandReg = ctx:AddCommand({
            id = "view",
            title = I18n:T("command.usage.view"),
            description = I18n:T("command.usage.view.description"),
            order = PALETTE_ORDER,
            enabled = function()
                return Composer:Active()
            end,
            run = function()
                openDialog()
            end,
        })

        dialogReg = ctx:AddDialog({
            id = "main",
            title = I18n:T("usage.title"),
            size = "normal",
            layer = "standard",
            radius = 12,
            hideScrollbar = true,
            render = function(v, props)
                local load: machine.Load
                local model: limits.CurrentModel?
                local credits: boolean
                local pLoad, pModel, pCredits = previewState(props.data)
                if pLoad ~= nil then
                    load = pLoad
                    model = pModel
                    credits = pCredits
                elseif openState ~= nil then
                    load = openState.load
                    model = openState.model
                    credits = openState.credits
                else
                    return nil
                end

                local nowMs = System:Now()
                local go: limits.Go? = if load.status == "ready" or load.status == "refreshing" then load.info.go else nil
                scheduleCountdown(go, nowMs)

                local minuteBucket = if go ~= nil
                    then `{math.max(1, math.floor((go.rolling.resetsAt - nowMs) / 60000))}:{math.max(1, math.floor((go.weekly.resetsAt - nowMs) / 60000))}:{math.max(1, math.floor((go.monthly.resetsAt - nowMs) / 60000))}`
                    else ""
                local previewKey = if type(props.data) == "table" and props.data.key ~= nil
                    then tostring(props.data.key)
                    else ""
                local memoVersion = `{changed:Version()}:{previewKey}:{load.status}:{minuteBucket}:{Theme:Current().id}:{Theme:IsDark()}:{I18n:Locale().id}`

                local usageUrl = if load.status == "ready" or load.status == "refreshing"
                    then load.info.workspace.usageUrl
                    else ""

                return v:Memo("usage-dialog-body", memoVersion, function()
                    return view.render(I18n, load, model, credits, nowMs, {
                        onRetry = v:Handler(retry),
                        onSwitch = v:Handler(function()
                            closeState()
                            Dialog:Close()
                            Commands:Run("model")
                        end),
                        onDetails = v:Handler(function()
                            if usageUrl ~= "" then
                                System:OpenExternal(usageUrl)
                            end
                        end),
                    })
                end)
            end,
            onDismiss = closeState,
        })
    end

    register()
    I18n.Changed:Connect(function()
        register()
        publishBadges()
    end)
    Composer.PickerChanged:Connect(function(kind)
        if kind == "model" then
            publishBadges()
            refreshForPicker()
        end
    end)
    Models.Changed:Connect(publishBadges)
    Sessions.RoutedChanged:Connect(function()
        publishBadges()
    end)
    Server.Connected:Connect(function()
        publishBadges()
    end)

    ctx.Disposing:Connect(closeState)
end)
