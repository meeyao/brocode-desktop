--!strict
-- Settings > Browser matching upstream `packages/gui-plugins/src/browser/settings.tsx`,
-- `settings-view.ts`, `UP-108` (#65095), and `UP-111` (#65101):
-- - Persists user preferences under `plugin.browser/preferences` without altering existing keys
--   (`history`, `rune.siteZoom`, `rune.sitePermissions`).
-- - Capability-gates sections whose engine side is owned by WS-1 / WS-9 (`import`, `extensions`,
--   `remoteSignIns`, `cookieFile`) so nothing broken or stubbed is ever shown.
-- - Collapsible `Advanced` section automatically expands when Settings search targets an entry
--   inside it, without mutating the stored `advanced` preference.

local ui = require("@opencode/ui")
local hostDisplay = require("./host_display")

local settings = {}

export type Preferences = {
    links: boolean,
    agentPages: boolean,
    advanced: boolean,
    welcomed: boolean,
    -- `profile/index.ts:118` `remote-sign-ins`: off until the person turns it on.
    remoteSignIns: boolean,
}

export type SiteDecision = {
    origin: string,
    permission: string,
    setting: "allow" | "block",
}

export type Capabilities = {
    available: boolean,
    import: boolean,
    extensions: boolean,
    remoteSignIns: boolean,
    cookieFile: boolean,
}

export type ImportProfile = {
    id: string,
    name: string,
}

export type ImportBrowser = {
    id: string,
    name: string,
    engine: string,
    profiles: { ImportProfile },
}

-- How the last import from one browser profile ended, shown in its row (`settings.tsx` `Identity`).
export type ImportMessage = {
    text: string,
    error: boolean,
    -- Announced as it changes (an action's outcome), not a fixed detail.
    live: boolean?,
}

export type Env = {
    I18n: I18nService,
    preferences: () -> Preferences,
    updatePreferences: (patch: { [string]: any }) -> (),
    capabilities: () -> Capabilities,
    sitePermissions: () -> { SiteDecision },
    permissionRestartNote: (() -> boolean)?,
    setSitePermission: (origin: string, permission: string, setting: "allow" | "block") -> (),
    removeSitePermission: (origin: string, permission: string) -> (),
    clearNotice: () -> string?,
    -- The profile's cookie count while the engine runs (`Browser:CookieCount`), else nil.
    cookieCount: () -> number?,
    -- Clear browsing data is running.
    clearing: () -> boolean,
    requestClearBrowsingData: () -> (),
    importSources: () -> { ImportBrowser },
    -- Whether the last `importSources` call had a listing to give: false until the host's first
    -- one is in.
    importSourcesLoaded: () -> boolean,
    importMessage: (browserId: string, profileId: string) -> ImportMessage?,
    openImport: (browserId: string, profileId: string) -> (),
    importIcon: (browserId: string) -> ImageId?,
    Menu: MenuService,
    -- Optional contract-backed sections provided by WS-9 (extensions) or WS-1 (cookies/remote).
    renderImport: ((view: View) -> ui.Node?)?,
    renderExtensions: ((view: View) -> ui.Node?)?,
    -- nil while the engine's cookie store is unknown (`settings.tsx:208-209`).
    remoteSignInsEnabled: (() -> boolean?)?,
    setRemoteSignIns: ((enabled: boolean) -> ())?,
    pickCookieFile: (() -> ())?,
    cookieFileMessage: (() -> ImportMessage?)?,
    cookieFileBusy: (() -> boolean)?,
}

-- Search entries whose rows live inside the collapsible Advanced disclosure (`settings-view.ts`).
local ADVANCED_ENTRIES: { [string]: boolean } = {
    ["settings-browser-remote"] = true,
    ["settings-browser-agent-pages"] = true,
    ["settings-browser-sites"] = true,
    ["settings-browser-cookies"] = true,
}

local DEFAULT_PREFS: Preferences = table.freeze({
    links = true,
    agentPages = true,
    advanced = false,
    welcomed = false,
    remoteSignIns = false,
})

function settings.normalizePreferences(raw: any): Preferences
    if type(raw) ~= "table" then
        return table.clone(DEFAULT_PREFS)
    end
    return {
        links = raw.links ~= false,
        agentPages = raw.agentPages ~= false,
        advanced = raw.advanced == true,
        welcomed = raw.welcomed == true,
        remoteSignIns = raw.remoteSignIns == true,
    }
end

function settings.advancedOpen(stored: boolean, target: string?, closedOver: string?): boolean
    if stored then
        return true
    end
    if target ~= nil and target ~= closedOver and ADVANCED_ENTRIES[target] == true then
        return true
    end
    return false
end

-- Builds Settings search entries reflecting only the currently active capabilities.
function settings.entries(i18n: I18nService, caps: Capabilities): { SettingEntry }
    local out: { SettingEntry } = {
        {
            id = "settings-browser-links",
            title = i18n:T("settings.links.title"),
            description = i18n:T("settings.links.description"),
            keywords = "links open system default browser opencode's browser web shift click",
        },
        {
            id = "settings-browser-agent-pages",
            title = i18n:T("settings.agentPages.title"),
            description = i18n:T("settings.agentPages.description"),
            keywords = "open the browser when the agent shows a page agent preview automatic browser pane tab file",
        },
        {
            id = "settings-browser-sites",
            title = i18n:T("settings.sites.title"),
            keywords = "site permissions camera microphone location notifications clipboard",
        },
        {
            id = "settings-browser-cookies",
            title = i18n:T("settings.cookies.title"),
            keywords = "cookies sign out browsing data history clear",
        },
    }
    if caps.import then
        table.insert(out, {
            id = "settings-browser-import",
            title = i18n:T("settings.import.title"),
            keywords = "import cookies history chrome edge firefox safari helium arc brave sign in",
        })
    end
    if caps.extensions then
        table.insert(out, {
            id = "settings-browser-extensions",
            title = i18n:T("settings.extensions.title"),
            keywords = "extensions",
        })
    end
    if caps.remoteSignIns then
        table.insert(out, {
            id = "settings-browser-remote",
            title = i18n:T("settings.remote.title"),
            description = i18n:T("settings.remote.description"),
            keywords = "remote server sign in cookies share",
        })
    end
    return out
end

-- `settings.tsx` `Note`: 12 / 16 in the faint ink, under a section's card.
local NOTE: UiTextProps = table.freeze({ size = 12, weight = "regular", lineHeight = 16, color = "text-faint", wrap = true } :: UiTextProps)
local MUTED: UiTextProps = table.freeze({ size = 13, weight = "regular", lineHeight = 20, color = "text-muted", wrap = true } :: UiTextProps)
-- `.settings-section-title`, as the Advanced heading's text.
local HEADING: UiTextProps = table.freeze({ size = 13, weight = "medium", lineHeight = 16, color = "text-chrome", noWrap = true } :: UiTextProps)
-- A settings row's title (`SettingsRow`), for rows that draw their own subject.
local ROW_TITLE: UiTextProps = table.freeze({ size = 13, weight = "medium", lineHeight = 16, tracking = -0.04, color = "text-chrome", truncate = true } :: UiTextProps)

local SUBLINE: UiTextProps = table.freeze({ size = 13, weight = "regular", lineHeight = 20, color = "text-muted", wrap = true } :: UiTextProps)

-- Official's summary names the remote-server and cookie-file rows too; it is shown only once both exist.
local function advancedSummary(i18n: I18nService, caps: Capabilities): string
    if caps.remoteSignIns and caps.cookieFile then
        return i18n:T("settings.advanced.summary")
    end
    return i18n:T("settings.advanced.summary.local")
end

-- `choices.tsx` `Glyph`: the browser's own icon at 20 pt (`size-5`, square), or a 20 pt box holding
-- a muted 16 pt icon where it has none.
local function glyph(name: string, image: ImageId?): ui.Node
    if image ~= nil then
        return ui.image({ image = image, w = 20, h = 20, fit = "contain", shrink = false })
    end
    return ui.row({ w = 20, h = 20, center = true, shrink = false }, {
        ui.icon(name, { size = 16, color = "icon-weak" }),
    })
end

-- `settings.tsx` `Identity` in a row of its own: the icon, the name with an optional aside as a
-- badge on its line (`settings.tsx:887-895`), and under them a detail or what the last action did.
-- Search reveals it by `id`.
local function identityRow(
    key: string,
    id: string?,
    icon: string,
    image: ImageId?,
    name: string,
    aside: string?,
    detail: ImportMessage?,
    control: ui.Node?
): ui.Node
    local line: { ui.Node } = { ui.text(name, ROW_TITLE, { shrink = true, minW = 0 }) }
    if aside ~= nil then
        table.insert(line, ui.badge({ text = aside }))
    end
    local words: { ui.Node } = { ui.row({ gap = 6, align = "center", minW = 0 }, line) }
    if detail ~= nil then
        table.insert(words, ui.text(detail.text, SUBLINE, {
            role = if detail.error then "alert" elseif detail.live then "status" else nil,
            color = if detail.error then "status-danger" else "text-muted",
        }))
    end
    local items: { ui.Node } = {
        ui.row({ grow = 1, shrink = true, minW = 0, gap = 12, align = "center" }, {
            glyph(icon, image),
            ui.col({ grow = 1, shrink = true, minW = 0, gap = 4 }, words),
        }),
    }
    if control then
        table.insert(items, control)
    end
    return ui.row({ key = if id then `setting-{id}` else key, w = "full", gap = 16, py = 20, align = "center" }, items)
end

-- A row holding only muted words, as official's empty states (`Muted` titles).
local function mutedRow(id: string, words: string): ui.Node
    return ui.row({ key = `setting-{id}`, w = "full", py = 20 }, {
        ui.text(words, MUTED),
    })
end

local function permissionName(i18n: I18nService, kind: string): string
    local key = `settings.sites.permission.{kind}`
    local translated = i18n:T(key)
    return if translated ~= key then translated else kind
end

-- Stateful page renderer holding ephemeral visit state (`closedOver` search target).
function settings.create(env: Env, refresh: () -> ())
    local closedOver: string? = nil

    local function render(view: View, props: SettingsPageProps): ui.Node
        local i18n = env.I18n
        local caps = env.capabilities()
        if not caps.available then
            return ui.col({ key = "settings-browser-page", w = "full", gap = 16 }, {
                ui.settingsList({ key = "settings-browser-unavailable" }, {
                    ui.row({ w = "full", px = 16, py = 12 }, {
                        ui.text(i18n:T("settings.unavailable"), MUTED),
                    }),
                }),
            })
        end

        local prefs = env.preferences()
        local target = props.target
        local isAdvanced = settings.advancedOpen(prefs.advanced, target, closedOver)
        local sections: { ui.Node } = {}

        -- 1. Import from another browser (user-explicit read-only history import + optional WS-1 cookie import)
        if caps.import then
            local customImport = if env.renderImport then env.renderImport(view) else nil
            if customImport then
                table.insert(sections, customImport)
            else
                -- `settings.tsx:88-129`: one row per detected browser profile, the browser used last first.
                local rows: { ui.Node } = {}
                for _, browser in env.importSources() do
                    for _, prof in browser.profiles do
                        local idx = #rows + 1
                        local bid, pid = browser.id, prof.id
                        table.insert(rows, identityRow(
                            `import-source-{idx}`,
                            if idx == 1 then "settings-browser-import" else nil,
                            "outline-globe",
                            env.importIcon(bid),
                            browser.name,
                            if #browser.profiles > 1 then prof.name else nil,
                            env.importMessage(bid, pid),
                            ui.button({
                                key = `import-source-btn-{idx}`,
                                text = i18n:T("settings.import.open"),
                                size = "small",
                                variant = "neutral",
                                onPress = view:Handler(function()
                                    env.openImport(bid, pid)
                                end),
                            })
                        ))
                    end
                end
                -- `settings.tsx:151`: said only once the browsers are listed, not while they are.
                if #rows == 0 and env.importSourcesLoaded() then
                    table.insert(rows, mutedRow("settings-browser-import", i18n:T("settings.import.none")))
                end
                table.insert(sections, ui.settingsList({
                    key = "settings-browser-import-section",
                    title = i18n:T("settings.import.title"),
                }, rows))
            end
        end

        -- 2. Extensions (shown when WS-9's extensions capability is active)
        if caps.extensions and env.renderExtensions then
            local extNode = env.renderExtensions(view)
            if extNode then
                table.insert(sections, extNode)
            end
        end

        -- 3. Open links in Brocode (`settings-browser-links`)
        table.insert(sections, ui.settingsList({ key = "settings-browser-links-section" }, {
            ui.settingsRow(
                {
                    id = "settings-browser-links",
                    title = i18n:T("settings.links.title"),
                    description = i18n:T("settings.links.description"),
                },
                ui.switch({
                    key = "settings-browser-links-switch",
                    label = i18n:T("settings.links.title"),
                    checked = prefs.links,
                    onChange = view:Handler(function(event)
                        env.updatePreferences({ links = (event :: any).checked == true })
                    end),
                })
            ),
        }))

        -- 4. Collapsible Advanced disclosure heading (`settings.tsx:172-198`)
        local disclosureItems: { ui.Node } = {
            ui.text(i18n:T("settings.advanced"), HEADING),
        }
        if not isAdvanced then
            table.insert(disclosureItems, ui.text(advancedSummary(i18n, caps), {
                size = 13,
                weight = "regular",
                lineHeight = 16,
                color = "text-muted",
                ml = 8,
                truncate = true,
                shrink = true,
                minW = 0,
            }))
        end
        -- `ADVANCED_CHEVRON`: the heading's own ink at 40%, ending the line's text.
        table.insert(disclosureItems, ui.icon(if isAdvanced then "outline-chevron-down" else "outline-chevron-right", {
            size = 16,
            color = "text-chrome",
            opacity = 0.4,
        }))
        -- `settings.tsx:171-197`: a section heading with no card and no fill that is a button.
        table.insert(sections, ui.row({
            key = "settings-browser-advanced",
            role = "button",
            label = i18n:T("settings.advanced"),
            w = "full",
            gap = 4,
            radius = 4,
            align = "center",
            onPress = view:Handler(function()
                local nextOpen = not isAdvanced
                closedOver = if nextOpen then nil else target
                env.updatePreferences({ advanced = nextOpen })
                refresh()
            end),
        }, disclosureItems))

        if isAdvanced then
            -- 4a. Behavior rows (Remote sign-in sharing if capable + Agent pages toggle)
            local advRows: { ui.Node } = {}
            if caps.remoteSignIns and env.remoteSignInsEnabled and env.setRemoteSignIns then
                -- `settings.tsx:200-212`: first row of the Advanced card.
                local shared = env.remoteSignInsEnabled()
                table.insert(advRows, ui.settingsRow(
                    {
                        id = "settings-browser-remote",
                        title = i18n:T("settings.remote.title"),
                        description = i18n:T("settings.remote.description"),
                    },
                    ui.switch({
                        key = "settings-browser-remote-switch",
                        label = i18n:T("settings.remote.title"),
                        checked = shared == true,
                        disabled = shared == nil,
                        onChange = view:Handler(function(event)
                            if env.setRemoteSignIns then
                                env.setRemoteSignIns((event :: any).checked == true)
                            end
                        end),
                    })
                ))
            end
            table.insert(advRows, ui.settingsRow(
                {
                    id = "settings-browser-agent-pages",
                    title = i18n:T("settings.agentPages.title"),
                    description = i18n:T("settings.agentPages.description"),
                },
                ui.switch({
                    key = "settings-browser-agent-pages-switch",
                    label = i18n:T("settings.agentPages.title"),
                    checked = prefs.agentPages,
                    onChange = view:Handler(function(event)
                        env.updatePreferences({ agentPages = (event :: any).checked == true })
                    end),
                })
            ))
            table.insert(sections, ui.settingsList({ key = "settings-browser-behavior-section" }, advRows))

            -- 4b. Import cookie file (`settings.tsx:230-255`): how the last import ended takes the
            -- description's place, in the danger ink when it failed.
            if caps.cookieFile and env.pickCookieFile then
                local message = if env.cookieFileMessage then env.cookieFileMessage() else nil
                local busy = env.cookieFileBusy ~= nil and env.cookieFileBusy()
                table.insert(sections, ui.settingsList({ key = "settings-browser-cookie-file-section" }, {
                    ui.row({ key = "setting-settings-browser-cookie-file", w = "full", gap = 16, py = 20, align = "center" }, {
                        ui.col({ grow = 1, shrink = true, minW = 0, gap = 4 }, {
                            ui.text(i18n:T("settings.import.file.title"), ROW_TITLE, { truncate = false, wrap = true }),
                            ui.text(if message then message.text else i18n:T("settings.import.file.description"), SUBLINE, {
                                role = if message == nil then nil elseif message.error then "alert" else "status",
                                color = if message ~= nil and message.error then "status-danger" else "text-muted",
                            }),
                        }),
                        ui.button({
                            key = "settings-browser-cookie-file-btn",
                            text = i18n:T("settings.import.file.action"),
                            size = "small",
                            variant = "neutral",
                            loading = busy,
                            disabled = busy,
                            onPress = view:Handler(function()
                                if env.pickCookieFile then
                                    env.pickCookieFile()
                                end
                            end),
                        }),
                    }),
                }))
            end

            -- 4c. Site permissions section (`settings-browser-sites`)
            local decisions = env.sitePermissions()
            local siteRows: { ui.Node } = {}
            if #decisions == 0 then
                table.insert(siteRows, mutedRow("settings-browser-sites", i18n:T("settings.sites.none")))
            else
                for idx, d in decisions do
                    local origin = d.origin
                    local perm = d.permission
                    local setting = d.setting
                    local host = hostDisplay.displayHost(origin)
                    if host == "" then
                        host = origin
                    end
                    local permLabel = permissionName(i18n, perm)
                    local allowLabel = i18n:T("settings.sites.allow")
                    local blockLabel = i18n:T("settings.sites.block")
                    local selectKey = `site-perm-setting-{idx}`
                    local selectLabel = i18n:T("settings.sites.setting", { permission = permLabel, site = host })
                    -- `settings.tsx:339-351`: an inline `Select` (`SETTING_COLUMN`, 112 wide) of Allowed and
                    -- Blocked, drawn as the kit's inline trigger (24 tall, 8 / 4 in, value then chevron).
                    local controls = ui.row({ gap = 8, align = "center" }, {
                        ui.row({ w = 112, justify = "end" }, {
                            ui.row({
                                key = selectKey,
                                role = "button",
                                label = selectLabel,
                                h = 24,
                                py = 4,
                                pl = 8,
                                pr = 4,
                                gap = 4,
                                radius = 4,
                                align = "center",
                                hover = "surface-hover",
                                press = "surface-press",
                                hoverTransition = "instant",
                                shrink = true,
                                minW = 0,
                                menuTrigger = true,
                                onPress = view:Handler(function()
                                    local picked = env.Menu:OpenAsync({
                                        anchorKey = selectKey,
                                        label = selectLabel,
                                        align = "end",
                                        minWidth = 160,
                                        items = {
                                            { id = "allow", title = allowLabel, checked = setting == "allow" },
                                            { id = "block", title = blockLabel, checked = setting == "block" },
                                        },
                                    })
                                    if (picked == "allow" or picked == "block") and picked ~= setting then
                                        env.setSitePermission(origin, perm, picked :: any)
                                    end
                                end),
                            }, {
                                ui.text(if setting == "allow" then allowLabel else blockLabel, {
                                    size = 13,
                                    weight = "medium",
                                    lineHeight = 16,
                                    color = "text-chrome",
                                    truncate = true,
                                    shrink = true,
                                    minW = 0,
                                }),
                                ui.icon("select-chevron", { size = 16, color = "icon-weak" }),
                            }),
                        }),
                        ui.iconButton({
                            key = `site-perm-remove-{idx}`,
                            icon = "outline-trash",
                            variant = "ghost-muted",
                            size = "small",
                            label = i18n:T("settings.sites.remove", { permission = permLabel, site = host }),
                            help = i18n:T("settings.sites.remove", { permission = permLabel, site = host }),
                            onPress = view:Handler(function()
                                env.removeSitePermission(origin, perm)
                            end),
                        }),
                    })
                    -- `settings.tsx:326-336`: the globe, the site, and the feature under it.
                    table.insert(siteRows, identityRow(
                        `site-perm-{idx}`,
                        if idx == 1 then "settings-browser-sites" else nil,
                        "outline-globe",
                        nil,
                        host,
                        nil,
                        { text = permLabel, error = false },
                        controls
                    ))
                end
            end
            -- `SettingsSection` footer: the notes sit 16 under the card, 4 apart.
            local notes: { ui.Node } = { ui.text(i18n:T("settings.sites.note"), NOTE) }
            if env.permissionRestartNote and env.permissionRestartNote() then
                table.insert(notes, ui.text(i18n:T("settings.sites.restartNote"), NOTE))
            end
            table.insert(sections, ui.col({ key = "settings-browser-sites-section", w = "full", gap = 16 }, {
                ui.settingsList({ key = "settings-browser-sites-list", title = i18n:T("settings.sites.title") }, siteRows),
                ui.col({ w = "full", gap = 4 }, notes),
            }))

            -- 4d. Browser data section (`settings-browser-cookies`, UP-117 #65124 confirm before clearing):
            -- how the last clear went, else the cookie count (`settings.tsx:259-268`).
            local count = env.cookieCount()
            local dataDesc = env.clearNotice()
                or (if count == nil then nil else i18n:T(if count == 1 then "settings.cookies.count.one" else "settings.cookies.count.other", { count = count }))
            table.insert(sections, ui.settingsList({ key = "settings-browser-data-section", title = i18n:T("settings.data.title") }, {
                ui.settingsRow(
                    {
                        id = "settings-browser-cookies",
                        title = i18n:T("settings.cookies.title"),
                        description = dataDesc,
                    },
                    ui.button({
                        key = "settings-browser-clear-data",
                        text = i18n:T("settings.cookies.clear"),
                        size = "small",
                        variant = "danger",
                        loading = env.clearing(),
                        onPress = view:Handler(function()
                            if env.clearing() then
                                return
                            end
                            env.requestClearBrowsingData()
                        end),
                    })
                ),
            }))
        end

        -- `SettingsLayout`: sections 32 apart.
        return ui.col({ key = "settings-browser-page", w = "full", gap = 32 }, sections)
    end

    return { render = render }
end

local DIALOG_TITLE: UiTextProps = table.freeze({ size = 15, weight = "medium", lineHeight = 20, color = "text-chrome", noWrap = true } :: UiTextProps)

-- Confirmation dialog body for Clear browsing data (`UP-117` `#65124`).
function settings.renderClearConfirm(i18n: I18nService, view: View, onCancel: () -> (), onConfirm: () -> ()): ui.Node
    return ui.col({ key = "settings-browser-clear-confirm", w = "full", pad = 20, gap = 12 }, {
        ui.text(i18n:T("settings.clear.confirm.title"), DIALOG_TITLE),
        ui.text(i18n:T("settings.clear.confirm.description"), MUTED),
        ui.row({ w = "full", justify = "end", gap = 8, pt = 4 }, {
            ui.button({
                key = "settings-browser-clear-cancel",
                text = i18n:T("common.cancel"),
                size = "small",
                variant = "ghost",
                autofocus = true,
                onPress = view:Handler(function()
                    onCancel()
                end),
            }),
            ui.button({
                key = "settings-browser-clear-confirm-btn",
                text = i18n:T("settings.clear.confirm.action"),
                size = "small",
                variant = "danger",
                onPress = view:Handler(function()
                    onConfirm()
                end),
            }),
        }),
    })
end

-- What the import dialog brings: both boxes ticked until the person unticks one (`site-chooser.tsx:86-87`).
export type ImportBoxes = { signins: boolean, history: boolean }

-- One registrable domain the browser profile has cookies for (`profile/ipc.ts` `ProfileSite`).
export type ProfileSite = { site: string, cookies: number }

-- The dialog's title (`site-chooser.tsx:358-373`): with one profile to offer, "Import from <browser>"
-- and, for a browser that has several, the profile's name as a badge; with a profile picker, which
-- names the browser itself, "Import from another browser".
export type ImportTitle = { text: string, badge: string? }

-- The "Sites" group (`site-chooser.tsx:443-660`) as init.luau keeps it for the open dialog: the
-- choice between every site and the ticked ones, and under the second the search, the box that
-- ticks every shown site with the count, and the list.
export type SiteChooser = {
    -- Every site comes over (`all`), or only the ticked ones (`some`), which start with none
    -- (`ChooserState.scope`).
    scope: string,
    -- The import brings every site: "All sites" is picked, or every site is ticked.
    everything: boolean,
    chosen: number,
    total: number,
    shown: { ProfileSite },
    shownChosen: number,
    isChosen: (site: string) -> boolean,
    query: string,
    editor: Editor,
    -- The list was read and has sites; the count beside "All sites" and the list's header show only then.
    ready: boolean,
    -- In the list's place: loading, failed (with Try again), empty or unmatched.
    status: { text: string, error: boolean, loading: boolean?, retry: boolean? }?,
    -- Every browser profile, for the "From" picker: only where the dialog has several to offer, as
    -- the pane's offer and a passkey notice do; nil from a Settings row, which picked its profile.
    profiles: { { value: string, label: string } }?,
    profile: string,
    -- Changes when the list's rows do.
    version: string,
    -- Import may start as far as the sites go: some chosen, or every site and a list not read empty.
    canImport: boolean,
    Menu: MenuService,
    onScope: (scope: string) -> (),
    onToggle: (site: string) -> (),
    onSelectShown: () -> (),
    onClearShown: () -> (),
    onRetry: () -> (),
    onProfile: (value: string) -> (),
    -- Opened for one site's sign-in, from a page's passkey notice: no boxes and no choice, only the
    -- list, and sign-ins only (`site-chooser.tsx:404`, `:447`, `:259-261`).
    single: boolean,
    -- Searched for that site, which this profile has no cookies for: what the count line says while
    -- sites that only contain its name show (`site-chooser.tsx:138-145`, `:539-556`).
    missing: string?,
}

-- `site-chooser.tsx:27` `ROW_HEIGHT`.
local SITE_ROW = 32
-- `site-chooser.tsx:24` `HEADING`: a group's heading, 12 at 440 on a 20 line, muted.
local GROUP_HEADING: UiTextProps = table.freeze({ size = 12, weight = "regular", lineHeight = 20, color = "text-muted", noWrap = true } :: UiTextProps)
-- Set below the import dialog, which draws it.
local siteList: (i18n: I18nService, view: View, chooser: SiteChooser, busy: boolean, onImport: () -> ()) -> ui.Node

-- "Import from <browser>" (`site-chooser.tsx:354-697`), in two groups: what comes over, as two boxes,
-- "Sign-ins" and "Browsing history", and from which sites, "All sites" or "Only sites I choose"; the
-- second shows the sites under it, none ticked, and the button then names how many sites it brings.
-- Then Cancel and Import. Import needs at least one box; while it runs nothing can change and Import
-- shows it is busy. A clean import closes the dialog and its row says so; a partial or failed one
-- stays open and says why. The dialog is `tall` and fits: its top edge stays, and showing the sites
-- only grows it downward, up to its size's height, where the list gives way and scrolls.
function settings.renderImportDialog(
    i18n: I18nService,
    view: View,
    title: ImportTitle,
    message: ImportMessage?,
    boxes: ImportBoxes,
    busy: boolean,
    onBox: (which: string, on: boolean) -> (),
    onImport: () -> (),
    onClose: () -> (),
    chooser: SiteChooser?
): ui.Node
    local function box(which: string, label: string, checked: boolean): ui.Node
        -- `site-chooser.tsx:417-438`: each box in a 28 row, on the dialog's bg-base.
        return ui.row({ key = `settings-import-{which}-row`, w = "full", h = 28, align = "center" }, {
            ui.checkbox({
                key = `settings-import-{which}`,
                text = label,
                checked = checked,
                surface = "base",
                disabled = busy,
                onChange = view:Handler(function(e: any)
                    onBox(which, e.checked == true)
                end),
            }),
        })
    end
    local single = chooser ~= nil and chooser.single
    local canImport = (single or boxes.signins or boxes.history) and not busy and (chooser == nil or chooser.canImport)
    local function import()
        if canImport then
            onImport()
        end
    end
    -- The body's groups, 16 apart (`site-chooser.tsx:378`): where it comes from, what comes over,
    -- which sites, then how the last import went.
    local body: { ui.Node } = {}
    local profiles = if chooser then chooser.profiles else nil
    if chooser and profiles then
        local current = chooser.profile
        local selectOptions: { ui.UiSelectOption } = {}
        for _, option in profiles do
            table.insert(selectOptions, { value = option.value, label = option.label })
        end
        -- `site-chooser.tsx:380-402`: "From", then the profile in Desktop's `Select` control.
        table.insert(body, ui.row({ key = "settings-import-from", w = "full", gap = 8, align = "center" }, {
            ui.text(i18n:T("import.from"), { size = 13, weight = "regular", lineHeight = 20, color = "text-muted", noWrap = true }),
            ui.select({
                key = "settings-import-profile",
                label = i18n:T("sites.profile"),
                value = current,
                options = selectOptions,
                disabled = busy,
                shrink = true,
                minW = 0,
                onChange = view:Handler(function(ev: any)
                    if type(ev) == "table" and type(ev.value) == "string" and ev.value ~= current then
                        chooser.onProfile(ev.value)
                    end
                end),
            }),
        }))
    end
    if not single then
        -- `site-chooser.tsx:405-439`: the heading and the two boxes, 4 apart; Return on a box
        -- imports, as it does on the list.
        table.insert(body, ui.frame({
            key = "settings-import-what",
            w = "full",
            keys = { "Enter" },
            onKey = view:Handler(import),
        }, { ui.col({ key = "settings-import-what-rows", w = "full", gap = 4 }, {
            ui.text(i18n:T("import.what"), GROUP_HEADING, { key = "settings-import-what-title", role = "heading" }),
            box("signins", i18n:T("import.signins"), boxes.signins),
            box("history", i18n:T("import.history"), boxes.history),
        }) }))
    end
    if chooser then
        local group: { ui.Node } = {}
        if not single then
            table.insert(group, ui.text(i18n:T("import.sites"), GROUP_HEADING, { key = "settings-import-sites-title", role = "heading" }))
            -- `site-chooser.tsx:451-469`: the two choices 8 apart, 4 above and under them; "All
            -- sites" names how many once the list is read. Each choice is as wide as its words
            -- (`radio.css`: the item is a row of its control and label), so the row beside it
            -- takes no press. Each stands in a row of its own: a choice set straight into this
            -- column is laid out as wide as its ring, its words drawn outside its box.
            table.insert(group, ui.col({ key = "settings-import-scope", w = "full", py = 4, gap = 8, align = "start" }, {
                ui.row({ key = "settings-import-scope-all-row", gap = 6, align = "start" }, {
                    ui.radio({
                        key = "settings-import-scope-all",
                        text = i18n:T("sites.every"),
                        checked = chooser.scope == "all",
                        disabled = busy,
                        onChange = view:Handler(function()
                            chooser.onScope("all")
                        end),
                    }),
                    if chooser.ready
                        then ui.text(i18n:Number(chooser.total), { key = "settings-import-scope-count", size = 13, weight = "regular", lineHeight = 20, color = "text-muted", noWrap = true })
                        else nil,
                } :: { any }),
                ui.row({ key = "settings-import-scope-some-row", align = "start" }, {
                    ui.radio({
                        key = "settings-import-scope-some",
                        text = i18n:T("sites.only"),
                        checked = chooser.scope == "some",
                        disabled = busy,
                        onChange = view:Handler(function()
                            chooser.onScope("some")
                        end),
                    }),
                }),
            }))
        end
        if chooser.scope == "some" then
            table.insert(group, siteList(i18n, view, chooser, busy, import))
        end
        table.insert(body, ui.col({ key = "settings-import-sites-group", w = "full", gap = 4, shrink = true, minH = 0 }, group))
    end
    if message ~= nil then
        -- `site-chooser.tsx:661-674`: 13 on a 20 line, 4 above and under.
        table.insert(body, ui.text(message.text, MUTED, {
            key = "settings-import-status",
            role = if message.error then "alert" else "status",
            color = if message.error then "status-danger" else "text-muted",
            py = 4,
        }))
    end
    -- `site-chooser.tsx:691-693`: how many sites, once some but not all are chosen.
    local action = if chooser == nil or chooser.everything or chooser.chosen == 0
        then i18n:T("signins.action")
        else i18n:T(if chooser.chosen == 1 then "import.some.one" else "import.some.other", { number = i18n:Number(chooser.chosen) })
    -- The dialog's frame (`Dialog size="tall" fit`): the header with its close button, the body 16
    -- in and 4 under the header, the footer's buttons 16 in on every side (`dialog.css:134-143`).
    -- The body and what holds the list may shrink, so at the size's height the list gives way.
    return ui.col({ key = "settings-browser-import-dialog", w = "full", shrink = true, minH = 0 }, {
        ui.dialogHeader({
            key = "settings-import-header",
            title = title.text,
            badge = title.badge,
            wrapTitle = false,
        }),
        ui.col({ key = "settings-import-body", w = "full", px = 16, pt = 4, gap = 16, shrink = true, minH = 0 }, body),
        ui.row({ key = "settings-import-footer", w = "full", pad = 16, gap = 8, justify = "end", align = "center" }, {
            ui.button({
                key = "settings-import-cancel",
                text = i18n:T("common.cancel"),
                variant = "neutral",
                onPress = view:Handler(function()
                    onClose()
                end),
            }),
            ui.button({
                key = "settings-import-start",
                text = action,
                variant = "primary",
                disabled = not canImport or busy,
                loading = busy,
                onPress = view:Handler(import),
            }),
        }),
    })
end

-- The sites under "Only sites I choose" (`site-chooser.tsx:471-659`): the search, then in one frame
-- the box that ticks or clears every shown site beside the count, and the list, one row per site
-- with its box, or in its place what is going on (loading, failed, empty, unmatched). Under the
-- choice it starts at the choice's label, 24 in; opened for one site it starts at the body's edge.
siteList = function(i18n: I18nService, view: View, chooser: SiteChooser, busy: boolean, onImport: () -> ()): ui.Node
    local searching = chooser.query ~= ""
    local shown = chooser.shown
    local count = if chooser.missing ~= nil and #shown > 0
        then chooser.missing
        elseif searching
        then i18n:T("sites.selected.shown", { shown = i18n:Number(#shown), selected = i18n:Number(chooser.chosen) })
        else i18n:T("sites.selected", { selected = i18n:Number(chooser.chosen), total = i18n:Number(chooser.total) })
    -- `site-chooser.tsx:347-348`, `:523-533`: ticked when every shown site is, a dash when some are;
    -- a press ticks every shown site, or clears them when all are ticked. It is named by what it
    -- does, so the count beside it, which alone is announced, is not its name.
    local allChosen = #shown > 0 and chooser.shownChosen == #shown
    local someChosen = chooser.shownChosen > 0 and not allChosen
    local header = ui.row({ key = "settings-import-counts", w = "full", h = 36, px = 12, gap = 8, align = "center", opacity = if chooser.ready then 1 else 0 }, {
        ui.checkbox({
            key = "settings-import-all",
            text = "",
            label = if searching then i18n:T("sites.all.shown", { count = i18n:Number(#shown) }) else i18n:T("sites.all"),
            checked = allChosen,
            indeterminate = someChosen,
            surface = "layer-1",
            disabled = busy or not chooser.ready or #shown == 0,
            onChange = view:Handler(function()
                if allChosen then
                    chooser.onClearShown()
                else
                    chooser.onSelectShown()
                end
            end),
        }),
        ui.text(count, { key = "settings-import-count", size = 13, weight = "regular", lineHeight = 20, color = "text-muted", truncate = true, shrink = true, minW = 0, grow = 1, role = "status" }),
    })
    local status = chooser.status
    local inner: ui.Node = if status
        then ui.col({ key = "settings-import-sites-status", w = "full", px = 24, gap = 12, align = "center", justify = "center" }, {
            ui.row({ key = "settings-import-sites-status-line", gap = 8, align = "center" }, {
                if status.loading then ui.loader({ key = "settings-import-sites-loader", size = "small" }) else nil,
                ui.text(status.text, {
                    key = "settings-import-sites-message",
                    size = 13,
                    weight = "regular",
                    lineHeight = 20,
                    color = if status.error then "status-danger" else "text-muted",
                    role = if status.error then "alert" else "status",
                    wrap = true,
                }),
            } :: { any }),
            if status.retry
                then ui.button({
                    key = "settings-import-sites-retry",
                    text = i18n:T("common.retry"),
                    size = "small",
                    variant = "neutral",
                    onPress = view:Handler(function()
                        chooser.onRetry()
                    end),
                })
                else nil,
        } :: { any })
        -- One Tab stop, as official's listbox (`site-chooser.tsx:309-343`): Up and Down move between the
        -- sites' boxes and stop at the ends, Home and End go to the ends, a typed name moves to its site,
        -- Space ticks or unticks the focused one, and Return imports. The list is as tall as its
        -- rows (`:576`, the virtualizer's total with 4 above and under) and gives way from there.
        else ui.listbox({
            key = "settings-import-sites",
            label = i18n:T("sites.list"),
            focusGroup = true,
            keys = { "Enter" },
            onKey = view:Handler(function()
                onImport()
            end),
            w = "full",
            shrink = true,
            minH = 0,
        }, { ui.virtualList({
            key = "settings-import-sites-rows",
            w = "full",
            h = #shown * SITE_ROW + 8,
            shrink = true,
            minH = 0,
            px = 4,
            py = 4,
            count = #shown,
            estimatedRowHeight = SITE_ROW,
            version = chooser.version,
            rowKey = function(index: number): string
                return shown[index].site
            end,
            renderRow = function(index: number): ui.Node
                local site = shown[index].site
                local chosen = chooser.isChosen(site)
                return ui.option({
                    direction = "row",
                    key = `settings-import-site-{site}`,
                    selected = chosen,
                    label = site,
                    w = "full",
                    h = SITE_ROW,
                    px = 8,
                    gap = 8,
                    radius = 6,
                    align = "center",
                    hover = "surface-hover",
                    hoverTransition = "instant",
                    onPress = view:Handler(function()
                        chooser.onToggle(site)
                    end),
                }, {
                    ui.checkbox({ key = `settings-import-site-box-{site}`, text = "", checked = chosen, surface = "layer-1", presentation = true }),
                    ui.text(site, { size = 13, weight = "regular", lineHeight = 20, color = "text-base", truncate = true, shrink = true, minW = 0, grow = 1, dir = "ltr" }),
                })
            end,
        }) })
    return ui.col({ key = "settings-import-sites-area", w = "full", shrink = true, minH = 0, gap = 8, pb = 2, pl = if chooser.single then 0 else 24 }, {
        ui.field({
            key = "settings-import-search",
            editor = chooser.editor,
            label = i18n:T("sites.search"),
            placeholder = i18n:T("sites.search"),
            icon = "magnifying-glass",
            clearable = searching,
            -- Down goes on to the list, once it shows sites (`site-chooser.tsx:507-512`).
            focusDown = if chooser.status == nil and #shown > 0 then "settings-import-sites" else nil,
            size = "large",
            w = "full",
            mt = 4,
            keys = if searching then { "Escape" } else nil,
        }),
        -- `site-chooser.tsx:516-518`: the frame on layer-01 with a hairline, its header 36 over a
        -- hairline of its own.
        ui.col({
            key = "settings-import-sites-frame",
            w = "full",
            shrink = true,
            minH = 0,
            radius = 8,
            fill = "surface-1",
            border = "border-weak",
            clip = true,
        }, {
            header,
            ui.divider({ key = "settings-import-counts-rule" }),
            inner,
        }),
    })
end

return settings
