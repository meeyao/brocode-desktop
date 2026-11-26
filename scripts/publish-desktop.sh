#!/usr/bin/env bash
# Publishes the desktop builds to the R2 bucket behind dl.apibrother.com.
# Usage: scripts/publish-desktop.sh <dir>
# <dir> holds brocode-desktop-win-x64.exe, brocode-desktop-darwin-arm64.zip and
# brocode-desktop-darwin-x64.zip. CLOUDFLARE_API_TOKEN (R2 object write) and
# CLOUDFLARE_ACCOUNT_ID must be in the environment.
set -euo pipefail

DIR=${1:?usage: publish-desktop.sh DIR}
: "${CLOUDFLARE_API_TOKEN:?set CLOUDFLARE_API_TOKEN}"
: "${CLOUDFLARE_ACCOUNT_ID:?set CLOUDFLARE_ACCOUNT_ID}"

VERSION=$(python3 - "$DIR/brocode-desktop-darwin-arm64.zip" <<'PY'
import plistlib, sys, zipfile
with zipfile.ZipFile(sys.argv[1]) as z:
    print(plistlib.loads(z.read("Brocode.app/Contents/Info.plist"))["CFBundleShortVersionString"])
PY
)

python3 - "$DIR" "$VERSION" <<'PY'
import datetime, hashlib, json, os, sys
directory, version = sys.argv[1], sys.argv[2]
files = [
    ("windows-x64", "brocode-desktop-win-x64.exe"),
    ("darwin-arm64", "brocode-desktop-darwin-arm64.zip"),
    ("darwin-x64", "brocode-desktop-darwin-x64.zip"),
]
entries = []
for platform, name in files:
    digest, size = hashlib.sha256(), 0
    with open(os.path.join(directory, name), "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            digest.update(chunk)
            size += len(chunk)
    entries.append({"platform": platform, "name": name, "bytes": size, "sha256": digest.hexdigest()})
built = datetime.datetime.now(datetime.timezone.utc).isoformat().replace("+00:00", "Z")
with open(os.path.join(directory, "brocode-desktop-latest.json"), "w") as f:
    json.dump({"version": version, "builtAt": built, "files": entries}, f, indent=2)
print("version", version, "built", built)
PY

api="https://api.cloudflare.com/client/v4/accounts/${CLOUDFLARE_ACCOUNT_ID}/r2/buckets/brocode/objects"
while IFS='|' read -r file key type; do
    curl -fsS -X PUT --data-binary "@${DIR}/${file}" \
        -H "Authorization: Bearer ${CLOUDFLARE_API_TOKEN}" \
        -H "Content-Type: ${type}" \
        -o /dev/null \
        "${api}/${key}"
    printf '%s -> %s\n' "${file}" "${key}"
done <<'EOF'
brocode-desktop-win-x64.exe|brocode-desktop-win-x64.exe|application/x-msdownload
brocode-desktop-darwin-arm64.zip|brocode-desktop-darwin-arm64.zip|application/zip
brocode-desktop-darwin-x64.zip|brocode-desktop-darwin-x64.zip|application/zip
brocode-desktop-latest.json|brocode-desktop-latest.json|application/json
EOF
