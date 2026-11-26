--!strict
-- The WSL main side's state machine (Desktop's `machine.ts`), as plain data. WSL itself runs one
-- host job at a time (probe the runtime, list distros, install WSL); each distro runs one job
-- (probe, install, update Brocode, start or remove its server), and every completion carries
-- the attempt that started it, so a stale one changes nothing. `step` changes the state and
-- returns the effects; the controller (`app.luau`) runs them.

local machine = {}

export type RuntimeCheck = { available: boolean, version: string?, error: string? }
export type InstalledDistro = { name: string, version: number?, isDefault: boolean }
export type OnlineDistro = { name: string, label: string }
export type DistroProbe = { name: string, canExecute: boolean, hasBash: boolean, hasCurl: boolean, error: string? }
export type OpencodeCheck = {
    distro: string,
    resolvedPath: string?,
    version: string?,
    error: string?,
}
export type ServerConfig = { id: string, distro: string }

-- A configured distro's server while no job owns it.
export type Server = { kind: "stopped" } | { kind: "failed", message: string } | { kind: "ready", attempt: number, url: string, password: string }

-- The one job a distro runs. `updating`, `starting` and `removing` own its server (down meanwhile).
export type Distro =
    { status: "idle", server: Server }
    | { status: "probing", attempt: number, server: Server }
    | { status: "installing", attempt: number }
    | { status: "updating", attempt: number }
    | { status: "starting", attempt: number }
    | { status: "removing", attempt: number }

export type HostJob = "probe-runtime" | "refresh-distros" | "install-wsl"

export type State = {
    runtime: RuntimeCheck?,
    installed: { InstalledDistro },
    online: { OnlineDistro },
    distroProbes: { [string]: DistroProbe },
    opencodeChecks: { [string]: OpencodeCheck },
    pendingRestart: boolean,
    configured: { ServerConfig },
    -- WSL's own job, while it runs.
    host: number?,
    distros: { [string]: Distro },
}

export type Failure = { kind: "busy", distro: string } | { kind: "already-added", distro: string } | { kind: "error", message: string }

export type Event = { type: string, [string]: any }
export type Effect = { type: string, [string]: any }

function machine.create(configured: { ServerConfig }): State
    return {
        runtime = nil,
        installed = {},
        online = {},
        distroProbes = {},
        opencodeChecks = {},
        pendingRestart = false,
        configured = configured,
        host = nil,
        distros = {},
    }
end

local function idle(): Distro
    return { status = "idle", server = { kind = "stopped" } }
end

local function distroOf(s: State, distro: string): Distro
    return s.distros[distro] or idle()
end

local function isConfigured(s: State, distro: string): boolean
    for _, c in s.configured do
        if c.distro == distro then
            return true
        end
    end
    return false
end

local function needsCliCheck(s: State, distro: string): boolean
    local p = s.distroProbes[distro]
    return p ~= nil and p.canExecute and p.hasBash and p.hasCurl and s.opencodeChecks[distro] == nil
end

local function without(s: State, distro: string)
    local kept = {}
    for _, c in s.configured do
        if c.distro ~= distro then
            table.insert(kept, c)
        end
    end
    s.configured = kept
    s.distros[distro] = nil
    s.distroProbes[distro] = nil
    s.opencodeChecks[distro] = nil
end

local function configOf(s: State, id: string): string?
    for _, c in s.configured do
        if c.id == id then
            return c.distro
        end
    end
    return nil
end

local function persist(s: State): Effect
    local list = {}
    for _, c in s.configured do
        table.insert(list, { id = c.id, distro = c.distro })
    end
    return { type = "persist", servers = list }
end

local function resolve(attempt: number): Effect
    return { type = "resolve", attempt = attempt }
end

local function reject(attempt: number, failure: Failure): Effect
    return { type = "reject", attempt = attempt, failure = failure }
end

local function hostSettled(s: State, attempt: number, apply: () -> ()): { Effect }
    if s.host ~= attempt then
        return {}
    end
    apply()
    s.host = nil
    return { resolve(attempt) }
end

local function sameServer(a: Server, b: Server): boolean
    return a.kind == b.kind and (a :: any).attempt == (b :: any).attempt
end

-- One step: changes `s` and returns what to do.
function machine.step(s: State, e: Event): { Effect }
    local t = e.type
    -- A second host request while one runs joins it: the dialog asks again once WSL is idle.
    if t == "host" then
        if s.host ~= nil then
            return { resolve(e.attempt) }
        end
        s.host = e.attempt
        return { { type = "host", attempt = e.attempt, job = e.job } }
    elseif t == "runtime.probed" then
        return hostSettled(s, e.attempt, function()
            s.pendingRestart = s.pendingRestart and not e.runtime.available
            s.runtime = e.runtime
        end)
    elseif t == "runtime.installed" then
        return hostSettled(s, e.attempt, function()
            s.pendingRestart = not e.runtime.available
            s.runtime = e.runtime
        end)
    elseif t == "distros.listed" then
        return hostSettled(s, e.attempt, function()
            s.installed = e.installed
            s.online = e.online
        end)
    elseif t == "host.failed" then
        if s.host ~= e.attempt then
            return {}
        end
        s.host = nil
        return { reject(e.attempt, { kind = "error", message = e.message }) }
    elseif t == "probe" then
        -- The dialog probes what it shows; a distro already running a job is checked again once idle.
        local d = distroOf(s, e.distro)
        if d.status ~= "idle" then
            return { resolve(e.attempt) }
        end
        if s.distroProbes[e.distro] == nil then
            s.distros[e.distro] = { status = "probing", attempt = e.attempt, server = (d :: any).server }
            return { { type = "probe-distro", distro = e.distro, attempt = e.attempt } }
        end
        if needsCliCheck(s, e.distro) then
            s.distros[e.distro] = { status = "probing", attempt = e.attempt, server = (d :: any).server }
            return { { type = "inspect-cli", distro = e.distro, attempt = e.attempt } }
        end
        return { resolve(e.attempt) }
    elseif t == "distro.probed" then
        local d = distroOf(s, e.distro) :: any
        if d.status ~= "probing" or d.attempt ~= e.attempt then
            return {}
        end
        s.distroProbes[e.distro] = e.probe
        if needsCliCheck(s, e.distro) then
            return { { type = "inspect-cli", distro = e.distro, attempt = e.attempt } }
        end
        s.distros[e.distro] = { status = "idle", server = d.server }
        return { resolve(e.attempt) }
    elseif t == "cli.inspected" then
        local d = distroOf(s, e.distro) :: any
        if d.status ~= "probing" or d.attempt ~= e.attempt then
            return {}
        end
        s.opencodeChecks[e.distro] = e.check
        s.distros[e.distro] = { status = "idle", server = d.server }
        return { resolve(e.attempt) }
    elseif t == "install-distro" then
        local d = distroOf(s, e.distro)
        if d.status == "installing" then
            return { resolve(e.attempt) }
        end
        if isConfigured(s, e.distro) then
            return { reject(e.attempt, { kind = "already-added", distro = e.distro }) }
        end
        if d.status ~= "idle" then
            return { reject(e.attempt, { kind = "busy", distro = e.distro }) }
        end
        s.distros[e.distro] = { status = "installing", attempt = e.attempt }
        return { { type = "install-distro", distro = e.distro, attempt = e.attempt } }
    elseif t == "distro.installed" then
        local d = distroOf(s, e.distro) :: any
        if d.status ~= "installing" or d.attempt ~= e.attempt then
            return {}
        end
        s.installed = e.installed
        s.online = e.online
        s.distroProbes[e.distro] = e.probe
        s.distros[e.distro] = idle()
        return { resolve(e.attempt) }
    elseif t == "update" then
        -- Updating stops the distro's server first, even one still starting, and starts it again
        -- once the CLI is installed.
        local d = distroOf(s, e.distro) :: any
        if d.status == "updating" then
            return { resolve(e.attempt) }
        elseif d.status == "starting" then
            s.distros[e.distro] = { status = "updating", attempt = e.attempt }
            return { resolve(d.attempt), { type = "stop", distro = e.distro, attempt = e.attempt, server = d.attempt } }
        elseif d.status == "idle" and d.server.kind == "ready" then
            s.distros[e.distro] = { status = "updating", attempt = e.attempt }
            return { { type = "stop", distro = e.distro, attempt = e.attempt, server = d.server.attempt } }
        elseif d.status == "idle" then
            s.distros[e.distro] = { status = "updating", attempt = e.attempt }
            return { { type = "install-cli", distro = e.distro, attempt = e.attempt } }
        end
        return { reject(e.attempt, { kind = "busy", distro = e.distro }) }
    elseif t == "cli.installed" then
        -- The update's caller hears once the restarted server is up or failed.
        local d = distroOf(s, e.distro) :: any
        if d.status ~= "updating" or d.attempt ~= e.attempt then
            return {}
        end
        s.opencodeChecks[e.distro] = e.check
        if not isConfigured(s, e.distro) then
            s.distros[e.distro] = idle()
            return { resolve(e.attempt) }
        end
        s.distros[e.distro] = { status = "starting", attempt = e.attempt }
        return { { type = "spawn", distro = e.distro, attempt = e.attempt } }
    elseif t == "job.failed" then
        -- A failed update leaves the server stopped, so the row offers to start it again.
        local d = distroOf(s, e.distro) :: any
        local nextDistro: Distro
        if d.status == "probing" and d.attempt == e.attempt then
            nextDistro = { status = "idle", server = d.server }
        elseif (d.status == "installing" or d.status == "updating") and d.attempt == e.attempt then
            nextDistro = idle()
        else
            return {}
        end
        if e.check then
            s.opencodeChecks[e.distro] = e.check
        end
        s.distros[e.distro] = nextDistro
        return { reject(e.attempt, { kind = "error", message = e.message }) }
    elseif t == "add" then
        -- The caller hears once the server is saved; it starts in the background.
        local config: ServerConfig = e.config
        for _, c in s.configured do
            if c.id == config.id or c.distro == config.distro then
                return { reject(e.attempt, { kind = "already-added", distro = config.distro }) }
            end
        end
        if distroOf(s, config.distro).status ~= "idle" then
            return { reject(e.attempt, { kind = "busy", distro = config.distro }) }
        end
        table.insert(s.configured, config)
        s.distros[config.distro] = { status = "starting", attempt = e.attempt }
        return {
            persist(s),
            { type = "spawn", distro = config.distro, attempt = e.attempt },
            { type = "check-cli", distro = config.distro },
            resolve(e.attempt),
        }
    elseif t == "start" then
        -- A start while one is starting or an update will restart it joins that.
        local distro = configOf(s, e.id)
        if not distro then
            return { resolve(e.attempt) }
        end
        local d = distroOf(s, distro) :: any
        if d.status == "starting" or d.status == "updating" then
            return { resolve(e.attempt) }
        elseif d.status == "idle" and d.server.kind == "ready" then
            s.distros[distro] = { status = "starting", attempt = e.attempt }
            return { { type = "stop", distro = distro, attempt = e.attempt, server = d.server.attempt }, { type = "check-cli", distro = distro } }
        elseif d.status == "idle" then
            s.distros[distro] = { status = "starting", attempt = e.attempt }
            return { { type = "spawn", distro = distro, attempt = e.attempt }, { type = "check-cli", distro = distro } }
        end
        return { reject(e.attempt, { kind = "busy", distro = distro }) }
    elseif t == "remove" then
        local distro = configOf(s, e.id)
        if not distro then
            return { resolve(e.attempt) }
        end
        local d = distroOf(s, distro) :: any
        if d.status == "removing" then
            return { resolve(e.attempt) }
        elseif d.status == "starting" then
            s.distros[distro] = { status = "removing", attempt = e.attempt }
            return { resolve(d.attempt), { type = "stop", distro = distro, attempt = e.attempt, server = d.attempt } }
        elseif d.status == "idle" and d.server.kind == "ready" then
            s.distros[distro] = { status = "removing", attempt = e.attempt }
            return { { type = "stop", distro = distro, attempt = e.attempt, server = d.server.attempt } }
        elseif d.status == "idle" then
            without(s, distro)
            return { persist(s), resolve(e.attempt) }
        end
        return { reject(e.attempt, { kind = "busy", distro = distro }) }
    elseif t == "server.stopped" then
        local d = distroOf(s, e.distro) :: any
        if d.attempt ~= e.attempt then
            return {}
        elseif d.status == "starting" then
            return { { type = "spawn", distro = e.distro, attempt = e.attempt } }
        elseif d.status == "updating" then
            return { { type = "install-cli", distro = e.distro, attempt = e.attempt } }
        elseif d.status == "removing" then
            without(s, e.distro)
            return { persist(s), resolve(e.attempt) }
        end
        return {}
    elseif t == "server.spawned" then
        -- A server that comes up for a start nobody wants any more is stopped by the job that
        -- superseded it.
        local d = distroOf(s, e.distro) :: any
        if d.status ~= "starting" or d.attempt ~= e.attempt then
            return {}
        end
        s.distros[e.distro] = { status = "idle", server = { kind = "ready", attempt = e.attempt, url = e.url, password = e.password } }
        return {
            resolve(e.attempt),
            { type = "check-cli", distro = e.distro },
            { type = "log", error = false, message = `wsl sidecar ready {e.distro} {e.url}` },
        }
    elseif t == "server.failed" then
        -- A failed start answers its callers without an error: the row shows it and offers Retry.
        local d = distroOf(s, e.distro) :: any
        if d.status ~= "starting" or d.attempt ~= e.attempt then
            return {}
        end
        s.distros[e.distro] = { status = "idle", server = { kind = "failed", message = e.message } }
        return { resolve(e.attempt), { type = "log", error = true, message = `wsl sidecar failed to start {e.distro}: {e.message}` } }
    elseif t == "server.exited" then
        -- Only the live server's exit is a failure; a server a job stopped exits on purpose.
        local d = distroOf(s, e.distro) :: any
        local failed: Server = { kind = "failed", message = e.message }
        local live: Server = { kind = "ready", attempt = e.attempt, url = "", password = "" }
        if d.status == "idle" and sameServer(d.server, live) then
            s.distros[e.distro] = { status = "idle", server = failed }
        elseif d.status == "probing" and sameServer(d.server, live) then
            s.distros[e.distro] = { status = "probing", attempt = d.attempt, server = failed }
        else
            return {}
        end
        return { { type = "log", error = true, message = `wsl sidecar exited {e.distro}` } }
    elseif t == "cli.checked" then
        -- Background checks never overwrite what an update or removal decides.
        local d = distroOf(s, e.distro)
        if isConfigured(s, e.distro) and d.status ~= "updating" and d.status ~= "removing" then
            s.opencodeChecks[e.distro] = e.check
        end
        return {}
    end
    return {}
end

-- What windows see (`toServersState`).
function machine.published(s: State)
    local servers = {}
    for _, c in s.configured do
        local d = s.distros[c.distro]
        local runtime: { [string]: any }
        if d and d.status == "starting" then
            runtime = { kind = "starting" }
        elseif d and (d.status == "idle" or d.status == "probing") then
            local server = (d :: any).server
            if server.kind == "ready" then
                runtime = { kind = "ready", url = server.url, password = server.password }
            elseif server.kind == "failed" then
                runtime = { kind = "failed", message = server.message }
            else
                runtime = { kind = "stopped" }
            end
        else
            runtime = { kind = "stopped" }
        end
        table.insert(servers, { config = { id = c.id, distro = c.distro }, runtime = runtime })
    end
    local jobs = {}
    for name, d in s.distros do
        if d.status ~= "idle" then
            jobs[name] = d.status
        end
    end
    return {
        runtime = s.runtime,
        installed = s.installed,
        online = s.online,
        distroProbes = s.distroProbes,
        opencodeChecks = s.opencodeChecks,
        pendingRestart = s.pendingRestart,
        servers = servers,
        hostJob = if s.host ~= nil then "busy" else "idle",
        distroJobs = jobs,
    }
end

return machine
