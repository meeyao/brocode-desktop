--!strict
-- What the Add WSL server dialog shows (Desktop's `model.ts`, `row-view.ts`), from the app half's
-- state and the dialog's own: which distros it lists and how each reads, the primary button,
-- the catalog of distros to install, the probe the dialog needs next, and a configured
-- distro's row in Settings › Servers.

local model = {}

export type Text = { key: string, params: { [string]: string }? }
export type Status = { label: Text, tone: "success" | "warning" | "muted" }
export type Primary = {
    variant: "neutral" | "contrast",
    label: Text,
    disabled: boolean,
    action: ("install-opencode" | "add")?,
    loading: boolean,
    width: number?,
}
export type Plan = { kind: "auto", key: string, action: "probeRuntime" | "refreshDistros" } | { kind: "addable", key: string, distros: { string } }

local function hidden(name: string): boolean
    local n = string.lower(name)
    return n == "docker-desktop" or n == "docker-desktop-data"
end

local function hasServer(state: any, distro: string): boolean
    for _, s in state.servers do
        if s.config.distro == distro then
            return true
        end
    end
    return false
end

-- Jobs the dialog waits for: WSL's, and those of distros without a server (an added server's
-- jobs belong to its row).
local function dialogJobRunning(state: any): boolean
    if state.hostJob == "busy" then
        return true
    end
    for distro in state.distroJobs do
        if not hasServer(state, distro) then
            return true
        end
    end
    return false
end

local function installedOf(state: any, name: string): any
    for _, i in state.installed do
        if i.name == name then
            return i
        end
    end
    return nil
end

local function ready(state: any, name: string): boolean
    local installed, probe = installedOf(state, name), state.distroProbes[name]
    if not installed or not probe or installed.version == 1 then
        return false
    end
    return probe.canExecute and probe.hasBash and probe.hasCurl
end

local function selectedDistro(selected: string?, visible: { any }, addable: { any }): string?
    if selected then
        for _, i in addable do
            if i.name == selected and i.version ~= 1 then
                return selected
            end
        end
    end
    for _, d in visible do
        if d.isDefault and d.version ~= 1 then
            for _, a in addable do
                if a.name == d.name then
                    return d.name
                end
            end
        end
    end
    for _, a in addable do
        if a.version ~= 1 then
            return a.name
        end
    end
    return nil
end

local function runtimeState(state: any): string
    if not state then
        return "loading"
    elseif state.pendingRestart then
        return "pendingRestart"
    elseif not state.runtime then
        return "checking"
    elseif not state.runtime.available then
        return "unavailable"
    end
    return "ready"
end

local function checking(): Status
    return { label = { key = "onboarding.distroStatus.checking" }, tone = "muted" }
end

local function status(state: any, name: string, probingAddable: boolean): Status?
    if not state then
        return nil
    end
    local installed = installedOf(state, name)
    if installed and installed.version == 1 then
        return { label = { key = "onboarding.distroStatus.unsupported" }, tone = "muted" }
    end
    local probing = probingAddable or state.distroJobs[name] == "probing"
    local probe = state.distroProbes[name]
    if not probe then
        return if probing then checking() else nil
    end
    if not probe.canExecute then
        if not installed then
            return { label = { key = "onboarding.distroNotInstalled", params = { distro = name } }, tone = "warning" }
        end
        return { label = { key = "onboarding.openDistroOnce", params = { distro = name } }, tone = "warning" }
    end
    if not probe.hasBash or not probe.hasCurl then
        return { label = { key = "onboarding.distroStatus.missingTools" }, tone = "warning" }
    end
    local check = state.opencodeChecks[name]
    if not check then
        return if probing then checking() else nil
    end
    if not check.resolvedPath then
        return { label = { key = "onboarding.distroStatus.opencodeMissing" }, tone = "warning" }
    elseif check.error then
        return { label = { key = "onboarding.installOpencode" }, tone = "warning" }
    end
    return { label = { key = "onboarding.distroStatus.ready" }, tone = "success" }
end

local function settled(state: any, selected: string?): boolean
    if not state or not selected then
        return false
    end
    local installed = installedOf(state, selected)
    if (installed and installed.version == 1) or not state.distroProbes[selected] then
        return false
    end
    return not ready(state, selected) or state.opencodeChecks[selected] ~= nil
end

local function primary(state: any, selected: string?, check: any, busy: boolean, installingOpencode: boolean, adding: boolean, probingAddable: boolean): Primary
    local isReady = selected ~= nil and state ~= nil and ready(state, selected)
    local probingSelected = probingAddable and not settled(state, selected)
    local job = if selected and state then state.distroJobs[selected] else nil
    local probingOpencode = probingSelected or (isReady and (check == nil or job == "probing"))
    local installing = job == "updating" or installingOpencode
    if not isReady or probingOpencode then
        return {
            variant = "contrast",
            label = if probingSelected then { key = "onboarding.distroStatus.checking" } else { key = "server.add" },
            disabled = true,
            action = nil,
            loading = probingSelected,
            width = nil,
        }
    end
    local opencodeReady = check ~= nil and check.resolvedPath ~= nil and check.error == nil
    if not opencodeReady then
        return {
            variant = "neutral",
            label = if installing then { key = "onboarding.updatingOpencode" } else { key = "onboarding.installOpencode" },
            disabled = busy,
            action = "install-opencode",
            loading = installing,
            width = 129,
        }
    end
    return {
        variant = "contrast",
        label = if adding then { key = "onboarding.adding" } else { key = "server.add" },
        disabled = busy,
        action = "add",
        loading = adding,
        width = nil,
    }
end

local function installable(installed: { any }, online: { any }): { any }
    local names = {}
    for _, i in installed do
        names[i.name] = true
    end
    local versionedUbuntu = false
    for _, o in online do
        if string.match(o.name, "^Ubuntu%-%d") then
            versionedUbuntu = true
        end
    end
    local out = {}
    for _, o in online do
        if not names[o.name] and (o.name ~= "Ubuntu" or not versionedUbuntu) then
            table.insert(out, o)
        end
    end
    return out
end

-- `fuzzysort` over label and name: every query character in order, the tighter and earlier the
-- match the higher it ranks.
local function filter(distros: { any }, search: string): { any }
    local q = string.lower((string.match(search, "^%s*(.-)%s*$") or ""))
    if #q == 0 then
        return distros
    end
    local function score(text: string): number?
        local t = string.lower(text)
        local at, first, last = 1, nil :: number?, 0
        for c in string.gmatch(q, utf8.charpattern) do
            local found = string.find(t, c, at, true)
            if not found then
                return nil
            end
            first = first or found
            last = found
            at = found + #c
        end
        local f = first or 1
        return -(last - f) * 10 - f
    end
    local scored = {}
    for index, d in distros do
        local a, b = score(d.label), score(d.name)
        local s = if a and b then math.max(a, b) else a or b
        if s then
            table.insert(scored, { s = s, d = d, i = index })
        end
    end
    table.sort(scored, function(x, y)
        if x.s ~= y.s then
            return x.s > y.s
        end
        return x.i < y.i
    end)
    local out = {}
    for _, x in scored do
        table.insert(out, x.d)
    end
    return out
end

-- The dialog's view model (`addServerViewModel`). `dialog` is the dialog machine's state.
function model.build(state: any, dialog: any, search: string, probingAddable: boolean)
    local visibleInstalled, visibleOnline, addable = {}, {}, {}
    for _, i in (if state then state.installed else {}) do
        if not hidden(i.name) then
            table.insert(visibleInstalled, i)
        end
    end
    for _, o in (if state then state.online else {}) do
        if not hidden(o.name) then
            table.insert(visibleOnline, o)
        end
    end
    for _, i in visibleInstalled do
        if not (state and hasServer(state, i.name)) then
            table.insert(addable, i)
        end
    end
    local selected = selectedDistro(dialog.selectedDistro, visibleInstalled, addable)
    local check = if selected and state then state.opencodeChecks[selected] else nil
    local installableDistros = installable(visibleInstalled, visibleOnline)
    local filtered = filter(installableDistros, search)
    local target = dialog.catalogTarget
    local targetOk = false
    for _, d in filtered do
        if d.name == target then
            targetOk = true
        end
    end
    if not targetOk then
        target = if filtered[1] then filtered[1].name else nil
    end
    local busyDialog = dialog.status == "main-running" or dialog.status == "catalog-installing" or dialog.status == "completed"
    local adding = (dialog.status == "main-running" and dialog.op.kind == "add-server") or dialog.status == "completed"
    local installingOpencode = selected ~= nil and dialog.status == "main-running" and dialog.op.kind == "install-opencode" and dialog.op.distro == selected
    local working = (state ~= nil and dialogJobRunning(state)) or busyDialog
    local statuses = {}
    for _, i in addable do
        statuses[i.name] = status(state, i.name, probingAddable)
    end
    local installingCatalog = dialog.status == "catalog-installing"
    for _, j in (if state then state.distroJobs else {}) do
        if j == "installing" then
            installingCatalog = true
        end
    end
    return {
        view = if dialog.status == "catalog" or dialog.status == "catalog-installing" then "catalog" else "main",
        adding = adding,
        busy = working or probingAddable,
        runtimeState = runtimeState(state),
        visibleInstalled = visibleInstalled,
        addable = addable,
        selected = selected,
        statuses = statuses,
        primary = primary(state, selected, check, working, installingOpencode, adding, probingAddable),
        installable = installableDistros,
        filtered = filtered,
        catalogTarget = target,
        installingCatalog = installingCatalog,
    }
end

-- The probe the dialog needs next (`addServerProbePlan`).
function model.plan(state: any, view: string, busy: boolean, selected: string?, addable: { any }): Plan?
    if not state then
        return nil
    end
    if not busy and not state.pendingRestart then
        if not state.runtime then
            return { kind = "auto", key = "auto:runtime", action = "probeRuntime" }
        end
        if state.runtime.available and #state.installed == 0 and #state.online == 0 then
            return { kind = "auto", key = "auto:distros", action = "refreshDistros" }
        end
    end
    if not (state.runtime and state.runtime.available) or state.pendingRestart or view ~= "main" or busy or dialogJobRunning(state) then
        return nil
    end
    local ordered = {}
    for _, i in addable do
        if i.name == selected then
            table.insert(ordered, i)
        end
    end
    for _, i in addable do
        if i.name ~= selected then
            table.insert(ordered, i)
        end
    end
    local pending, distros = {}, {}
    for _, i in ordered do
        if i.version ~= 1 then
            table.insert(distros, i.name)
            if not state.distroProbes[i.name] then
                table.insert(pending, `distro:{i.name}`)
            elseif ready(state, i.name) and not state.opencodeChecks[i.name] then
                table.insert(pending, `opencode:{i.name}`)
            end
        end
    end
    if #pending == 0 then
        return nil
    end
    return { kind = "addable", key = "addable:" .. table.concat(pending, "|"), distros = distros }
end

-- A configured distro's row (`wslRowView`): its Brocode version, its install or update button,
-- whether Retry start and Remove are offered.
function model.row(state: any, distro: string)
    local job = if state then state.distroJobs[distro] else nil
    local check = if state then state.opencodeChecks[distro] else nil
    local action: string? = nil
    if check then
        if not check.resolvedPath then
            action = "onboarding.installOpencode"
        end
    end
    local runtime: string? = nil
    for _, s in (if state then state.servers else {}) do
        if s.config.distro == distro then
            runtime = s.runtime.kind
        end
    end
    local free = job == nil or job == "starting"
    return {
        version = if check then check.version else nil,
        button = if action then { label = if job == "updating" then "server.updating" else action, disabled = not free } else nil,
        retry = job == nil and (runtime == "failed" or runtime == "stopped"),
        removable = free,
        runtime = runtime,
    }
end

return model
