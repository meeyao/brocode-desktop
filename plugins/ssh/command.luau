--!strict
-- `ssh` itself (Desktop's `command.ts`, `remote-cli.ts`, `bootstrap.ts`): the target a user
-- types, the options every connection takes, the tunnel's, and the scripts run on the host to
-- find, install and start its Brocode service and read its registration.

local decode = require("./decode")

local command = {}

export type Code = "connection" | "input" | "platform" | "install" | "service" | "host-key" | "ssh-missing" | "unpublished"
export type Failure = { code: Code, detail: string }
export type Target = { host: string, args: { string } }

function command.failure(code: Code, detail: string?): Failure
    return { code = code, detail = detail or "" }
end

function command.quote(value: string): string
    return "'" .. string.gsub(value, "'", "'\\''") .. "'"
end

local OPTIONS = {
    hostname = true,
    user = true,
    port = true,
    identityfile = true,
    identityagent = true,
    identitiesonly = true,
    proxyjump = true,
    proxycommand = true,
    connecttimeout = true,
    addressfamily = true,
}

local function isSpace(c: string): boolean
    return string.match(c, "^%s$") ~= nil
end

-- `parseTarget`: `host`, `user@host`, or an `ssh` command with only connection options
-- (`-p -l -i -F -J`, `-o` for a few, `-4 -6 -C -A -a`) and no remote command. Nil when it isn't one.
function command.parseTarget(input: string): Target?
    local tokens: { string } = {}
    local word, quote, started = "", nil :: string?, false
    local chars: { string } = {}
    for _, cp in utf8.codes(input) do
        table.insert(chars, utf8.char(cp))
    end
    local i = 1
    while i <= #chars do
        local c = chars[i]
        if c == "\n" or c == "\r" or c == "\0" then
            return nil
        end
        local nextc = chars[i + 1]
        if c == "\\" and quote ~= "'" and nextc ~= nil and (isSpace(nextc) or nextc == "\\" or nextc == '"' or nextc == "'") then
            word ..= nextc
            started = true
            i += 2
            continue
        end
        if quote then
            if c == quote then
                quote = nil
            else
                word ..= c
            end
        elseif c == "'" or c == '"' then
            quote = c
            started = true
        elseif isSpace(c) then
            if started then
                table.insert(tokens, word)
            end
            word, started = "", false
        else
            word ..= c
            started = true
        end
        i += 1
    end
    if quote then
        return nil
    end
    if started then
        table.insert(tokens, word)
    end
    if tokens[1] == "ssh" then
        table.remove(tokens, 1)
    end
    local args = {}
    while tokens[1] and string.sub(tokens[1], 1, 1) == "-" do
        local token = table.remove(tokens, 1) :: string
        if token == "-4" or token == "-6" or token == "-C" or token == "-A" or token == "-a" then
            table.insert(args, token)
            continue
        end
        local flag = string.sub(token, 1, 2)
        if not (flag == "-p" or flag == "-l" or flag == "-i" or flag == "-F" or flag == "-J" or flag == "-o") then
            return nil
        end
        local value = if #token > 2 then string.sub(token, 3) else table.remove(tokens, 1)
        if not value or #value == 0 or string.sub(value, 1, 1) == "-" then
            return nil
        end
        if flag == "-p" then
            local n = tonumber(value)
            if not string.match(value, "^%d+$") or not n or n < 1 or n > 65535 then
                return nil
            end
        end
        if flag == "-o" then
            local key = string.lower(string.match(value, "^[^=%s]*") or "")
            if not OPTIONS[key] then
                return nil
            end
        end
        table.insert(args, flag)
        table.insert(args, value)
    end
    local host = tokens[1]
    if #tokens ~= 1 or not host or not string.match(host, "^[%w_@%.:%[%]%%%-]+$") or string.sub(host, 1, 1) == "-" then
        return nil
    end
    local at = nil
    local from = 1
    while true do
        local f = string.find(host, "@", from, true)
        if not f then
            break
        end
        at, from = f, f + 1
    end
    if at and string.find(string.sub(host, 1, at - 1), ":", 1, true) then
        return nil
    end
    return { host = host, args = args }
end

-- `sshArgs`: every connection's options, then the target's.
function command.sshArgs(target: Target): { string }
    local a = { "-T", "-o", "ConnectTimeout=10", "-o", "ServerAliveInterval=15", "-o", "ServerAliveCountMax=3", "-o", "RemoteCommand=none", "-o", "RequestTTY=no", "-o", "PermitLocalCommand=no" }
    for _, x in target.args do
        table.insert(a, x)
    end
    return a
end

-- `tunnelArgs`: forwards `127.0.0.1:local` to the remote service and keeps a session open on
-- stdin, which the tunnel's process owns.
function command.tunnelArgs(target: Target, localPort: number, remoteHost: string, remotePort: number): { string }
    local a = { "-o", "ControlMaster=no", "-o", "ControlPersist=no" }
    for _, x in command.sshArgs(target) do
        table.insert(a, x)
    end
    for _, x in { "-o", "ExitOnForwardFailure=yes", "-L", `127.0.0.1:{localPort}:{remoteHost}:{remotePort}`, target.host, "sh -c 'exec cat >/dev/null'" } do
        table.insert(a, x)
    end
    return a
end

local function trim(s: string): string
    return (string.match(s, "^%s*(.-)%s*$") or "")
end

-- What a failed command said, without the service's private registration.
function command.failureDetail(code: number?, stdout: string, stderr: string): string
    local out = string.gsub(stdout, "OPENCODE_SSH_REGISTRATION_BEGIN.-OPENCODE_SSH_REGISTRATION_END", "")
    out = string.gsub(out, "OPENCODE_SSH_REGISTRATION_BEGIN.*$", "")
    local parts = {}
    for _, p in { trim(stderr), trim(out) } do
        if #p > 0 then
            table.insert(parts, p)
        end
    end
    if #parts == 0 then
        return '{"exitCode":' .. (if code ~= nil then tostring(code) else "null") .. "}"
    end
    return table.concat(parts, "\n")
end

local REGISTRATION = [[status=$("$cli" service status) || exit 0
if [ "$status" = stopped ]; then exit 0; fi
printf 'OPENCODE_SSH_STATUS=%s\n' "$status"
for file in "${XDG_STATE_HOME:-$HOME/.local/state}"/opencode/service*.json; do
  if [ ! -f "$file" ]; then continue; fi
  printf 'OPENCODE_SSH_REGISTRATION_BEGIN\n'
  cat "$file"
  printf '\nOPENCODE_SSH_REGISTRATION_END\n'
done
]]

-- The CLI on PATH, the managed one, or one this app installed, then its service's registration.
function command.discoverScript(): string
    return 'set -eu\ncli=$(command -v opencode || true)\nif [ -z "$cli" ] && [ -x "$HOME/.opencode/bin/opencode" ]; then cli="$HOME/.opencode/bin/opencode"; fi\nif [ -z "$cli" ]; then\n  for binary in "$HOME"/\'.opencode/desktop-ssh\'/*/opencode; do\n    if [ -x "$binary" ]; then cli="$binary"; fi\n  done\nfi\nif [ -n "$cli" ]; then printf \'%s\\n\' "$cli"; fi\nif [ -z "$cli" ]; then exit 0; fi\n'
        .. REGISTRATION
end

function command.validVersion(version: string): boolean
    return version == "local" or (string.match(version, "^%d") ~= nil and string.match(version, "^[%w%.%+%-]+$") ~= nil)
end

function command.binaryPath(version: string): string
    return `$HOME/.opencode/desktop-ssh/{version}/opencode`
end

function command.startScript(version: string): string
    return `set -eu\ncli="{command.binaryPath(version)}"\n"$cli" service start\n` .. REGISTRATION
end

function command.versionScript(cmd: string): string
    return `if [ -x {cmd} ]; then {cmd} --version 2>/dev/null || true; fi\n`
end

command.PROBE_SCRIPT =
    'set -eu\nos=$(uname -s | tr \'[:upper:]\' \'[:lower:]\')\narch=$(uname -m)\ncase "$os" in linux|darwin) ;; *) exit 2 ;; esac\ncase "$arch" in x86_64|amd64) arch=x64 ;; aarch64|arm64) arch=arm64 ;; *) exit 2 ;; esac\ntarget="$os-$arch"\nif [ "$arch" = x64 ]; then target="$target-baseline"; fi\nif [ "$os" = linux ]; then\n  if [ -f /etc/alpine-release ] || (ldd --version 2>&1 | grep -qi musl); then target="$target-musl"; fi\nfi\nprintf \'OPENCODE_REMOTE_TARGET=%s\\n\' "$target"\n'

local ARCHIVES = {
    ["linux-x64-baseline"] = true,
    ["linux-arm64"] = true,
    ["darwin-x64-baseline"] = true,
    ["darwin-arm64"] = true,
    ["linux-x64-baseline-musl"] = true,
    ["linux-arm64-musl"] = true,
}

function command.archiveUrl(target: string, version: string): string?
    if not ARCHIVES[target] then
        return nil
    end
    return `https://registry.npmjs.org/@opencode/cli-{target}/-/cli-{target}-{version}.tgz`
end

-- Installs the CLI from `url` into this app's own folder on the host, checking that it runs.
function command.installScript(version: string, url: string): string
    local destination = command.quote(`.opencode/desktop-ssh/{version}/opencode`)
    return 'set -eu\numask 077\ndestination="$HOME"/'
        .. destination
        .. '\nmkdir -p "$(dirname "$destination")"\nstage=$(mktemp -d "$(dirname "$destination")/.install-XXXXXX")\ntrap \'rm -rf "$stage"\' EXIT\nurl='
        .. command.quote(url)
        .. '\nif command -v curl >/dev/null 2>&1; then\n  curl -fsSL --connect-timeout 15 --max-time 180 "$url" -o "$stage/archive.tgz"\nelse\n  wget -T 180 -O "$stage/archive.tgz" "$url"\nfi\ntar -xzf "$stage/archive.tgz" -C "$stage"\nchmod 755 "$stage/package/bin/opencode"\n"$stage/package/bin/opencode" --version >/dev/null\nmv "$stage/package/bin/opencode" "$destination"\n'
end

function command.parseVersion(output: string): string?
    for line in string.gmatch(output, "[^\r\n]+") do
        local t = trim(line)
        if #t > 0 then
            local at = nil
            local from = 1
            while true do
                local f = string.find(t, " v", from, true)
                if not f then
                    break
                end
                at, from = f, f + 1
            end
            return if at then string.sub(t, at + 2) else t
        end
    end
    return nil
end

export type Registration = { url: string, password: string }

-- The service's registration the scripts print, when its URL is the one its status names.
function command.parseRegistration(output: string): Registration?
    local status: string? = nil
    for line in string.gmatch(output, "[^\r\n]+") do
        local s = string.match(line, "^OPENCODE_SSH_STATUS=(.*)$")
        if s then
            status = trim(s)
        end
    end
    if not status then
        return nil
    end
    for body in string.gmatch(output, "OPENCODE_SSH_REGISTRATION_BEGIN(.-)OPENCODE_SSH_REGISTRATION_END") do
        local v = decode(trim(body))
        if type(v) == "table" and type(v.url) == "string" and type(v.password) == "string" and v.url == status and type(v.pid) == "number" and v.pid > 0 then
            return { url = v.url, password = v.password }
        end
    end
    return nil
end

-- The service's loopback address on the host, to forward to.
function command.remoteAddress(url: string): (string?, number?)
    local rest = string.match(url, "^http://(.*)$")
    if not rest then
        return nil, nil
    end
    local hostport = string.match(rest, "^[^/]*") or ""
    local host, port = string.match(hostport, "^(.*):(%d+)$")
    if not host or (string.sub(host, -1) == "]" and string.sub(hostport, 1, 1) ~= "[") then
        host, port = hostport, "80"
    end
    if not (host == "127.0.0.1" or host == "localhost" or host == "0.0.0.0" or host == "[::]" or host == "[::1]") then
        return nil, nil
    end
    return if host == "[::1]" then "[::1]" else "127.0.0.1", tonumber(port)
end

-- `sshHostname`: the host a target ends with, for display.
function command.hostname(target: string): string
    local last = ""
    for w in string.gmatch(target, "%S+") do
        last = w
    end
    last = string.gsub(last, "[\"'\\]", "")
    local host = last
    for part in string.gmatch(last, "[^@]+") do
        host = part
    end
    return if string.sub(last, -1) == "@" then "" else host
end

-- `sshName`: the name given, or the host the target ends with.
function command.name(config: { name: string, target: string }): string
    local n = trim(config.name or "")
    return if #n > 0 then n else command.hostname(config.target)
end

return command
