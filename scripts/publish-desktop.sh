#!/usr/bin/env bash
# Publishes the desktop builds to the R2 bucket behind dl.apibrother.com.
# Usage: scripts/publish-desktop.sh [--version V] [--require PLATFORM[,PLATFORM]] <dir>
# <dir> holds whatever of brocode-desktop-win-x64.exe, brocode-desktop-darwin-arm64.zip,
# brocode-desktop-darwin-x64.zip was built. CLOUDFLARE_API_TOKEN (R2 object write) and
# CLOUDFLARE_ACCOUNT_ID must be in the environment.
#
# The manifest (brocode-desktop-latest.json) lists only what is here, and names each file the
# last manifest had but this build lacks in a warning: a missing macOS build (the CLI it bundles
# has no darwin binary yet) leaves the earlier one in the bucket and downloadable. A file that is
# here replaces the old one; the manifest's files whose bytes did not change are left alone.
# The version comes from --version, else the app's Info.plist inside the macOS arm64 zip, else
# the Windows app in brocode-desktop-windows-x64.zip. It is published here and in the manifest.
set -euo pipefail

DIR=""
VERSION=""
REQUIRE=""
while [ $# -gt 0 ]; do
    case "$1" in
        --version) VERSION=${2:?--version needs a value}; shift 2 ;;
        --require) REQUIRE=${2:?--require needs a value}; shift 2 ;;
        -*) echo "publish-desktop: unknown option $1" >&2; exit 2 ;;
        *) DIR=$1; shift ;;
    esac
done
[[ -n $DIR && -d $DIR ]] || { echo "usage: publish-desktop.sh [--version V] [--require PLATFORM[,..]] DIR" >&2; exit 2; }
: "${CLOUDFLARE_API_TOKEN:?set CLOUDFLARE_API_TOKEN}"
: "${CLOUDFLARE_ACCOUNT_ID:?set CLOUDFLARE_ACCOUNT_ID}"

files=(
    "windows-x64|brocode-desktop-win-x64.exe|application/x-msdownload"
    "darwin-arm64|brocode-desktop-darwin-arm64.zip|application/zip"
    "darwin-x64|brocode-desktop-darwin-x64.zip|application/zip"
)
for platform in ${REQUIRE//,/ }; do
    for entry in "${files[@]}"; do
        if [ "${entry%%|*}" = "$platform" ]; then
            rest=${entry#*|}
            [[ -f $DIR/${rest%%|*} ]] || { echo "publish-desktop: $DIR has no ${rest%%|*}, which $platform needs" >&2; exit 1; }
        fi
    done
done

if [ -z "$VERSION" ] && [ -f "$DIR/brocode-desktop-darwin-arm64.zip" ]; then
    VERSION=$(python3 - "$DIR/brocode-desktop-darwin-arm64.zip" <<'PY'
import plistlib, sys, zipfile
with zipfile.ZipFile(sys.argv[1]) as z:
    print(plistlib.loads(z.read("Brocode.app/Contents/Info.plist"))["CFBundleShortVersionString"])
PY
)
fi
if [ -z "$VERSION" ] && [ -f "$DIR/brocode-desktop-windows-x64.zip" ]; then
    # `brocode-desktop --version` has no `--version-only` mode a Linux runner could run, so the
    # app's name and version are read from the exe's strings.
    VERSION=$(unzip -p "$DIR/brocode-desktop-windows-x64.zip" brocode-desktop.exe 2>/dev/null | strings | grep -oE 'brocode [0-9]+\.[0-9]+\.[0-9]+[^ ]* \([a-z]+\)' | head -1 | cut -d' ' -f2) || true
fi
if [ -z "$VERSION" ]; then
    echo "publish-desktop: no macOS zip to read the version from and no --version given" >&2
    exit 1
fi

api="https://api.cloudflare.com/client/v4/accounts/${CLOUDFLARE_ACCOUNT_ID}/r2/buckets/brocode/objects"
old=$(curl -fsS "${api}/brocode-desktop-latest.json" 2>/dev/null) || true
[[ -n $old ]] || old='{}'
case $old in \{*) ;; *) old='{}' ;; esac

for entry in "${files[@]}"; do
    platform=${entry%%|*}
    rest=${entry#*|}
    file=${rest%%|*}
    type=${rest#*|}
    [ -f "$DIR/$file" ] && continue
    note=$(jq -r --arg p "$platform" '.files[]? | select(.platform == $p) | "manifest: no '"$file"' in this build; dl.apibrother.com keeps publishing the last one (\(.sha256[0:12]))"' <<<"$old" 2>/dev/null || true)
    [ -z "$note" ] && note="manifest: no $file in this build, and none published before (dl.apibrother.com has no $platform download)"
    echo "warning: $note" >&2
    [ -n "${GITHUB_STEP_SUMMARY:-}" ] && echo "- warning: $note" >> "$GITHUB_STEP_SUMMARY" || true
done

python3 - "$DIR" "$VERSION" "$old" "${files[@]}" <<'PY'
import datetime, hashlib, json, os, sys
directory, version, old = sys.argv[1], sys.argv[2], json.loads(sys.argv[3] or "{}")
entries = []
for spec in sys.argv[4:]:
    platform, name, _ = spec.split("|")
    path = os.path.join(directory, name)
    if not os.path.exists(path):
        continue
    digest, size = hashlib.sha256(), 0
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            digest.update(chunk)
            size += len(chunk)
    sha = digest.hexdigest()
    prior = next((e for e in old.get("files", []) if e.get("name") == name), None)
    unchanged = prior and prior.get("sha256") == sha
    entries.append({"platform": platform, "name": name, "bytes": size, "sha256": sha, "unchanged": bool(unchanged)})
    print(("keep" if unchanged else "upload"), name, f"{size:,} bytes", sha[:12])
built = datetime.datetime.now(datetime.timezone.utc).isoformat().replace("+00:00", "Z")
with open(os.path.join(directory, "brocode-desktop-latest.json"), "w") as f:
    json.dump({"version": version, "builtAt": built, "files": [{k: v for k, v in e.items() if k != "unchanged"} for e in entries]}, f, indent=2)
changed = [e["name"] for e in entries if not e["unchanged"]]
with open(os.path.join(directory, "upload.txt"), "w") as f:
    f.write("\n".join(changed) + ("\n" if changed else ""))
print("version", version, "built", built, "platforms", ", ".join(e["platform"] for e in entries))
PY

while IFS='|' read -r file type; do
    [ -f "$DIR/$file" ] || continue
    if [ -f "$DIR/upload.txt" ] && ! grep -qxF "$file" "$DIR/upload.txt" && [ "$file" != "brocode-desktop-latest.json" ]; then
        printf '%s unchanged; left as published\n' "$file"
        continue
    fi
    curl -fsS -X PUT --data-binary "@${DIR}/${file}" \
        -H "Authorization: Bearer ${CLOUDFLARE_API_TOKEN}" \
        -H "Content-Type: ${type}" \
        -o /dev/null \
        "${api}/${file}"
    printf '%s -> %s\n' "${file}" "${file}"
done <<'EOF'
brocode-desktop-win-x64.exe|application/x-msdownload
brocode-desktop-darwin-arm64.zip|application/zip
brocode-desktop-darwin-x64.zip|application/zip
brocode-desktop-latest.json|application/json
EOF
rm -f "$DIR/upload.txt"
