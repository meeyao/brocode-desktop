--!strict
-- What's New after an update (`whats-new-machine.ts`, `whats-new.tsx`): at startup an unchanged
-- version shows nothing; the first run and disabled release notes only remember the version;
-- otherwise the channel's notes load: on beta those the build carries since the commit seen last,
-- elsewhere the changelog's desktop highlights since the version seen last. It only moves
-- forward, so results match on the phase.

local whatsnew = {}

-- Bounds the changelog request, so a stalled connection never opens a dialog mid-session.
whatsnew.TIMEOUT = 15

whatsnew.CHANGELOG_URL = ""

export type Highlight = { title: string, description: string, media: { type: string, src: string, alt: string? }? }

export type Content = { highlights: { Highlight } }

export type Init = {
    version: string?,
    previousVersion: string?,
    enabled: boolean,
}

export type State = { status: "loading", version: string } | { status: "done" }

export type Event =
    { type: "loaded", content: Content? }
    | { type: "load.failed" }
    | { type: "load.timed-out" }
    | { type: "disabled" }
    | { type: "disposed" }

export type Effect =
    { type: "load", previousVersion: string, version: string, timeout: number }
    | { type: "abort-load" }
    | { type: "mark-seen", version: string }
    | { type: "open", content: Content }

local DONE: State = { status = "done" }

function whatsnew.init(input: Init): (State, { Effect })
    local version = input.version
    if not version or input.previousVersion == version then
        return DONE, {}
    end
    if not input.previousVersion or not input.enabled or whatsnew.CHANGELOG_URL == "" then
        return DONE, { { type = "mark-seen", version = version } }
    end
    local previous = input.previousVersion :: string
    return { status = "loading", version = version },
        { { type = "load", previousVersion = previous, version = version, timeout = whatsnew.TIMEOUT } }
end

function whatsnew.transition(state: State, event: Event): (State, { Effect })
    if state.status == "done" then
        return state, {}
    end
    -- A failed or abandoned load leaves the release unseen, so the next start tries again.
    if event.type == "load.failed" or event.type == "load.timed-out" or event.type == "disposed" then
        return DONE, { { type = "abort-load" } }
    end
    local seen: { Effect } = { { type = "abort-load" }, { type = "mark-seen", version = state.version } }
    local content = if event.type == "loaded" then event.content else nil
    if not content or #content.highlights == 0 then
        return DONE, seen
    end
    table.insert(seen, { type = "open", content = content })
    return DONE, seen
end

-- A trimmed, non-empty string, or a number written out.
local function text(value: any): string?
    if type(value) == "number" then
        return tostring(value)
    end
    if type(value) ~= "string" then
        return nil
    end
    local trimmed = string.match(value, "^%s*(.-)%s*$") :: string
    return if trimmed == "" then nil else trimmed
end

local function highlight(item: any): Highlight?
    if type(item) ~= "table" then
        return nil
    end
    local title = text(item.title)
    local description = text(item.description) or text(item.shortDescription)
    if not title or not description then
        return nil
    end
    local media = if type(item.media) == "table" then item.media else nil
    local kind = media and text(media.type)
    local src = media and (text(media.src) or text(media.url))
    kind = kind and string.lower(kind)
    local shown = if src and (kind == "image" or kind == "video") then { type = kind :: string, src = src, alt = title } else nil
    return { title = title, description = description, media = shown }
end

local function normalize(value: string?): string?
    local v = text(value)
    if not v then
        return nil
    end
    return if string.sub(v, 1, 1) == "v" or string.sub(v, 1, 1) == "V" then string.sub(v, 2) else v
end

-- The desktop highlights of the releases from `current` back to `previous`, at most 5, each once.
function whatsnew.highlights(changelog: any, current: string, previous: string): { Highlight }
    local releases = if type(changelog) == "table" and type(changelog.releases) == "table" then changelog.releases else changelog
    if type(releases) ~= "table" then
        return {}
    end
    local list = {}
    for _, release in releases do
        if type(release) ~= "table" then
            continue
        end
        local items = {}
        for _, group in (if type(release.highlights) == "table" then release.highlights else {}) do
            local source = type(group) == "table" and text(group.source)
            if not source or not string.find(string.lower(source), "desktop", 1, true) then
                continue
            end
            if type(group.items) == "table" then
                for _, item in group.items do
                    local h = highlight(item)
                    if h then
                        table.insert(items, h)
                    end
                end
            else
                local h = highlight(group)
                if h then
                    table.insert(items, h)
                end
            end
        end
        table.insert(list, { tag = text(release.tag) or text(release.tag_name) or text(release.name), highlights = items })
    end
    local now, before = normalize(current), normalize(previous)
    local start = 1
    for i, release in list do
        if now and normalize(release.tag) == now then
            start = i
            break
        end
    end
    local finish = #list + 1
    for i = start, #list do
        if before and normalize(list[i].tag) == before then
            finish = i
            break
        end
    end
    local seen, out = {}, {}
    for i = start, finish - 1 do
        for _, h in list[i].highlights do
            local key = `{h.title}\n{h.description}\n{h.media and h.media.type or ""}\n{h.media and h.media.src or ""}`
            if not seen[key] and #out < 5 then
                seen[key] = true
                table.insert(out, h)
            end
        end
    end
    return out
end

return whatsnew
