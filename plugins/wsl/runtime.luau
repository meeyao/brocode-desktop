--!strict
-- WSL's commands and what they print (Desktop's `runtime.ts`, `remote-cli.ts`): `wsl.exe`'s
-- arguments, the scripts run inside a distro, and the parsers for `--list --verbose`,
-- `--list --online` and `opencode --version`.

local runtime = {}

-- How long a `wsl.exe` command may take: long enough for a cold start, short enough to fail
-- fast on a wedged distro.
runtime.TIMEOUT = 20
-- Installs (WSL, a distro, Brocode).
runtime.INSTALL_TIMEOUT = 15 * 60
-- How long a started server has to answer `/api/info`.
runtime.HEALTH_TIMEOUT = 30

-- `wslArgs`: `--exec` runs the command itself; `--` would hand it to the distro's shell, which
-- expands `$VAR` before `sh -lc` runs the script.
function runtime.inDistro(args: { string }, distro: string?): { string }
    local out = {}
    if distro then
        table.insert(out, "-d")
        table.insert(out, distro)
    end
    table.insert(out, "--exec")
    for _, a in args do
        table.insert(out, a)
    end
    return out
end

function runtime.sh(script: string, distro: string): { string }
    return runtime.inDistro({ "sh", "-lc", script }, distro)
end

-- Single quotes for a POSIX shell.
function runtime.quote(value: string): string
    return "'" .. string.gsub(value, "'", "'\"'\"'") .. "'"
end

-- The managed CLI's path, when installed.
runtime.DISCOVER_SCRIPT = 'cli=""\nif [ -z "$cli" ] && [ -x "$HOME/.opencode/bin/opencode" ]; then cli="$HOME/.opencode/bin/opencode"; fi\nif [ -n "$cli" ]; then printf \'%s\\n\' "$cli"; fi\n'

function runtime.versionScript(command: string): string
    return `if [ -x {command} ]; then {command} --version 2>/dev/null || true; fi\n`
end

local function validVersion(version: string): boolean
    return version == "local" or (string.match(version, "^%d") ~= nil and string.match(version, "^[%w%.%+%-]+$") ~= nil)
end

-- The managed CLI installer, which also puts it on the shell's PATH, then a check that what it
-- installed runs.
function runtime.installScript(version: string): string?
    if not validVersion(version) then
        return nil
    end
    return "set -eu\ncurl -fsSL https://raw.githubusercontent.com/anomalyco/opencode/v2/install | bash -s -- --version "
        .. runtime.quote(version)
        .. "\n\"$HOME/.opencode/bin/opencode\" --version >/dev/null\n"
end

-- The server's script: off the Windows PATH, a password, no file watcher, `serve` on `port`.
function runtime.sidecarScript(opencode: string, password: string, port: number, packaged: boolean): string
    return table.concat({
        "set -euo pipefail",
        'cd "$HOME" || cd /',
        "PATH=$(awk -v RS=: -v ORS=: '$0 !~ /^\\/mnt\\//' <<<\"$PATH\" | sed \"s/:$//\")",
        "export PATH",
        "export WSLENV=",
        "export OPENCODE_EXPERIMENTAL_DISABLE_FILEWATCHER=true",
        "export OPENCODE_CLIENT=desktop",
        `export OPENCODE_SERVER_PASSWORD={runtime.quote(password)}`,
        'export XDG_STATE_HOME="$HOME/.local/state"',
        `exec {runtime.quote(opencode)} --log-level {if packaged then "warn" else "info"} serve --hostname 0.0.0.0 --port {port}`,
    }, "\n")
end

local function trim(s: string): string
    return (string.match(s, "^%s*(.-)%s*$") or "")
end

-- Non-empty lines, trimmed, joined.
function runtime.summarize(value: string): string
    local lines = {}
    for line in string.gmatch(value, "[^\r\n]+") do
        local t = trim(line)
        if #t > 0 then
            table.insert(lines, t)
        end
    end
    return table.concat(lines, "\n")
end

function runtime.firstLine(value: string): string?
    for line in string.gmatch(value, "[^\r\n]+") do
        local t = trim(line)
        if #t > 0 then
            return t
        end
    end
    return nil
end

-- `opencode --version`'s version: the word after its last ` v`, or the line.
function runtime.parseVersion(output: string): string?
    local line = runtime.firstLine(output)
    if not line then
        return nil
    end
    local at: number? = nil
    local from = 1
    while true do
        local found = string.find(line, " v", from, true)
        if not found then
            break
        end
        at = found
        from = found + 1
    end
    local version = if at then string.sub(line, at + 2) else line
    return if #version > 0 then version else nil
end

-- A row's first column and the rest, split at the first run of two or more spaces.
local function splitColumns(line: string): (string?, string?)
    local at = string.find(line, "  ", 1, true)
    if not at then
        return nil, nil
    end
    local head, tail = trim(string.sub(line, 1, at - 1)), trim(string.sub(line, at))
    if #head == 0 or #tail == 0 then
        return nil, nil
    end
    return head, tail
end

-- `wsl --list --verbose`: `* NAME  STATE  VERSION` rows, the default marked.
function runtime.parseInstalled(output: string)
    local out = {}
    for raw in string.gmatch(output, "[^\r\n]+") do
        local line = trim(raw)
        if #line > 0 then
            local default = string.sub(line, 1, 1) == "*"
            local rest = trim((string.gsub(line, "^%*+", "")))
            local name, tail = splitColumns(rest)
            if name and tail and string.lower(name) ~= "name" then
                local words = {}
                for w in string.gmatch(tail, "%S+") do
                    table.insert(words, w)
                end
                if #words == 2 then
                    table.insert(out, { name = name, version = tonumber(words[2]), isDefault = default })
                end
            end
        end
    end
    return out
end

-- `wsl --list --online`: `NAME  FRIENDLY NAME` rows after the preamble.
function runtime.parseOnline(output: string)
    local out = {}
    for raw in string.gmatch(output, "[^\r\n]+") do
        local name, label = splitColumns(trim(raw))
        if name and label and string.lower(name) ~= "name" and string.match(name, "^[%w%._%-]+$") then
            table.insert(out, { name = name, label = label })
        end
    end
    return out
end

-- Why a finished command failed, or nil when it ran fine (`requireSuccess`).
function runtime.failed(out: ProcessOutput, fallback: string): string?
    if out.ok then
        return nil
    end
    if out.error then
        return out.error
    end
    if out.timedOut then
        return fallback
    end
    local text = runtime.summarize(if #trim(out.stderr) == 0 then out.stdout else out.stderr)
    return if #text > 0 then text else fallback
end

return runtime
