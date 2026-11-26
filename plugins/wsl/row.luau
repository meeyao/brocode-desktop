--!strict
-- A WSL server's row in Settings › Servers (Desktop's `row.tsx`): its health, the distro's name
-- and the WSL badge, the Brocode version it runs, Install or Update Brocode when it needs it,
-- and its menu (Retry start, Remove).

local ui = require("@opencode/ui")
local model = require("./model")

local row = {}

-- `.settings-servers-row`: the lead (indicator, name over meta) and the actions, 16 apart.
function row.render(env: any, v: View, props: ServerRowProps): UiNode?
    local api = env.api()
    if not api then
        return nil
    end
    local distro = props.id
    local state = api.state
    local r = model.row(state, distro)
    local I18n = env.I18n
    return v:Memo(`row-{distro}`, `{env.version()}|{props.health and props.health.version or ""}`, function()
        local name = ui.row({ gap = 4, align = "center", minW = 0 }, {
            ui.text(distro, { size = 13, weight = "medium", lineHeight = 16, color = "text-chrome", truncate = true, shrink = true }),
            ui.row({ px = 4, py = 2, radius = 3, border = "border-base", borderWidth = 1 }, {
                ui.text(I18n:T("server.label"), { size = 9, weight = "regular", lineHeight = 9, color = "text-muted" }),
            }),
        })
        local copy = ui.col({ grow = 1, shrink = true, minW = 0, gap = 6 }, {
            name,
            ui.text(if r.version then `v{r.version}` else "", { size = 11, weight = "regular", lineHeight = 11, color = "text-muted" }),
        })
        local lead = ui.row({ grow = 1, shrink = true, minW = 0, gap = 10 }, {
            ui.serverIndicator({ health = props.health, connecting = r.runtime == "starting" or (r.runtime == "ready" and props.health == nil) }),
            copy,
        })
        local actions: { UiNode } = {}
        if r.button then
            table.insert(actions, ui.button({
                key = `wsl-row-install-{distro}`,
                text = I18n:T(r.button.label),
                variant = "neutral",
                size = "small",
                disabled = r.button.disabled,
                onPress = v:Handler(function()
                    env.run("installOpencode", distro)
                end),
            }))
        end
        local anchor = `wsl-row-menu-{distro}`
        table.insert(actions, ui.iconButton({
            key = anchor,
            icon = "outline-dots",
            variant = "ghost-muted",
            size = "small",
            label = I18n:T("common.moreOptions"),
            menuTrigger = env.menuTrigger,
            onPress = v:Handler(function()
                local now = model.row(env.api() and env.api().state, distro)
                local items: { MenuEntry } = { { kind = "group", title = I18n:T("server.menu.label") } }
                if now.retry then
                    table.insert(items, { id = "retry", title = I18n:T("server.retryStart") })
                end
                table.insert(items, { kind = "separator" })
                table.insert(items, { id = "remove", title = I18n:T("menu.remove"), enabled = now.removable })
                local picked = env.Menu:OpenAsync({ anchorKey = anchor, items = items, label = I18n:T("server.menu.label"), align = "end" })
                if picked == "retry" then
                    env.run("startServer", `wsl:{distro}`)
                elseif picked == "remove" then
                    env.run("removeServer", `wsl:{distro}`)
                end
            end),
        }))
        return ui.row({ key = `wsl-row-{distro}`, w = "full", gap = 16, align = "center" }, {
            lead,
            ui.row({ gap = 8, align = "center" }, actions),
        })
    end)
end

return row
