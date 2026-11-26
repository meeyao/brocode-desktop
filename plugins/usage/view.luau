--!strict
-- The Usage dialog view (`usage/dialog.tsx`, `usage/dialog.css`): Usage header, Brocode Go
-- windows and Console budgets in bordered panels with spend and track bars, danger callout when
-- paid models are paused, model-switch action row, and footer with reset time and View details.

local ui = require("@opencode/ui")
local limits = require("./limits")
local machine = require("./machine")

local view = {}

-- Typography matching Desktop's `usage/dialog.css`.
local LINE = table.freeze({
    size = 13,
    weight = "regular" :: FontWeight,
    lineHeight = 16,
    tracking = -0.04,
} :: UiTextProps)

local LINE_MED = table.freeze({
    size = 13,
    weight = "medium" :: FontWeight,
    lineHeight = 16,
    tracking = -0.04,
} :: UiTextProps)

local BODY = table.freeze({
    size = 13,
    weight = "regular" :: FontWeight,
    lineHeight = 20,
    tracking = -0.04,
} :: UiTextProps)

local BODY_MED = table.freeze({
    size = 13,
    weight = "medium" :: FontWeight,
    lineHeight = 20,
    tracking = -0.04,
} :: UiTextProps)

local BADGE_TEXT = table.freeze({
    size = 11,
    weight = "medium" :: FontWeight,
    lineHeight = 12,
    tracking = 0.05,
    color = "text-muted" :: Color,
} :: UiTextProps)

local DOT_BOX = table.freeze({ w = 3, h = 3, center = true } :: UiBoxProps)
local DOT = table.freeze({ w = 1.125, h = 1.125, radius = 0.5625, fill = "icon-weak" :: Color } :: UiBoxProps)
local DIVIDER = table.freeze({ w = "full", h = 0.5, fill = "border-base" :: Color } :: UiBoxProps)
local ITEM = table.freeze({ w = "full", gap = 16, px = 16 } :: UiBoxProps)
local TRACK = table.freeze({ w = "full", h = 8, radius = 2, fill = "surface-3" :: Color, clip = true } :: UiBoxProps)

export type Callbacks = {
    onRetry: UiHandler,
    onSwitch: UiHandler,
    onDetails: UiHandler,
}

-- `format.money`: US dollars in the locale's currency pattern, cents always or (`auto`) only
-- off a whole amount.
local function money(I18n: I18nService, value: number, auto: boolean): string
    local whole = auto and (math.fmod(value, 1) == 0)
    return I18n:Number(
        value,
        { style = "currency", currency = "USD", maxFraction = if whole then 0 else 2, localePattern = true }
    )
end
view.money = money

-- `format.time`: `Nov 1 at 12:00 AM (GMT-7)`.
local function formatTime(I18n: I18nService, at: number): string
    return I18n:T("usage.time", {
        date = I18n:Date(at, "monthDay"),
        time = I18n:Date(at, "time"),
        zone = I18n:Date(at, "timeZone"),
    })
end

-- `duration`: what's left until `untilMs`, at least a minute.
local function formatDuration(I18n: I18nService, untilMs: number, nowMs: number): string
    local minutes = math.max(1, math.floor((untilMs - nowMs) / 60000))
    if minutes >= 1440 then
        return I18n:T("usage.duration.days", { days = minutes // 1440, hours = (minutes % 1440) // 60 })
    end
    if minutes >= 60 then
        return I18n:T("usage.duration.hours", { hours = minutes // 60, minutes = minutes % 60 })
    end
    return I18n:T("usage.duration.minutes", { minutes = minutes })
end

local function roundInt(n: number): number
    if n >= 0 then
        return math.floor(n + 0.5)
    end
    return math.ceil(n - 0.5)
end

local function line(dir: TextDirection, words: string, style: UiTextProps, color: Color, extra: UiTextProps?): UiNode
    if extra ~= nil then
        local props = table.clone(extra) :: any
        props.color = color
        props.dir = dir
        props.truncate = true
        props.shrink = true
        props.minW = 0
        return ui.text(words, style, props)
    end
    return ui.text(words, style, { color = color, dir = dir, truncate = true, shrink = true, minW = 0 })
end

-- `Panel`: 8 pt corners, a 0.5 `border-base` edge on `bg-base`, 16 above and below.
local function panel(gap: number, children: { UiNode }): UiNode
    return ui.frame({
        w = "full",
        py = 16,
        gap = gap,
        radius = 8,
        fill = "bg-base",
        border = "border-base",
        borderWidth = 0.5,
    }, children)
end

local function message(dir: TextDirection, title: string, body: string): UiNode
    return panel(8, {
        ui.col({ w = "full", px = 16, gap = 4 }, {
            line(dir, title, LINE_MED, "text-chrome"),
            ui.text(body, BODY, { color = "text-muted", dir = dir, wrap = true, w = "full" }),
        }),
    })
end

-- `Spend`: the status, a 1.125 pt dot, then the detail.
local function spend(dir: TextDirection, status: string, detail: string, isReached: boolean): UiNode
    local ink: Color = if isReached then "status-danger-strong" else "text-chrome"
    return ui.row({ gap = 6, align = "center" }, {
        ui.text(status, LINE_MED, { tabular = true, truncate = true, shrink = true, minW = 0, color = ink, dir = dir }),
        ui.box(DOT_BOX, { ui.box(DOT) }),
        line(dir, detail, LINE, "text-muted"),
    })
end

-- `Track`: an 8 pt bar on `surface-3`, filled to `fill` in `surface-inverse`, or the reached red.
local function track(fill: number, isReached: boolean): UiNode
    local f = math.clamp(fill, 0, 1)
    local ink: Color = if isReached then "status-danger-strong" else "surface-inverse"
    return ui.row(TRACK, {
        ui.box({ h = 8, grow = f, fill = ink }),
        ui.spacer({ grow = 1 - f }),
    })
end

-- `Section`: its muted label (and the workspace's badge) over its panel, 12 apart.
local function section(dir: TextDirection, labelText: string, badge: string?, dimmed: boolean, bodyNode: UiNode): UiNode
    local headChildren: { UiNode } = { line(dir, labelText, LINE, "text-muted") }
    if badge ~= nil then
        table.insert(
            headChildren,
            ui.row({ h = 16, px = 4, radius = 2, align = "center", fill = "surface-3" }, {
                ui.text(badge, BADGE_TEXT, { dir = dir }),
            })
        )
    end
    local head = ui.row({ w = "full", gap = 8, pl = 16, pr = 20, align = "center" }, headChildren)
    return ui.col({ w = "full", gap = 12, opacity = if dimmed then 0.6 else nil }, {
        head,
        ui.col({ w = "full", px = 12 }, { bodyNode }),
    })
end

local function budgetLabelText(I18n: I18nService, l: limits.Label): string
    if l.kind == "all" then
        return I18n:T("usage.budget.all")
    elseif l.kind == "each" then
        return I18n:T("usage.budget.each")
    elseif l.kind == "vendors" then
        local vendors = I18n:List(l.vendors)
        if l.price ~= nil then
            local price = money(I18n, l.price, true)
            return I18n:T(if l.each then "usage.budget.vendorsPriceEach" else "usage.budget.vendorsPrice", {
                vendors = vendors,
                price = price,
            })
        end
        return I18n:T(if l.each then "usage.budget.vendorsEach" else "usage.budget.vendors", {
            vendors = vendors,
        })
    elseif l.kind == "price" then
        local price = money(I18n, l.price, true)
        return I18n:T(if l.each then "usage.budget.priceEach" else "usage.budget.price", { price = price })
    else
        local models = if #l.names > 2
            then I18n:T("usage.budget.modelsMore", {
                count = #l.names - 2,
                first = l.names[1],
                second = l.names[2],
            })
            else I18n:List(l.names)
        return if l.each then I18n:T("usage.budget.modelsEach", { models = models }) else models
    end
end

local function noteText(I18n: I18nService, n: limits.Note, r: limits.Row): string
    if n.kind == "runs-out-first" then
        return I18n:T("usage.note.runsOutFirst")
    elseif n.kind == "paused" then
        return I18n:T("usage.note.paused", { label = budgetLabelText(I18n, r.label) })
    elseif n.kind == "includes-web-search" then
        return I18n:T("usage.note.webSearch")
    elseif n.kind == "paused-by" then
        return I18n:T(if n.shared then "usage.note.pausedByWorkspace" else "usage.note.pausedByBudget")
    else
        return I18n:T("usage.note.currentModel", { model = n.name })
    end
end

local function scopeText(I18n: I18nService, r: limits.Row, workspace: string): string
    local limit = money(I18n, r.limit, true)
    local key: string
    if r.sharing == "you" then
        key = if r.perModel then "usage.budget.youEach" else "usage.budget.you"
    elseif r.sharing == "everyone" then
        key = if r.perModel then "usage.budget.everyoneEach" else "usage.budget.everyone"
    else
        key = if r.perModel then "usage.budget.groupEach" else "usage.budget.group"
    end
    local parts: { string } = { I18n:T(key, { limit = limit, workspace = workspace }) }
    for _, n in r.notes do
        table.insert(parts, noteText(I18n, n, r))
    end
    return table.concat(parts, " · ")
end

local function budgetRow(I18n: I18nService, dir: TextDirection, r: limits.Row, workspace: string): UiNode
    local status: string
    local detail: string
    if r.reached then
        status = I18n:T("usage.budget.reached")
        detail = I18n:T("usage.budget.spentOf", {
            spent = money(I18n, r.spent, false),
            limit = money(I18n, r.limit, true),
        })
    else
        status = I18n:T("usage.budget.left", { amount = money(I18n, r.left, false) })
        detail = I18n:T("usage.budget.of", { limit = money(I18n, r.limit, true) })
    end
    return ui.col(ITEM, {
        ui.row({ w = "full", align = "start" }, {
            ui.col({ grow = 1, shrink = true, minW = 0, gap = 2 }, {
                line(dir, budgetLabelText(I18n, r.label), LINE_MED, "text-chrome"),
                ui.text(scopeText(I18n, r, workspace), LINE, { color = "text-muted", dir = dir, wrap = true, w = "full" }),
            }),
            spend(dir, status, detail, r.reached),
        }),
        track(r.fill, r.reached),
    })
end

local function calloutNode(I18n: I18nService, dir: TextDirection, c: limits.Callout, workspace: string): UiNode
    local title: string
    local body: string
    if c.kind == "credits" then
        title = I18n:T("usage.callout.credits.title", { workspace = workspace })
        body = I18n:T("usage.callout.credits.body")
    elseif c.kind == "workspace" then
        title = I18n:T("usage.callout.workspace.title")
        body = I18n:T("usage.callout.workspace.body", { workspace = workspace, time = formatTime(I18n, c.resetsAt) })
    else
        title = I18n:T("usage.callout.personal.title")
        body = I18n:T("usage.callout.personal.body", { workspace = workspace, time = formatTime(I18n, c.resetsAt) })
    end
    return ui.row({ key = "usage-callout", w = "full", px = 16, gap = 2, align = "stretch" }, {
        ui.box({ w = 2, fill = "status-danger" }),
        ui.col({
            grow = 1,
            shrink = true,
            minW = 0,
            gap = 6,
            pad = 12,
            corners = { 0, 6, 6, 0 },
            fill = "status-danger-bg",
        }, {
            line(dir, title, LINE_MED, "status-danger-strong"),
            ui.text(body, BODY, { color = "status-danger-strong", dir = dir, wrap = true, w = "full" }),
        }),
    })
end

local function actionRow(I18n: I18nService, dir: TextDirection, a: limits.Action, onSwitch: UiHandler): UiNode
    local title: string
    local body: string
    if a.kind == "other-models" then
        title = I18n:T("usage.action.otherModels.title")
        body = if #a.vendors == 0
            then I18n:T("usage.action.otherModels.body")
            else I18n:T("usage.action.otherModels.vendors", { vendors = I18n:List(a.vendors) })
    else
        local limit = money(I18n, a.limit, true)
        title = if #a.vendors == 0
            then I18n:T("usage.action.each.title")
            else I18n:T("usage.action.each.titleVendors", { vendors = I18n:List(a.vendors) })
        if a.suggestion ~= nil then
            body = I18n:T("usage.action.each.suggestion", {
                limit = limit,
                model = a.suggestion.name,
                left = money(I18n, a.suggestion.left, false),
            })
        else
            body = I18n:T("usage.action.each.body", { limit = limit })
        end
    end
    return ui.row({ key = "usage-action", w = "full", px = 16, gap = 16, align = "center" }, {
        ui.col({ grow = 1, shrink = true, minW = 0, gap = 4 }, {
            line(dir, title, LINE_MED, "text-chrome"),
            ui.text(body, BODY, { color = "text-muted", dir = dir, wrap = true, w = "full" }),
        }),
        ui.button({
            key = "usage-switch",
            text = I18n:T("usage.action.switch"),
            variant = "neutral",
            size = "normal",
            onPress = onSwitch,
        }),
    })
end

local function footerRow(I18n: I18nService, dir: TextDirection, v: limits.UsageView, onDetails: UiHandler): UiNode
    local words: string
    if v.footer.kind == "credits-only" then
        local workspace = v.workspace or I18n:T("usage.workspace.fallback")
        words = I18n:T("usage.footer.creditsOnly", { workspace = workspace })
    elseif v.footer.kind == "budgets-reset" then
        words = I18n:T("usage.footer.budgetsReset", { time = formatTime(I18n, v.footer.at) })
    else
        words = I18n:T("usage.footer.resets", { time = formatTime(I18n, v.footer.at) })
    end
    local detailsLabel = I18n:T("usage.footer.details")
    return ui.row({ key = "usage-footer", w = "full", pad = 16, gap = 8, align = "center" }, {
        ui.row({ grow = 1, shrink = true, minW = 0, h = 28, align = "center" }, {
            line(dir, words, LINE, "text-faint"),
        }),
        ui.row({
            key = "usage-details",
            role = "button",
            label = detailsLabel,
            h = 28,
            px = 11,
            radius = 6,
            center = true,
            hover = "surface-hover",
            press = "surface-press",
            onPress = onDetails,
        }, {
            ui.text(detailsLabel, BODY_MED, { color = "text-muted", dir = dir }),
        }),
    })
end

local function contentNode(I18n: I18nService, dir: TextDirection, v: limits.UsageView, nowMs: number, onSwitch: UiHandler): UiNode
    local workspace = v.workspace or I18n:T("usage.workspace.fallback")
    local children: { UiNode } = {}

    if v.callout ~= nil then
        table.insert(children, calloutNode(I18n, dir, v.callout, workspace))
    end

    if v.go ~= nil then
        local goChildren: { UiNode } = {}
        for k, g in v.go do
            if k > 1 then
                table.insert(goChildren, ui.box(DIVIDER))
            end
            local status = I18n:T("usage.go.used", { percent = roundInt(g.meter.percent) })
            local resets = I18n:T("usage.go.resetsIn", { duration = formatDuration(I18n, g.meter.resetsAt, nowMs) })
            local title = if g.window == "rolling"
                then I18n:T("usage.go.rolling")
                elseif g.window == "weekly" then I18n:T("usage.go.weekly")
                else I18n:T("usage.go.monthly")
            table.insert(
                goChildren,
                ui.col(ITEM, {
                    ui.row({ w = "full", align = "center" }, {
                        line(dir, title, LINE_MED, "text-chrome", { grow = 1 }),
                        spend(dir, status, resets, g.meter.limited),
                    }),
                    track(g.meter.percent / 100, g.meter.limited),
                })
            )
        end
        table.insert(children, section(dir, I18n:T("usage.section.go"), nil, false, panel(16, goChildren)))
    end

    local budgetsNode: UiNode
    if #v.budgets == 0 then
        budgetsNode = message(dir, I18n:T("usage.empty.title"), I18n:T("usage.empty.body", { workspace = workspace }))
    else
        local budgetChildren: { UiNode } = {}
        for k, r in v.budgets do
            if k > 1 then
                table.insert(budgetChildren, ui.box(DIVIDER))
            end
            table.insert(budgetChildren, budgetRow(I18n, dir, r, workspace))
        end
        if v.action ~= nil then
            table.insert(budgetChildren, actionRow(I18n, dir, v.action, onSwitch))
        end
        budgetsNode = panel(16, budgetChildren)
    end
    table.insert(children, section(dir, I18n:T("usage.section.console"), v.workspace, v.dimmed, budgetsNode))

    return ui.col({ key = "usage-content", w = "full", gap = 16 }, children)
end

local function fallbackNode(I18n: I18nService, dir: TextDirection, load: machine.Load, onRetry: UiHandler): UiNode
    if load.status == "signed-out" then
        return ui.col({ key = "usage-state", w = "full", px = 12, pb = 12 }, {
            message(dir, I18n:T("usage.signedOut.title"), I18n:T("usage.signedOut.body")),
        })
    end
    local inner: UiNode
    if load.status == "failed" then
        inner = ui.col({ gap = 12, align = "center" }, {
            ui.text(I18n:T("usage.error"), LINE, { color = "text-muted", dir = dir }),
            ui.button({
                key = "usage-retry",
                text = I18n:T("usage.retry"),
                variant = "neutral",
                size = "normal",
                onPress = onRetry,
            }),
        })
    else
        inner = ui.col({}, { ui.loader({}) })
    end
    return ui.row({ key = "usage-state", w = "full", minH = 120, px = 16, pb = 16, center = true }, { inner })
end

function view.render(
    I18n: I18nService,
    load: machine.Load,
    model: limits.CurrentModel?,
    credits: boolean,
    nowMs: number,
    callbacks: Callbacks
): UiNode
    local dir = I18n:Direction()
    local body: UiNode
    local foot: UiNode? = nil
    if load.status == "ready" or load.status == "refreshing" then
        local v = limits.view(load.info, model, credits)
        body = contentNode(I18n, dir, v, nowMs, callbacks.onSwitch)
        foot = footerRow(I18n, dir, v, callbacks.onDetails)
    else
        body = fallbackNode(I18n, dir, load, callbacks.onRetry)
    end

    return ui.col({ w = "full" }, {
        ui.dialogHeader({
            title = I18n:T("usage.title"),
            closeLabel = I18n:T("ui.common.close"),
            wrapTitle = false,
        }),
        ui.col({ key = "usage-body", w = "full", gap = 16 }, { body }),
        foot,
    })
end

return view
