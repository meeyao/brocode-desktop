--!strict
-- The WSL plugin's app half (Desktop's `main.ts`, `servers.ts`, `sidecar.ts`): once per process,
-- whatever the number of windows. It owns the machine (`machine.luau`), runs its effects as
-- `wsl.exe` processes and health checks, saves the configured distros, lists their servers as
-- the `wsl` server source, and shares its state and requests with the window halves as the
-- `wsl` contract. A window asks with `request`, which returns a call id; the call's outcome
-- shows in the contract's `results` once it settles.

local plugin = require("@opencode/plugin")
local machine = require("./machine")
local runtime = require("./runtime")
local decode = require("./decode")

type Sidecar = {
    distro: string,
    process: ProcessHandle?,
    url: string,
    password: string,
    ready: boolean,
    started: number,
    -- The last dozen lines it wrote, for a failure's message.
    tail: { string },
    -- Stops waiting for it to be gone: each reports `server.stopped` with its attempt.
    stops: { number },
    -- Why the host killed it (its health check timed out).
    failure: string?,
}

-- One request a window made, settled once all its machine attempts did.
type Call = { pending: number, error: string? }

local RESULTS_KEPT = 64

return plugin.defineApp(function(ctx)
    local Process = ctx:GetService("Process")
    local Http = ctx:GetService("Http")
    local Store = ctx:GetService("Store")
    local Contracts = ctx:GetService("Contracts")
    local Console = ctx:GetService("Console")
    local System = ctx:GetService("System")
    local Server = ctx:GetService("Server")
    local App = ctx:GetService("App")
    local I18n = ctx:GetService("I18n")

    -- `wsl.exe` from System32, or `--wsl-exe` (a missing one reads as WSL not installed).
    local program = App:Arg("wsl-exe") or "wsl.exe"
    local packaged = App.Channel ~= "local"

    -- The saved distros (`servers`), read leniently: a record without a distro is skipped, one
    -- without an id gets `wsl:<distro>`. Earlier versions saved them as JSON text.
    local saved = Store:Value("servers", { servers = {} } :: any)
    local function readSaved(): { machine.ServerConfig }
        local value = saved:Get()
        if type(value) == "string" then
            value = decode(value)
        end
        local out = {}
        local list = if type(value) == "table" and type(value.servers) == "table" then value.servers else {}
        for _, s in list do
            if type(s) == "table" and type(s.distro) == "string" and #s.distro > 0 then
                local id = if type(s.id) == "string" and #s.id > 0 then s.id else `wsl:{s.distro}`
                table.insert(out, { id = id, distro = s.distro })
            end
        end
        return out
    end

    local state = machine.create(readSaved())
    local issued = 0
    local calls: { [number]: Call } = {}
    local waiters: { [number]: number } = {}
    local results: { [string]: { error: string? } } = {}
    local order: { string } = {}
    local sidecars: { [number]: Sidecar } = {}

    local source = ctx:AddServerSource({ id = "wsl", title = I18n:T("server.label"), order = 1 })
    local provided: Contribution? = nil
    local dispatch: (event: machine.Event) -> ()
    local dispatchRequest: (kind: string, arg: any) -> number

    local function log(isError: boolean, message: string)
        if isError then
            Console:Error(`[wsl] {message}`)
        else
            Console:Log(`[wsl] {message}`)
        end
    end

    local function publish()
        local published = machine.published(state)
        local entries = {}
        for _, item in published.servers do
            local rt = item.runtime
            local kind = rt.kind
            table.insert(entries, {
                id = item.config.distro,
                name = item.config.distro,
                label = I18n:T("server.label"),
                state = if kind == "ready" then "ready" elseif kind == "starting" then "starting" elseif kind == "failed" then "failed" else "stopped",
                -- A distro joins the app's servers once its server is up; settings lists it before that.
                listed = kind == "ready",
                http = if kind == "ready" then { url = rt.url, password = rt.password } else nil,
            })
        end
        source:SetEntries(entries, true)
        provided = Contracts:Provide("wsl", {
            state = published,
            results = results,
            request = function(kind: string, arg: any): number
                return dispatchRequest(kind, arg)
            end,
        })
    end

    -- A window's request settles: its outcome goes into the contract, the oldest drop out.
    local function finish(id: number, err: string?)
        calls[id] = nil
        local key = tostring(id)
        results[key] = { error = err }
        table.insert(order, key)
        while #order > RESULTS_KEPT do
            results[table.remove(order, 1) :: string] = nil
        end
    end

    local function settle(attempt: number, err: string?)
        local id = waiters[attempt]
        if not id then
            return
        end
        waiters[attempt] = nil
        local call = calls[id]
        if not call then
            return
        end
        call.pending -= 1
        call.error = call.error or err
        if call.pending <= 0 then
            finish(id, call.error)
        end
    end

    local function failureText(f: machine.Failure): string
        if f.kind == "busy" then
            return I18n:T("error.busy", { distro = (f :: any).distro })
        elseif f.kind == "already-added" then
            return I18n:T("error.alreadyAdded", { distro = (f :: any).distro })
        end
        return (f :: any).message
    end

    local function nextAttempt(): number
        issued += 1
        return issued
    end

    -- Runs `wsl.exe args`: its output, or why it didn't run.
    local function wsl(args: { string }, timeout: number): (ProcessOutput?, string?)
        local out = Process:RunAsync({ program = program, args = args, timeout = timeout })
        if out.timedOut then
            return nil, I18n:T("error.commandTimeout", { command = program, args = table.concat(args, " "), timeout = timeout * 1000 })
        end
        if out.error then
            return nil, out.error
        end
        return out, nil
    end

    local function probeRuntime(): machine.RuntimeCheck
        local out, err = wsl({ "--version" }, runtime.TIMEOUT)
        if not out then
            return { available = false, error = err }
        end
        if out.ok then
            return { available = true, version = runtime.firstLine(out.stdout) }
        end
        return { available = false, error = runtime.failed(out, I18n:T("error.unavailable")) }
    end

    local function listed(args: { string }, fallback: string): (string?, string?)
        local out, err = wsl(args, runtime.TIMEOUT)
        if not out then
            return nil, err
        end
        local failed = runtime.failed(out, fallback)
        if failed then
            return nil, failed
        end
        return out.stdout, nil
    end

    -- `--list --verbose` and `--list --online`, both at once.
    local function listDistros(): (any, any, string?)
        local installed, online, err1, err2
        local done = 0
        local finished = ctx:CreateSignal()
        task.spawn(function()
            local text, err = listed({ "--list", "--verbose" }, I18n:T("error.listInstalled"))
            installed, err1 = if text then runtime.parseInstalled(text) else nil, err
            done += 1
            finished:Fire()
        end)
        task.spawn(function()
            local text, err = listed({ "--list", "--online" }, I18n:T("error.listOnline"))
            online, err2 = if text then runtime.parseOnline(text) else nil, err
            done += 1
            finished:Fire()
        end)
        while done < 2 do
            finished:Wait()
        end
        return installed, online, err1 or err2
    end

    -- Can the distro run commands, and does it have bash and curl.
    local function probeDistro(name: string): machine.DistroProbe
        local out, err = wsl(runtime.inDistro({ "/bin/true" }, name), runtime.TIMEOUT)
        local failure = if out then runtime.failed(out, I18n:T("error.executeDistro")) else err
        if failure then
            return { name = name, canExecute = false, hasBash = false, hasCurl = false, error = failure }
        end
        local function has(tool: string): boolean
            local o = wsl(runtime.sh(`command -v {tool} >/dev/null && printf yes || printf no`, name), runtime.TIMEOUT)
            return o ~= nil and o.ok and runtime.summarize(o.stdout) == "yes"
        end
        return { name = name, canExecute = true, hasBash = has("bash"), hasCurl = has("curl") }
    end

    -- Where the distro's Brocode is and what version it runs. Any version that runs will do.
    local function inspectCli(distro: string): (machine.OpencodeCheck?, string?)
        local out, err = wsl(runtime.sh(runtime.DISCOVER_SCRIPT, distro), runtime.TIMEOUT)
        if not out then
            return nil, err
        end
        local path = runtime.firstLine(out.stdout)
        if not path then
            return { distro = distro, error = I18n:T("error.opencodeMissing") }, nil
        end
        local v = wsl(runtime.sh(runtime.versionScript(runtime.quote(path)), distro), runtime.TIMEOUT)
        local version = if v then runtime.parseVersion(v.stdout) else nil
        if not version then
            return { distro = distro, resolvedPath = path, error = I18n:T("error.opencodeCannotRun") }, nil
        end
        return { distro = distro, resolvedPath = path, version = version }, nil
    end

    local function hostJob(attempt: number, job: machine.HostJob)
        if job == "probe-runtime" then
            return dispatch({ type = "runtime.probed", attempt = attempt, runtime = probeRuntime() })
        elseif job == "refresh-distros" then
            local installed, online, err = listDistros()
            if err then
                return dispatch({ type = "host.failed", attempt = attempt, message = err })
            end
            return dispatch({ type = "distros.listed", attempt = attempt, installed = installed, online = online })
        end
        -- WSL itself, elevated through PowerShell.
        local script = "$ErrorActionPreference = 'Stop'; $process = Start-Process -FilePath 'wsl.exe' -Verb RunAs -ArgumentList @('--install','--no-distribution') -Wait -PassThru; if ($null -ne $process.ExitCode) { exit $process.ExitCode }"
        local out = Process:RunAsync({
            program = "powershell.exe",
            args = { "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", script },
            timeout = runtime.INSTALL_TIMEOUT,
        })
        local failed = runtime.failed(out, I18n:T("error.installWsl"))
        if failed then
            return dispatch({ type = "host.failed", attempt = attempt, message = failed })
        end
        dispatch({ type = "runtime.installed", attempt = attempt, runtime = probeRuntime() })
    end

    local function installDistro(distro: string, attempt: number)
        local out, err = wsl({ "--install", "-d", distro, "--web-download", "--no-launch" }, runtime.INSTALL_TIMEOUT)
        local failed = if out then runtime.failed(out, I18n:T("error.installDistro", { distro = distro })) else err
        if failed then
            return dispatch({ type = "job.failed", distro = distro, attempt = attempt, message = failed })
        end
        local installed, online, listErr = listDistros()
        if listErr then
            return dispatch({ type = "job.failed", distro = distro, attempt = attempt, message = listErr })
        end
        dispatch({ type = "distro.installed", distro = distro, attempt = attempt, installed = installed, online = online, probe = probeDistro(distro) })
    end

    local function installCli(distro: string, attempt: number)
        local script = runtime.installScript(Server:Version() or "")
        if not script then
            return dispatch({ type = "job.failed", distro = distro, attempt = attempt, message = I18n:T("error.installOpencode") })
        end
        local out, err = wsl(runtime.inDistro({ "bash", "-lc", script }, distro), runtime.INSTALL_TIMEOUT)
        local failed = if out then runtime.failed(out, I18n:T("error.installOpencode")) else err
        if failed then
            return dispatch({ type = "job.failed", distro = distro, attempt = attempt, message = failed })
        end
        local check, checkErr = inspectCli(distro)
        if not check then
            return dispatch({ type = "job.failed", distro = distro, attempt = attempt, message = checkErr or "" })
        end
        if check.error == nil then
            return dispatch({ type = "cli.installed", distro = distro, attempt = attempt, check = check })
        end
        dispatch({ type = "job.failed", distro = distro, attempt = attempt, message = check.error, check = check })
    end

    -- ---- Sidecars (`sidecar.ts`) ------------------------------------------------------------

    -- Its stops, answered now that its server is gone.
    local function stopped(side: Sidecar)
        for _, stop in side.stops do
            dispatch({ type = "server.stopped", distro = side.distro, attempt = stop })
        end
    end

    local function failStart(attempt: number, message: string)
        local side = sidecars[attempt]
        if not side then
            return
        end
        sidecars[attempt] = nil
        if #side.stops > 0 then
            return stopped(side)
        end
        dispatch({ type = "server.failed", distro = side.distro, attempt = attempt, message = message })
    end

    -- Polls `/api/info` until it answers, the start is stopped, or 30 s pass.
    local function healthCheck(attempt: number)
        while true do
            local side = sidecars[attempt]
            if not side or #side.stops > 0 or not side.process or side.failure then
                return
            end
            local answer = Http:FetchAsync({ url = `{side.url}/api/info`, basicAuth = { user = "opencode", password = side.password }, timeout = 3 })
            side = sidecars[attempt]
            if not side or #side.stops > 0 or not side.process or side.failure then
                return
            end
            if answer.ok then
                side.ready = true
                return dispatch({ type = "server.spawned", distro = side.distro, attempt = attempt, url = side.url, password = side.password })
            end
            if System:Now() - side.started >= runtime.HEALTH_TIMEOUT * 1000 then
                side.failure = I18n:T("error.healthTimeout", { distro = side.distro, timeout = runtime.HEALTH_TIMEOUT * 1000 })
                if side.process then
                    side.process:Kill()
                end
                return
            end
            task.wait(0.1)
        end
    end

    local function onExit(attempt: number, exit: ProcessExit)
        local side = sidecars[attempt]
        if not side then
            return
        end
        sidecars[attempt] = nil
        if #side.stops > 0 then
            return stopped(side)
        end
        local code = if exit.code ~= nil then tostring(exit.code) else "null"
        if side.ready then
            return dispatch({ type = "server.exited", distro = side.distro, attempt = attempt, message = I18n:T("error.serverExited", { code = code, signal = "null" }) })
        end
        local message = side.failure
        if not message then
            local output = if #side.tail > 0 then "\n" .. table.concat(side.tail, "\n") else ""
            if exit.error then
                output ..= "\n" .. exit.error
            end
            message = I18n:T("error.serverExitedBeforeHealthy", { code = code, signal = "null", output = output })
        end
        dispatch({ type = "server.failed", distro = side.distro, attempt = attempt, message = message :: string })
    end

    -- A distro's Brocode server: find its CLI, pick a free port and a password, run `opencode
    -- serve` in the distro (its script on stdin, the Windows PATH left out), and poll
    -- `/api/info` until it answers.
    local function spawnSidecar(distro: string, attempt: number)
        log(false, `wsl sidecar starting {distro}`)
        local side: Sidecar = { distro = distro, url = "", password = "", ready = false, started = System:Now(), tail = {}, stops = {} }
        sidecars[attempt] = side
        local out, err = wsl(runtime.sh(runtime.DISCOVER_SCRIPT, distro), runtime.TIMEOUT)
        -- A stop that came before the process had started: it's gone now.
        if #side.stops > 0 then
            sidecars[attempt] = nil
            return stopped(side)
        end
        if not out then
            return failStart(attempt, err or "")
        end
        local path = runtime.firstLine(out.stdout)
        if not path then
            return failStart(attempt, I18n:T("error.opencodeNotInstalled", { distro = distro }))
        end
        local port = System:FreePort()
        if not port then
            return failStart(attempt, I18n:T("error.failedPort"))
        end
        side.password = System:RandomId()
        side.url = `http://127.0.0.1:{port}`
        local process = Process:Spawn({
            program = program,
            args = runtime.inDistro({ "bash", "-se" }, distro),
            stdin = runtime.sidecarScript(path, side.password, port, packaged),
        })
        side.process = process
        process.Output:Connect(function(stream, text)
            for line in string.gmatch(text, "[^\r\n]+") do
                if string.match(line, "%S") then
                    table.insert(side.tail, `[{stream}] {line}`)
                    if #side.tail > 12 then
                        table.remove(side.tail, 1)
                    end
                    log(false, `wsl sidecar {distro} {stream}: {line}`)
                end
            end
        end)
        process.Exited:Connect(function(exit)
            onExit(attempt, exit)
        end)
        healthCheck(attempt)
    end

    -- Stops start `server`'s server; `server.stopped` (with `attempt`) follows once it's gone.
    local function stopSidecar(distro: string, attempt: number, server: number)
        local side = sidecars[server]
        if not side then
            return dispatch({ type = "server.stopped", distro = distro, attempt = attempt })
        end
        table.insert(side.stops, attempt)
        if side.process then
            side.process:Kill()
        end
    end

    -- ---- Effects ------------------------------------------------------------------------------

    local function run(effect: machine.Effect)
        local t = effect.type
        if t == "resolve" then
            settle(effect.attempt, nil)
        elseif t == "reject" then
            settle(effect.attempt, failureText(effect.failure))
        elseif t == "persist" then
            saved:Set({ servers = effect.servers })
        elseif t == "log" then
            log(effect.error, effect.message)
        elseif t == "spawn" then
            task.spawn(spawnSidecar, effect.distro, effect.attempt)
        elseif t == "stop" then
            stopSidecar(effect.distro, effect.attempt, effect.server)
        elseif t == "check-cli" then
            task.spawn(function()
                local check, err = inspectCli(effect.distro)
                if check then
                    dispatch({ type = "cli.checked", distro = effect.distro, check = check })
                else
                    log(true, `wsl CLI check failed {effect.distro}: {err}`)
                end
            end)
        elseif t == "host" then
            task.spawn(hostJob, effect.attempt, effect.job)
        elseif t == "probe-distro" then
            task.spawn(function()
                dispatch({ type = "distro.probed", distro = effect.distro, attempt = effect.attempt, probe = probeDistro(effect.distro) })
            end)
        elseif t == "inspect-cli" then
            task.spawn(function()
                local check, err = inspectCli(effect.distro)
                if check then
                    dispatch({ type = "cli.inspected", distro = effect.distro, attempt = effect.attempt, check = check })
                else
                    dispatch({ type = "job.failed", distro = effect.distro, attempt = effect.attempt, message = err or "" })
                end
            end)
        elseif t == "install-distro" then
            task.spawn(installDistro, effect.distro, effect.attempt)
        elseif t == "install-cli" then
            task.spawn(installCli, effect.distro, effect.attempt)
        end
    end

    -- Effects of nested dispatches wait their turn, so one step's effects run in order.
    local queue: { machine.Event } = {}
    local draining = false
    dispatch = function(event: machine.Event)
        table.insert(queue, event)
        if draining then
            return
        end
        draining = true
        while #queue > 0 do
            local effects = machine.step(state, table.remove(queue, 1) :: machine.Event)
            for _, effect in effects do
                run(effect)
            end
        end
        draining = false
        publish()
    end

    -- A window's request: a call id, settled once the machine answers each of its attempts.
    local function call(events: { (attempt: number) -> machine.Event }): number
        local id = nextAttempt()
        if #events == 0 then
            finish(id, nil)
            publish()
            return id
        end
        calls[id] = { pending = #events }
        for _, make in events do
            local attempt = nextAttempt()
            waiters[attempt] = id
            dispatch(make(attempt))
        end
        return id
    end

    dispatchRequest = function(kind: string, arg: any): number
        if kind == "probeRuntime" or kind == "refreshDistros" or kind == "installWsl" then
            local job = if kind == "probeRuntime" then "probe-runtime" elseif kind == "refreshDistros" then "refresh-distros" else "install-wsl"
            return call({ function(a)
                return { type = "host", attempt = a, job = job }
            end })
        elseif kind == "installDistro" then
            return call({ function(a)
                return { type = "install-distro", distro = arg, attempt = a }
            end })
        elseif kind == "probeAddable" then
            local seen, events = {}, {}
            for _, d in (arg or {}) :: { string } do
                if not seen[d] then
                    seen[d] = true
                    table.insert(events, function(a)
                        return { type = "probe", distro = d, attempt = a }
                    end)
                end
            end
            return call(events)
        elseif kind == "installOpencode" then
            return call({ function(a)
                return { type = "update", distro = arg, attempt = a }
            end })
        elseif kind == "addServer" then
            return call({ function(a)
                return { type = "add", config = { id = `wsl:{arg}`, distro = arg }, attempt = a }
            end })
        elseif kind == "removeServer" then
            return call({ function(a)
                return { type = "remove", id = arg, attempt = a }
            end })
        elseif kind == "startServer" then
            return call({ function(a)
                return { type = "start", id = arg, attempt = a }
            end })
        end
        local id = nextAttempt()
        finish(id, `unknown request {kind}`)
        return id
    end

    publish()

    -- Starts at launch have no caller to answer. Only the configured servers start, as Desktop's
    -- `main.ts` does (`startConfiguredServers`): WSL itself and its distros are read when the add
    -- dialog plans them (`model.plan`, Desktop's `autoProbePlan`), so a launch starts no
    -- `wsl.exe` unless a WSL server is configured.
    task.spawn(function()
        for _, c in state.configured do
            dispatch({ type = "start", id = c.id, attempt = nextAttempt() })
        end
    end)

    ctx.Disposing:Connect(function()
        if provided then
            provided:Withdraw()
        end
    end)
end)
