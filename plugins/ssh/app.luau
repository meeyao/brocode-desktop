--!strict
-- The SSH plugin's app half (Desktop's `main.ts`, `controller.ts`, `bootstrap.ts`): once per
-- process. It owns the connections' machine (`machine.luau`) and runs its effects: a connect
-- parses the target, opens a sign-in listener (`Askpass`), resolves where `ssh` goes
-- (`ssh -G`), finds or installs and starts the host's Brocode service and reads its
-- registration, forwards a free local port to it through an `ssh -L` tunnel, and waits for
-- `/api/info` to answer. Every step is a process the host supervises; a stop kills them and
-- closes the listener. Windows reach it through the `ssh` contract.

local plugin = require("@opencode/plugin")
local machine = require("./machine")
local command = require("./command")
local decode = require("./decode")

-- How long a started service has to answer, from the tunnel's start or the last prompt.
local READY_SECONDS = 30
local RESULTS_KEPT = 64

-- One connect attempt's running parts.
type Conn = {
    id: string,
    listener: AskpassListener?,
    prompts: { [string]: AskpassPrompt },
    processes: { ProcessHandle },
    tunnel: ProcessHandle?,
    tail: string,
    deadline: number,
}

type Run = { ok: boolean, stdout: string, failure: command.Failure? }

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
    local Askpass = ctx:GetService("Askpass")

    local program = if App.Platform == "windows" then "ssh.exe" else "ssh"

    -- The saved hosts (`servers`); earlier versions saved them as JSON text.
    local saved = Store:Value("servers", {} :: any)
    local function readSaved(): { machine.Config }
        local value = saved:Get()
        if type(value) == "string" then
            value = decode(value)
        end
        local out = {}
        for _, c in (if type(value) == "table" then value else {}) do
            if type(c) == "table" and type(c.id) == "string" and type(c.target) == "string" then
                table.insert(out, { id = c.id, target = c.target, name = if type(c.name) == "string" then c.name else "" })
            end
        end
        return out
    end

    local state = machine.create(readSaved())
    local conns: { [number]: Conn } = {}
    local retries: { [number]: thread } = {}
    local results: { [string]: { error: string? } } = {}
    local order: { string } = {}
    local calls = 0
    local revision = 0
    local source = ctx:AddServerSource({ id = "ssh", title = I18n:T("label"), order = 2 })
    local provided: Contribution? = nil
    local dispatch: (event: machine.Event) -> ()
    local request: (kind: string, payload: any) -> number

    local function publish()
        local servers = machine.published(state)
        local entries = {}
        for _, item in servers do
            if item.saved then
                local stage = item.stage
                table.insert(entries, {
                    id = item.config.id,
                    name = command.name(item.config),
                    label = I18n:T("label"),
                    state = if stage == "connecting" then "starting" elseif stage == "authentication" then "auth" elseif stage == "ready" or stage == "failed" then stage else "stopped",
                    -- Every saved host is in the app's registry, up or not, as Desktop lists it
                    -- (renderer.tsx sets no `listed`): Home shows its lock, spinner or red dot.
                    listed = true,
                    -- Its tunnel keeps it reachable: a check of a new port shows the spinner.
                    managed = true,
                    http = item.http,
                })
            end
        end
        source:SetEntries(entries, true)
        revision += 1
        provided = Contracts:Provide("ssh", {
            servers = servers,
            revision = revision,
            results = results,
            request = function(kind: string, payload: any): number
                return request(kind, payload)
            end,
        })
    end

    local function finish(id: number, err: string?)
        local key = tostring(id)
        results[key] = { error = err }
        table.insert(order, key)
        while #order > RESULTS_KEPT do
            results[table.remove(order, 1) :: string] = nil
        end
    end

    local function stopConn(attempt: number)
        local conn = conns[attempt]
        if not conn then
            return
        end
        conns[attempt] = nil
        for _, p in conn.processes do
            p:Kill()
        end
        if conn.tunnel then
            conn.tunnel:Kill()
        end
        if conn.listener then
            conn.listener:Close()
        end
    end

    local function fail(attempt: number, failure: command.Failure)
        local conn = conns[attempt]
        if not conn then
            return
        end
        stopConn(attempt)
        Console:Error(`[ssh] connect failed ({failure.code}): {string.match(failure.detail, "^%s*(.-)%s*$")}`)
        dispatch({ type = "failed", id = conn.id, attempt = attempt, code = failure.code, detail = failure.detail })
    end

    -- Runs `ssh args` (with `stdin`) for `attempt`: its stdout, or why it failed. The attempt's
    -- sign-in listener answers its prompts. Its stderr goes to a file: Windows' OpenSSH never
    -- exits after logging to a stderr pipe.
    local function ssh(attempt: number, args: { string }, stdin: string, timeout: number): Run?
        local conn = conns[attempt]
        if not conn then
            return nil
        end
        local process = Process:Spawn({
            program = program,
            args = args,
            stdin = stdin,
            timeout = timeout,
            env = if conn.listener then conn.listener.Env else nil,
            stderrFile = true,
        })
        table.insert(conn.processes, process)
        local stdout, stderr = {}, {}
        process.Output:Connect(function(stream, text)
            table.insert(if stream == "stderr" then stderr else stdout, text)
        end)
        local exit = process:WaitAsync()
        -- A stopped attempt's steps end unheard.
        if not conns[attempt] then
            return nil
        end
        local out, err = table.concat(stdout), table.concat(stderr)
        if exit.error then
            return { ok = false, stdout = out, failure = command.failure("ssh-missing", exit.error) }
        elseif exit.timedOut then
            return { ok = false, stdout = out, failure = command.failure("connection", "timed out") }
        elseif exit.code == 0 then
            return { ok = true, stdout = out }
        end
        return { ok = false, stdout = out, failure = command.failure("connection", command.failureDetail(exit.code, out, err)) }
    end

    local function health(attempt: number, http: machine.Http)
        while true do
            local conn = conns[attempt]
            if not conn then
                return
            end
            local answer = Http:FetchAsync({ url = `{http.url}/api/info`, basicAuth = { user = "opencode", password = http.password }, timeout = 2 })
            conn = conns[attempt]
            if not conn then
                return
            end
            if answer.ok then
                -- Prompts only come while signing in: the listener closes once the tunnel is up.
                if conn.listener then
                    conn.listener:Close()
                    conn.listener = nil
                end
                return dispatch({ type = "ready", id = conn.id, attempt = attempt, http = http })
            end
            local authenticating = false
            for _, r in state.servers do
                local p = r.phase :: any
                if p.kind == "authenticating" and p.attempt == attempt then
                    authenticating = true
                end
            end
            if not authenticating and System:Now() - conn.deadline >= READY_SECONDS * 1000 then
                return fail(attempt, command.failure("service", conn.tail))
            end
            task.wait(0.1)
        end
    end

    -- Forwards a free local port to the service and waits for it to answer.
    local function tunnel(attempt: number, target: command.Target, url: string, password: string)
        local host, port = command.remoteAddress(url)
        if not host or not port then
            return fail(attempt, command.failure("service"))
        end
        local localPort = System:FreePort()
        if not localPort then
            return fail(attempt, command.failure("connection"))
        end
        local conn = conns[attempt]
        if not conn then
            return
        end
        local process = Process:Spawn({
            program = program,
            args = command.tunnelArgs(target, localPort, host, port),
            keepStdin = true,
            env = if conn.listener then conn.listener.Env else nil,
            stderrFile = true,
        })
        conn.tunnel = process
        process.Output:Connect(function(stream, text)
            local c = conns[attempt]
            if c and stream == "stderr" then
                c.tail = string.sub(c.tail .. text, -8192)
            end
        end)
        -- The tunnel closing fails the attempt, which the machine reads as a ready server that dropped.
        process.Exited:Connect(function(exit)
            local c = conns[attempt]
            if not c then
                return
            end
            local detail = string.match(c.tail, "^%s*(.-)%s*$") or ""
            if #detail == 0 then
                detail = '{"exitCode":' .. (if exit.code ~= nil then tostring(exit.code) else "null") .. "}"
            end
            fail(attempt, command.failure("connection", detail))
        end)
        conn.deadline = System:Now()
        health(attempt, { url = `http://127.0.0.1:{localPort}`, password = password })
    end

    -- `bootstrap.ts`: the host's registered service, whatever its version, or the CLI installed
    -- and its service started. The app's version only picks which CLI to install.
    local function bootstrap(attempt: number, target: command.Target)
        local version = Server:Version() or ""
        local args = command.sshArgs(target)
        table.insert(args, target.host)
        table.insert(args, "sh -l -s")
        local found = ssh(attempt, args, command.discoverScript(), 600)
        if not found then
            return
        end
        if not found.ok then
            return fail(attempt, found.failure :: command.Failure)
        end
        local r = command.parseRegistration(found.stdout)
        if r then
            return tunnel(attempt, target, r.url, r.password)
        end
        if not command.validVersion(version) then
            return fail(attempt, command.failure("unpublished", version))
        end
        local check = ssh(attempt, args, command.versionScript(`"{command.binaryPath(version)}"`), 600)
        if not check then
            return
        end
        if not (check.ok and command.parseVersion(check.stdout) ~= nil) then
            local probe = ssh(attempt, args, command.PROBE_SCRIPT, 600)
            if not probe then
                return
            end
            local platform: string? = nil
            if probe.ok then
                for line in string.gmatch(probe.stdout, "[^\r\n]+") do
                    platform = string.match(line, "^OPENCODE_REMOTE_TARGET=(.*)$") or platform
                end
            end
            local url = if platform then command.archiveUrl(platform, version) else nil
            if not url then
                return fail(attempt, command.failure("platform", platform))
            end
            local installed = ssh(attempt, args, command.installScript(version, url), 600)
            if not installed then
                return
            end
            if not installed.ok then
                -- The host couldn't fetch it: a version never published reads as such.
                local head = Http:FetchAsync({ url = url, method = "HEAD", timeout = 30 })
                return fail(attempt, command.failure(if head.status == 404 then "unpublished" else "install", (installed.failure :: command.Failure).detail))
            end
        end
        local started = ssh(attempt, args, command.startScript(version), 600)
        if not started then
            return
        end
        if not started.ok then
            return fail(attempt, started.failure :: command.Failure)
        end
        local registration = command.parseRegistration(started.stdout)
        if not registration then
            return fail(attempt, command.failure("service"))
        end
        tunnel(attempt, target, registration.url, registration.password)
    end

    local function connect(id: string, attempt: number, config: machine.Config)
        local conn: Conn = { id = id, prompts = {}, processes = {}, tail = "", deadline = System:Now() }
        conns[attempt] = conn
        local ok, listener = pcall(function()
            return Askpass:Listen()
        end)
        if ok then
            conn.listener = listener
            listener.Prompted:Connect(function(prompt)
                if not conns[attempt] then
                    return prompt:Respond(nil)
                end
                conn.prompts[prompt.Id] = prompt
                conn.deadline = System:Now()
                dispatch({ type = "prompt.received", id = id, attempt = attempt, prompt = { id = prompt.Id, text = prompt.Prompt, confirm = not prompt.Secret } })
            end)
            listener.Cleared:Connect(function(promptId)
                conn.prompts[promptId] = nil
                if conns[attempt] then
                    conn.deadline = System:Now()
                    dispatch({ type = "prompt.cleared", id = id, attempt = attempt, prompt = promptId })
                end
            end)
        end
        local target = command.parseTarget(config.target)
        if not target then
            return fail(attempt, command.failure("input"))
        end
        local args = command.sshArgs(target)
        table.insert(args, "-G")
        table.insert(args, target.host)
        local resolved = ssh(attempt, args, "", 10)
        if not resolved then
            return
        end
        if resolved.ok then
            local function field(key: string): string?
                for line in string.gmatch(resolved.stdout, "[^\r\n]+") do
                    local v = string.match(line, "^" .. key .. " (.*)$")
                    if v then
                        return v
                    end
                end
                return nil
            end
            local host = field("hostname")
            if host then
                dispatch({ type = "destination.resolved", id = id, attempt = attempt, destination = `{field("user") or ""}@{host}:{field("port") or "22"}` })
            end
        end
        bootstrap(attempt, target)
    end

    local function run(effect: machine.Effect)
        local t = effect.type
        if t == "save" then
            saved:Set(effect.configs)
        elseif t == "attempt.stop" then
            local r = retries[effect.attempt]
            if r then
                task.cancel(r)
                retries[effect.attempt] = nil
            end
            stopConn(effect.attempt)
        elseif t == "attempt.respond" then
            local conn = conns[effect.attempt]
            local prompt = conn and conn.prompts[effect.prompt]
            if prompt then
                prompt:Respond(effect.value)
            end
        elseif t == "retry.schedule" then
            retries[effect.attempt] = task.delay(machine.RETRY_DELAY, function()
                retries[effect.attempt] = nil
                dispatch({ type = "retry.due", id = effect.id, attempt = effect.attempt })
            end)
        elseif t == "attempt.start" then
            task.spawn(connect, effect.id, effect.attempt, effect.config)
        end
    end

    local queue: { machine.Event } = {}
    local draining = false
    dispatch = function(event: machine.Event)
        table.insert(queue, event)
        if draining then
            return
        end
        draining = true
        while #queue > 0 do
            for _, effect in machine.step(state, table.remove(queue, 1) :: machine.Event) do
                run(effect)
            end
        end
        draining = false
        publish()
    end

    -- A window's request: its outcome shows in the contract's `results` under the id returned.
    request = function(kind: string, p: any): number
        calls += 1
        local id = calls
        if kind == "start" then
            dispatch({ type = "start", config = { id = p.id, target = p.target, name = p.name or "" }, background = p.background == true })
        elseif kind == "respond" then
            dispatch({ type = "respond", id = p.id, prompt = p.prompt, value = p.value })
        elseif kind == "cancel" then
            dispatch({ type = "cancel", id = p.id })
        elseif kind == "forget" then
            dispatch({ type = "forget", id = p.id })
        else
            finish(id, `unknown request {kind}`)
            publish()
            return id
        end
        finish(id, nil)
        publish()
        return id
    end

    publish()

    -- The saved hosts restore in the background, once at launch.
    task.spawn(function()
        for _, r in state.servers do
            if r.saved and r.phase.kind == "disconnected" then
                dispatch({ type = "start", config = r.config, background = true })
            end
        end
    end)

    ctx.Disposing:Connect(function()
        if provided then
            provided:Withdraw()
        end
    end)
end)
