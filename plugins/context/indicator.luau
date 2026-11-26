--!strict
-- `SessionContextUsage` (indicator.tsx): the session header's button (its context ring, its token
-- count and, when anything costs, its spend), which opens the Context tab; in a narrow header
-- only the ring, which opens the usage menu; and the ring that leads the tab's title.

local ui = require("@opencode/ui")

local indicator = {}

export type Usage = {
    -- The last step's tokens, all kinds summed.
    total: number,
    -- That as a share of its model's context window, when the catalog gives the window.
    percent: number?,
    cost: number,
    showCost: boolean,
}

export type Env = {
    Sessions: SessionsService,
    Models: ModelsService,
    I18n: I18nService,
    Layout: LayoutService,
    Popover: PopoverService,
    Theme: ThemeService,
}

function indicator.usage(env: Env, s: Session): Usage
    local u = s.usage
    local total = if u then u.input + u.output + u.reasoning + u.cacheRead + u.cacheWrite else 0
    local percent: number? = nil
    if u and s.usageProviderId and s.usageModelId then
        local limit = env.Models:Get(s.usageProviderId, s.usageModelId).contextLimit
        if limit and limit > 0 then
            percent = math.round(total / limit * 100)
        end
    end
    -- Spend anywhere in the family shows the cost, even on a free model; an unknown model too,
    -- except Brocode Go at zero spend (`indicator.tsx`).
    local provider, model = s.providerId, s.modelId
    if model == nil or model == "" then
        provider, model = s.usageProviderId, s.usageModelId
    end
    local showCost = s.cost > 0
    if not showCost then
        if provider == "opencode-go" then
            showCost = false
        elseif model == nil or model == "" then
            showCost = true
        else
            local info = env.Models:Get(provider or "", model)
            showCost = info.modelName == nil or info.paid == true
        end
    end
    return { total = total, percent = percent, cost = s.cost, showCost = showCost }
end

-- The ring at `percent`: Desktop's ProgressCircle, the header's look or the tab's.
local function ring(percent: number, indicatorLook: boolean): ui.Node
    if indicatorLook then
        return ui.progressCircle({
            appearance = "indicator",
            size = 16,
            strokeWidth = 2,
            percentage = percent,
            overlay = "surface-press",
        })
    end
    return ui.progressCircle({ appearance = "compact", size = 16, percentage = percent })
end

local function themeKey(env: Env): string
    return `{env.Theme:Current().id}:{env.Theme:IsDark()}`
end

-- The `session.header` slot: the usage button, or in a narrow header the ring alone.
function indicator.header(env: Env, view: View, props: SlotProps): ui.Node?
    local sid = props.sessionId or ""
    local session = env.Sessions:Get(sid)
    if session == nil or session.id == "" then
        return nil
    end
    local usage = indicator.usage(env, session)
    local open = env.Popover:IsOpen("usage")
    local compact = props.compact == true
    local version = `{usage.total}:{usage.percent}:{usage.cost}:{usage.showCost}:{compact}:{open}:{themeKey(env)}`
    return view:Memo(`header-{sid}`, version, function()
        local function button(label: string, children: { ui.Node }, onPress: ui.Handler, pressed: boolean): ui.Node
            return ui.row({
                key = "context-usage",
                role = "button",
                h = 28,
                px = 6,
                gap = 8,
                align = "center",
                radius = 6,
                hover = "surface-hover",
                press = "surface-press",
                hoverTransition = "instant",
                fill = if pressed then "surface-press" else nil,
                label = label,
                help = label,
                onPress = onPress,
            }, children)
        end
        local circle = ring(usage.percent or 0, false)
        if compact then
            local label = env.I18n:T("usage.view")
            -- The memo keys its root, so the button's own key sits a level down.
            return ui.row({}, {
                button(
                    label,
                    { circle },
                    view:Handler(function()
                        env.Popover:Toggle({
                            id = "usage",
                            anchorKey = "context-usage",
                            side = "bottom",
                            align = "end",
                            gap = 4,
                        })
                    end),
                    open
                ),
            })
        end
        local function figure(key: string, words: string, value: number): ui.Node
            return ui.metric({
                key = key,
                value = value,
                text = words,
                -- The count sits in a `button-v2`, which sets figures at one width (`button.css:21`).
                tabular = true,
                size = 13,
                weight = "regular",
                lineHeight = 20,
                tracking = -0.04,
                color = "text-faint",
            })
        end
        local children: { ui.Node } = {
            circle,
            figure(`ctx-{sid}`, env.I18n:Number(usage.total, { notation = "compact" }), usage.total),
        }
        if usage.showCost then
            table.insert(
                children,
                ui.box({ w = 6, center = true }, {
                    ui.text(
                        "\u{b7}",
                        { size = 13, weight = "regular", lineHeight = 20, tracking = -0.04, color = "text-faint" }
                    ),
                })
            )
            table.insert(
                children,
                figure(`cost-{sid}`, env.I18n:Number(usage.cost, { style = "currency" }), math.round(usage.cost * 100))
            )
        end
        local label = env.I18n:T("usage.toggle")
        return ui.row({}, {
            button(
                label,
                children,
                view:Handler(function()
                    env.Layout:TogglePanel("main")
                end),
                false
            ),
        })
    end)
end

-- The narrow header's usage menu: tokens, cost, context share, then View context usage.
function indicator.menu(env: Env, view: View, _props: PopoverProps): ui.Node?
    local session = env.Sessions:Routed()
    if session == nil then
        return nil
    end
    local usage = indicator.usage(env, session)
    local font = { size = 13, weight = "regular" :: FontWeight, lineHeight = 16, tracking = -0.04 }
    local function line(name: string, value: string): ui.Node
        return ui.row({ h = 28, px = 12, gap = 16, align = "center" }, {
            ui.text(
                name,
                {
                    size = font.size,
                    weight = font.weight,
                    lineHeight = font.lineHeight,
                    tracking = font.tracking,
                    color = "text-muted",
                    shrink = true,
                }
            ),
            ui.spacer(),
            ui.text(
                value,
                {
                    size = font.size,
                    weight = font.weight,
                    lineHeight = font.lineHeight,
                    tracking = font.tracking,
                    tabular = true,
                    truncate = true,
                    color = "text-chrome",
                }
            ),
        })
    end
    local rows: { ui.Node } = { line(env.I18n:T("usage.tokens"), env.I18n:Number(usage.total)) }
    if usage.showCost then
        table.insert(rows, line(env.I18n:T("usage.cost"), env.I18n:Number(usage.cost, { style = "currency" })))
    end
    table.insert(
        rows,
        line(env.I18n:T("usage.usage"), `{usage.percent or 0}%`)
    )
    table.insert(rows, ui.box({ w = "full", py = 2 }, { ui.box({ w = "full", h = 1, fill = "border-weak" }) }))
    table.insert(
        rows,
        ui.row({
            key = "context-usage-view",
            role = "button",
            h = 28,
            pl = 12,
            pr = 6,
            align = "center",
            radius = 4,
            hover = "surface-hover",
            hoverTransition = "instant",
            onPress = view:Handler(function()
                env.Popover:Close()
                env.Layout:OpenPanel("main")
            end),
        }, {
            ui.text(
                env.I18n:T("usage.view"),
                {
                    size = font.size,
                    weight = font.weight,
                    lineHeight = font.lineHeight,
                    tracking = font.tracking,
                    color = "text-chrome",
                }
            ),
        })
    )
    return ui.col({
        key = "context-usage-menu",
        role = "list",
        label = env.I18n:T("usage.view"),
        w = 200,
        pad = 2,
        radius = 6,
        fill = "surface-1",
        shadow = "floating",
    }, rows)
end

-- The Context tab's label: the ring in its indicator look, 8 from the title, which keeps the
-- trigger's muted ink.
function indicator.label(env: Env, view: View, props: PanelTabLabelProps): ui.Node?
    local session = env.Sessions:Get(props.sessionId)
    local percent = if session then indicator.usage(env, session).percent or 0 else 0
    return view:Memo("tab-label", `{percent}:{themeKey(env)}`, function()
        return ui.row({ gap = 8, align = "center" }, {
            ring(percent, true),
            ui.text(
                env.I18n:T("tab.title"),
                { size = 13, weight = "regular", lineHeight = 16, tracking = -0.04, tabular = true, color = "text-muted", noWrap = true }
            ),
        })
    end)
end

return indicator
