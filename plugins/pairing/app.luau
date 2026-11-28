--!strict
-- The Pairing plugin's app half (Desktop's `main.ts`): once per process. Remote access is a
-- setting of the background service, so it changes through `opencode service` as `opencode
-- pair --remote` changes it, one command at a time (`service.luau`). Keep screen active holds the
-- display awake (`Power:KeepAwake`) while the app runs, from launch when the remembered setting
-- is on. Windows reach it through the `pairing` contract: a request returns a call id, and its
-- outcome lands in `results` under it.

local plugin = require("@opencode/plugin")
local service = require("./service")

local RESULTS_KEPT = 64

return plugin.defineApp(function(ctx)
    local Process = ctx:GetService("Process")
    local Store = ctx:GetService("Store")
    local Contracts = ctx:GetService("Contracts")
    local Console = ctx:GetService("Console")
    local Power = ctx:GetService("Power")

    -- Whether the display is kept awake, remembered across launches. Earlier versions saved it
    -- as JSON text.
    local stored = Store:Value("keepScreenActive", false :: any)
    local lock: PowerLock? = nil
    local state = service.create()
    local results: { [string]: { value: boolean?, error: string? } } = {}
    local order: { string } = {}
    local calls = 0
    local provided: Contribution? = nil
    local dispatch: (event: service.Event) -> ()
    local request: (kind: string, payload: any) -> number

    local function publish()
        provided = Contracts:Provide("pairing", {
            screenActive = lock ~= nil,
            results = results,
            request = function(kind: string, payload: any): number
                return request(kind, payload)
            end,
        })
    end

    local function finish(call: number, value: boolean?, err: string?)
        local key = tostring(call)
        results[key] = { value = value, error = err }
        table.insert(order, key)
        while #order > RESULTS_KEPT do
            results[table.remove(order, 1) :: string] = nil
        end
    end

    -- Holds or releases the display; false when the system refused.
    local function hold(enabled: boolean): boolean
        if not enabled then
            if lock then
                lock:Release()
                lock = nil
            end
            return true
        end
        if lock then
            return true
        end
        local ok, held = pcall(function()
            return Power:KeepAwake("Keep screen active")
        end)
        if not ok then
            Console:Error(`[pairing] keep screen active failed: {held}`)
            return false
        end
        lock = held
        return true
    end

    local function run(effect: service.Effect)
        if effect.type == "resolve" then
            finish(effect.call, effect.value, nil)
        elseif effect.type == "reject" then
            finish(effect.call, nil, effect.message)
        else
            local attempt, args = effect.attempt, effect.args
            task.spawn(function()
                local all = { "service" }
                for _, a in args do
                    table.insert(all, a)
                end
                -- The desktop host resolves bare "opencode" to the bundled/staged CLI and its service
                -- environment, matching Desktop's `ctx.cli` (`packages/gui-plugins/src/pairing/main.ts:76-81`).
                local out = Process:RunAsync({ program = "opencode", args = all, timeout = effect.timeout })
                if out.ok then
                    return dispatch({ type = "succeeded", attempt = attempt, stdout = string.match(out.stdout, "^%s*(.-)%s*$") or "" })
                end
                local message = out.error or (if out.timedOut then "timed out" else `exit code {out.code}`)
                Console:Error(`[pairing] service command failed: {table.concat(args, " ")}: {message} {string.match(out.stderr, "^%s*(.-)%s*$")}`)
                dispatch({ type = "failed", attempt = attempt, message = message })
            end)
        end
    end

    local queue: { service.Event } = {}
    local draining = false
    dispatch = function(event: service.Event)
        table.insert(queue, event)
        if draining then
            return
        end
        draining = true
        while #queue > 0 do
            for _, effect in service.step(state, table.remove(queue, 1) :: service.Event) do
                run(effect)
            end
        end
        draining = false
        publish()
    end

    -- `remote` reads the setting; `setRemote` turns it on or off (`{enabled, restart}`);
    -- `setScreen` holds or releases the display (`{enabled}`) and remembers the choice.
    request = function(kind: string, payload: any): number
        calls += 1
        local call = calls
        if kind == "remote" then
            dispatch({ type = "requested", call = call, request = { kind = "get-remote" } })
        elseif kind == "setRemote" then
            dispatch({ type = "requested", call = call, request = { kind = "set-remote", enabled = payload.enabled == true, restart = payload.restart == true } })
        elseif kind == "setScreen" then
            local enabled = payload.enabled == true
            if hold(enabled) then
                stored:Set(enabled)
                finish(call, enabled, nil)
            else
                finish(call, nil, "Keep screen active failed")
            end
            publish()
        else
            finish(call, nil, `unknown request {kind}`)
            publish()
        end
        return call
    end

    local saved = stored:Get()
    if saved == true or saved == "true" then
        hold(true)
    end
    publish()

    ctx.Disposing:Connect(function()
        dispatch({ type = "disposed" })
        hold(false)
        if provided then
            provided:Withdraw()
        end
    end)
end)
