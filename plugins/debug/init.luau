--!strict
-- Desktop's built-in Debug plugin (`packages/gui-plugins/src/debug`): the debug bar across the
-- window's foot (Toggle debug bar), Capture heap snapshots, and in local and Beta builds memory
-- diagnostics and the motion tools: COMPONENTS the components lab, SPEED and SLOW the app's
-- animation clock, KNOBS the tuning panel. Those builds also get CONSOLE, the plugin console with
-- its command bar (Roblox Studio's Output).

local plugin = require("@opencode/plugin")
local ui = require("@opencode/ui")
local machine = require("@self/machine")
local turn = require("@self/turn")
local console = require("@self/console")

type Bar = { visible: boolean, motion: boolean }

-- The speed cell steps through the common slow-downs, then back to full speed.
local function nextSpeed(speed: number): number
    for _, step in { 0.5, 0.25, 1 } do
        if step < speed - 1e-4 then
            return step
        end
    end
    return 1
end

-- Shortens a session ID for the bar cell (`shortSessionID` in `debug/bar.tsx`).
local function shortSessionID(id: string?): string?
    if id == nil or id == "" then
        return nil
    end
    if #id <= 12 then
        return id
    end
    return string.sub(id, 1, 8) .. "\u{2026}" .. string.sub(id, -4)
end

return plugin.defineWindow(function(ctx)
    local I18n = ctx:GetService("I18n")
    local App = ctx:GetService("App")
    local Store = ctx:GetService("Store")
    local Sessions = ctx:GetService("Sessions")
    local Server = ctx:GetService("Server")
    local System = ctx:GetService("System")
    local Diagnostics = ctx:GetService("Diagnostics")
    local Toast = ctx:GetService("Toast")
    local Theme = ctx:GetService("Theme")
    local Layout = ctx:GetService("Layout")
    -- Whether the bar, and the Knobs panel with it, are open; kept across launches.
    local bar = Store:Value("bar", { visible = false, motion = false }) :: StoreValue<Bar>
    -- Whether the bar's MEMLOG turned memory diagnostics on, kept across launches.
    local memory = Store:Value("memory", false) :: StoreValue<boolean>
    -- Local and Beta builds run memory diagnostics and have the motion tools; local builds (the
    -- dev server's) also the layout tools, DIR and FOCUS.
    local debugBuild = App.Channel == "local" or App.Channel == "beta"
    local devTools = App.Channel == "local"
    local heap = machine.idle
    local focus = machine.focusIdle
    local changed = ctx:CreateSignal()
    local state: console.State = { target = nil }
    local editor = ctx:CreateEditor()
    local consoleEnv: console.Env = {
        Console = ctx:GetService("Console"),
        Diagnostics = Diagnostics,
        Links = ctx:GetService("Links"),
        I18n = I18n,
        editor = editor,
        state = state,
        own = ctx.id,
        refresh = function() changed:Fire() end,
    }
    editor.Submitted:Connect(function(text)
        console.submit(consoleEnv, text)
    end)

    if debugBuild then
        Diagnostics:Set("memoryLog", memory:Get())
    end

    local function memoryLogActive(state: unknown): boolean
        return state == "on" or state == "env"
    end

    -- How the bar shows (`debugBarView`): running diagnostics pin it open, without its close
    -- button, so they're never silently on.
    local function view(): (boolean, boolean)
        local pinned = debugBuild and memoryLogActive(Diagnostics:Get("memoryLog"))
        local visible = pinned or bar:Get().visible
        return visible, visible and not pinned
    end

    -- The Knobs panel shows while the bar does.
    local function show(visible: boolean)
        local b = bar:Get()
        bar:Set({ visible = visible, motion = b.motion })
        if debugBuild and b.motion then
            Diagnostics:Set("knobs", visible)
        end
    end

    if debugBuild and view() and bar:Get().motion then
        Diagnostics:Set("knobs", true)
    end

    local function runHeap(event: machine.HeapEvent)
        local nextState, effects = machine.heap(heap, event)
        local moved = (heap.status == "idle") ~= (nextState.status == "idle")
        heap = nextState
        if moved then changed:Fire() end
        for _, effect in effects do
            if effect.type == "toast-loading" then
                Toast:Show({ variant = "loading", title = I18n:T("heap.loading"), persistent = true })
            elseif effect.type == "capture" then
                local attempt = effect.attempt
                task.spawn(function()
                    local saved = Diagnostics:HeapSnapshotAsync()
                    runHeap({ type = "settled", attempt = attempt, path = saved.path, error = saved.error })
                end)
            elseif effect.type == "toast-saved" or effect.type == "toast-partial" then
                local saved = effect.type == "toast-saved"
                Toast:Show({
                    variant = if saved then "success" else "error",
                    title = I18n:T(if saved then "heap.done" else "heap.partial"),
                    description = effect.path,
                    persistent = true,
                    actions = { { label = I18n:T("heap.reveal"), revealPath = effect.path } },
                })
            elseif effect.type == "toast-failed" then
                Toast:Show({ variant = "error", title = I18n:T("heap.failed"), persistent = true })
            end
        end
    end

    -- FOCUS: every control draws its focus ring (Desktop forces `:focus-visible` over CDP).
    local function runFocus(event: machine.FocusEvent)
        local nextState, effects = machine.focus(focus, event)
        local moved = machine.focusActive(focus) ~= machine.focusActive(nextState)
        focus = nextState
        if moved then changed:Fire() end
        for _, effect in effects do
            if effect.type == "set" then
                Diagnostics:Set("forceFocus", effect.enabled)
                local attempt = effect.attempt
                task.defer(function()
                    runFocus({ type = "set.succeeded", attempt = attempt })
                end)
            elseif effect.type == "reset" then
                Diagnostics:Set("forceFocus", false)
            elseif effect.type == "notify-failed" then
                Toast:Info(I18n:T("requestFailed"))
            end
        end
    end
    ctx.Disposing:Connect(function()
        runFocus({ type = "disposed" })
    end)

    ctx:AddCommand({
        id = "toggle",
        title = I18n:T("command.toggle"),
        group = I18n:T("category.view"),
        keywords = "dev bar developer tools diagnostics",
        -- Running diagnostics keep the bar open.
        enabled = function()
            local visible, closable = view()
            return closable or not visible
        end,
        run = function()
            show(not bar:Get().visible)
            return nil
        end,
    })

    ctx:AddCommand({
        id = "heap",
        title = I18n:T("command.heap"),
        description = I18n:T("command.heap.description"),
        group = I18n:T("category.settings"),
        -- One capture at a time.
        enabled = function()
            return heap.status == "idle"
        end,
        run = function()
            runHeap({ type = "requested" })
            return nil
        end,
    })

    -- The plugin console, as a side tab: local and Beta builds open it from the bar's CONSOLE.
    ctx:AddPanel({
        id = "console",
        title = I18n:T("console.title"),
        icon = "console",
        launcher = false,
        transient = true,
        render = function(v, props)
            return console.render(consoleEnv, v, props)
        end,
    })

    -- The latest turn of the routed session, read again as its messages change.
    local function latest(sessionId: string?): turn.Turn?
        if sessionId == nil or sessionId == "" then
            return nil
        end
        local session = Sessions:Get(sessionId)
        if session == nil then
            return nil
        end
        local res = Server:Resource(`/api/session/{sessionId}/message?limit=100&order=desc`, session.version)
        local json = res.json
        if type(json) ~= "table" or type(json.data) ~= "table" then
            return nil
        end
        local messages = {}
        for i = #json.data, 1, -1 do
            table.insert(messages, json.data[i])
        end
        return turn.latest(messages)
    end

    -- The bar (`bar.tsx`, inline): across the window's foot in the mono face, the close button,
    -- the latest turn's TIME and TPS, MEMLOG in debug builds, and at the far end the motion tools.
    ctx:AddSlot({
        id = "bar",
        at = "window.bottom",
        render = function(v, props)
            local visible, closable = view()
            -- `hidden md:block`: the bar shows from 768 points up.
            if not visible or Layout:Narrow() then
                return nil
            end
            local dark = Theme:IsDark()
            local sid: string? = if props.sessionId ~= nil and props.sessionId ~= "" then props.sessionId else nil
            local t = latest(sid) or { duration = nil, tps = nil }
            local logState = if debugBuild then Diagnostics:Get("memoryLog") else "off"
            local speed = if debugBuild then (Diagnostics:Get("speed") or 1) :: number else 1
            local slow = debugBuild and Diagnostics:Get("slow") == true
            local knobs = debugBuild and Diagnostics:Get("knobs") == true
            local direction = if devTools then (Diagnostics:Get("direction") or "ltr") :: string else "ltr"
            local focused = devTools and machine.focusActive(focus)
            local version = table.concat({
                tostring(sid),
                tostring(t.duration),
                tostring(t.tps),
                tostring(closable),
                tostring(logState),
                tostring(speed),
                tostring(slow),
                tostring(knobs),
                direction,
                tostring(focused),
                tostring(dark),
                Theme:Current().id,
                I18n:Locale().id,
            }, ":")
            return v:Memo("debug-bar", version, function()
                -- Desktop's mono ink: `#171717`, white at 93.6% in dark; names and dim values at 70%.
                local ink = ui.hex(if dark then "#ffffffef" else "#171717ff")
                local dim = ui.hex(if dark then "#ffffffa7" else "#171717b3")
                -- `surface-raised-base`: a pressed cell, and any cell under the pointer.
                local raised = ui.hex(if dark then "#ffffff0f" else "#00000008")
                local function name(words: string, mt: number): ui.Node
                    return ui.text(words, {
                        size = 10,
                        weight = 900 :: any,
                        lineHeight = 10,
                        tracking = 0.4,
                        mono = true,
                        color = dim,
                        noWrap = true,
                        mt = mt,
                        dir = "ltr",
                    })
                end
                -- A reading (`Cell`): its name and value on one baseline; the 10 pt name sits 1.4
                -- lower than the 12 pt value line so their baselines meet.
                local function cell(key: string, label: string, tip: string, value: string, isDim: boolean): ui.Node
                    return ui.col({ key = key, help = tip, helpSide = "top", minH = 20, px = 6, pt = 3, pb = 5 }, {
                        ui.row({ gap = 6 }, {
                            name(label, 1.4),
                            ui.text(value:upper(), {
                                size = 11,
                                weight = "bold",
                                lineHeight = 12,
                                mono = true,
                                tabular = true,
                                color = if isDim then dim else ink,
                                noWrap = true,
                                dir = "ltr",
                            }),
                        }),
                    })
                end
                -- A switch (`ToggleCell`): pressed while on, as a cell under the pointer.
                local function toggle(
                    key: string,
                    label: string,
                    tip: string,
                    value: string,
                    active: boolean,
                    onPress: ui.Handler
                ): ui.Node
                    return ui.col({
                        key = key,
                        role = "button",
                        label = `{label}: {value}`,
                        help = tip,
                        helpSide = "top",
                        minH = 20,
                        px = 6,
                        pt = 3.5,
                        pb = 5.5,
                        radius = 4,
                        hover = raised,
                        hoverTransition = "instant",
                        fill = if active then raised else nil,
                        onPress = onPress,
                    }, {
                        ui.row({ gap = 6 }, {
                            name(label, 0.9),
                            ui.text(
                                value:upper(),
                                { size = 11, weight = "bold", lineHeight = 11, mono = true, color = ink, noWrap = true, dir = "ltr" }
                            ),
                        }),
                    })
                end
                -- A copyable reading (`CopyCell` in `debug/bar.tsx`): preserves exact value casing
                -- while enabled; disabled renders uppercased and dimmed like `n/a`.
                local function copyCell(
                    key: string,
                    label: string,
                    tip: string,
                    value: string,
                    onPress: ui.Handler?
                ): ui.Node
                    local enabled = onPress ~= nil
                    return ui.col({
                        key = key,
                        role = "button",
                        label = `{label}: {value}`,
                        help = tip,
                        helpSide = "top",
                        disabled = not enabled,
                        minH = 20,
                        px = 6,
                        pt = 3,
                        pb = 5,
                        radius = 4,
                        hover = if enabled then raised else nil,
                        hoverTransition = "instant",
                        onPress = onPress,
                    }, {
                        ui.row({ gap = 6 }, {
                            name(label, 1.4),
                            ui.text(if enabled then value else value:upper(), {
                                size = 11,
                                weight = "bold",
                                lineHeight = 12,
                                mono = true,
                                tabular = true,
                                color = if enabled then ink else dim,
                                noWrap = true,
                                dir = "ltr",
                            }),
                        }),
                    })
                end
                local na = I18n:T("na")
                local cells: { ui.Node } = {}
                if closable then
                    table.insert(
                        cells,
                        ui.row({
                            key = "debug-bar-close",
                            role = "button",
                            label = I18n:T("close.label"),
                            help = I18n:T("close.tip"),
                            helpSide = "top",
                            w = 20,
                            h = 20,
                            radius = 4,
                            center = true,
                            hover = "surface-hover",
                            press = "surface-press",
                            onPress = v:Handler(function()
                                show(false)
                            end),
                        }, { ui.icon("close", { size = 16, color = "icon-weak" }) })
                    )
                end
                local sessionTip = if sid ~= nil then I18n:T("session.tip", { id = sid }) else I18n:T("session.none")
                local copySession = if sid ~= nil
                    then v:Handler(function()
                        local id = sid :: string
                        System:CopyText(id)
                        Toast:Show({ variant = "success", title = I18n:T("common.copied"), description = id })
                    end)
                    else nil
                table.insert(
                    cells,
                    copyCell(
                        "debug-bar-session",
                        I18n:T("session.label"),
                        sessionTip,
                        shortSessionID(sid) or na,
                        copySession
                    )
                )
                table.insert(
                    cells,
                    cell(
                        "debug-bar-time",
                        I18n:T("duration.label"),
                        I18n:T("duration.tip"),
                        t.duration or na,
                        t.duration == nil
                    )
                )
                local tps = if t.tps then string.format("%.1f", t.tps) else nil
                table.insert(
                    cells,
                    cell("debug-bar-tps", I18n:T("tps.label"), I18n:T("tps.tip"), tps or na, tps == nil)
                )
                -- The dev server's layout tools: the layout direction and forced focus styles.
                if devTools then
                    local rtl = direction == "rtl"
                    table.insert(
                        cells,
                        toggle(
                            "debug-bar-direction",
                            I18n:T("direction.label"),
                            I18n:T("direction.tip"),
                            I18n:T(`direction.{direction}`),
                            rtl,
                            v:Handler(function()
                                Diagnostics:Set("direction", if rtl then "ltr" else "rtl")
                            end)
                        )
                    )
                    table.insert(
                        cells,
                        toggle(
                            "debug-bar-focus",
                            I18n:T("focus.label"),
                            I18n:T("focus.tip"),
                            I18n:T(if focused then "focus.on" else "focus.off"),
                            focused,
                            v:Handler(function()
                                runFocus({ type = "toggle" })
                            end)
                        )
                    )
                end
                if debugBuild then
                    local env = logState == "env"
                    local value = if env
                        then I18n:T("memory.env")
                        else I18n:T(if logState == "on" then "focus.on" else "focus.off")
                    table.insert(
                        cells,
                        toggle(
                            "debug-bar-memlog",
                            I18n:T("memory.label"),
                            I18n:T(if env then "memory.tipEnv" else "memory.tip"),
                            value,
                            memoryLogActive(logState),
                            v:Handler(function()
                                -- The environment's choice lasts for the launch.
                                if env then
                                    return
                                end
                                local on = logState ~= "on"
                                memory:Set(on)
                                Diagnostics:Set("memoryLog", on)
                            end)
                        )
                    )
                    -- A link cell: its name alone.
                    local function link(key: string, label: string, tip: string, onPress: ui.Handler): ui.Node
                        return ui.row({
                            key = key,
                            role = "link",
                            label = label,
                            help = tip,
                            helpSide = "top",
                            minH = 20,
                            px = 6,
                            pt = 4,
                            pb = 6,
                            radius = 4,
                            hover = raised,
                            hoverTransition = "instant",
                            onPress = onPress,
                        }, { name(label, 0) })
                    end
                    local motion: { ui.Node } = {
                        link(
                            "debug-bar-console",
                            I18n:T("console.label"),
                            I18n:T("console.tip"),
                            v:Handler(function()
                                Layout:OpenPanel("console")
                            end)
                        ),
                        link(
                            "debug-bar-components",
                            I18n:T("lab.label"),
                            I18n:T("lab.tip"),
                            v:Handler(function()
                                Diagnostics:Set("components", true)
                            end)
                        ),
                        -- Plex Mono draws an x between figures as a multiplication sign.
                        toggle(
                            "debug-bar-speed",
                            I18n:T("speed.label"),
                            I18n:T("speed.tip"),
                            string.format("%.2f\u{d7}", speed),
                            math.abs(speed - 1) > 1e-4,
                            v:Handler(function()
                                Diagnostics:Set("speed", nextSpeed(speed))
                            end)
                        ),
                        toggle(
                            "debug-bar-slow",
                            I18n:T("slow.label"),
                            I18n:T("slow.tip"),
                            I18n:T(if slow then "focus.on" else "focus.off"),
                            slow,
                            v:Handler(function()
                                Diagnostics:Set("slow", not slow)
                            end)
                        ),
                        toggle(
                            "debug-bar-knobs",
                            I18n:T("motion.label"),
                            I18n:T("motion.tip"),
                            I18n:T(if knobs then "focus.on" else "focus.off"),
                            knobs,
                            v:Handler(function()
                                local b = bar:Get()
                                bar:Set({ visible = b.visible, motion = not knobs })
                                Diagnostics:Set("knobs", not knobs)
                            end)
                        ),
                    }
                    table.insert(cells, ui.spacer())
                    table.insert(cells, ui.row({ key = "debug-bar-motion", gap = 9, align = "center" }, motion))
                end
                return ui.col({
                    key = "debug-bar",
                    role = "group",
                    label = I18n:T("providerAriaLabel"),
                    w = "full",
                    mt = -6,
                    px = 12,
                    py = 4,
                    clip = true,
                }, {
                    ui.row({ key = "debug-bar-cells", w = "full", gap = 9, align = "center" }, cells),
                })
            end)
        end,
    })
end)
