#!/usr/bin/env bash
# Package a native release build. All three formats install the same executable.
set -euo pipefail
bin=$(realpath "${1:?usage: package-linux.sh binary output-directory}")
mkdir -p "${2:?missing output directory}"
out=$(realpath "$2")
version=${OPENCODE_RUNE_VERSION:?}
expected="brocode $version (${OPENCODE_RUNE_BUILD_CHANNEL:?})"
[[ $("$bin" --version) == "$expected" ]] || { echo "Expected $expected" >&2; exit 1; }

machine=$(uname -m)
case $machine in
  x86_64) arch=x64 deb_arch=amd64 runtime_sha=2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d ;;
  aarch64) arch=arm64 deb_arch=arm64 runtime_sha=00cbdfcf917cc6c0ff6d3347d59e0ca1f7f45a6df1a428a0d6d8a78664d87444 ;;
  *) echo "Unsupported machine: $machine" >&2; exit 1 ;;
esac
stem="$out/OpenCode-Rune-$version-linux-$arch"
work=$(mktemp -d "$out/.package.XXXXXXXX")
trap 'rm -rf "$work"' EXIT
repo_root=$(realpath "$(dirname "$0")/../../..")
root=$work/root
icon=share/icons/hicolor/1024x1024/apps/brocode.png
desktop=share/applications/brocode.desktop
doc=share/doc/brocode
# The app's own URL scheme, as the binary has it (`win_install::Product::from_build`): the one
# the build was given, else the channel's. The desktop file claims it, so `xdg-open` and a
# browser hand its links to the app (the OpenCode Console's return after a sign-in is one);
# a running app gets them from the new process (`rune::single_instance`).
scheme=${OPENCODE_RUNE_URL_SCHEME:-}
if [[ -z $scheme ]]; then
  case $OPENCODE_RUNE_BUILD_CHANNEL in
    local) scheme=brocode-dev ;;
    stable | beta) scheme=brocode-$OPENCODE_RUNE_BUILD_CHANNEL ;;
    *) scheme=brocode ;;
  esac
fi
scheme=${scheme,,}
install -Dm755 "$bin" "$root/usr/bin/brocode"
# Strip brocode (and libopencode_office.so below when built with office-library), removing
# ~45 MB of .symtab/.strtab while keeping .eh_frame for panic unwinding. RPM disables __strip
# globally so bundled opencode-cli's EOF Bun payload stays intact.
if [ "${OPENCODE_RUNE_KEEP_SYMBOLS:-0}" != "1" ]; then
  if command -v strip >/dev/null 2>&1; then
    strip --strip-unneeded "$root/usr/bin/brocode"
    "$root/usr/bin/brocode" --unwind-check >/dev/null
  else
    echo "package-linux: strip not found; shipping unstripped brocode" >&2
  fi
fi
# Probe the staged binary itself to determine whether it expects the external office library:
# a non-office-library build succeeds with "in the exe" (ignoring any stale .so in target/);
# an office-library build fails before the .so is installed and requires it beside $bin.
office=0
office_rpm_files=""
office_lib="$(dirname "$bin")/libopencode_office.so"
if office_out=$("$root/usr/bin/brocode" --office-check 2>&1); then
  case "$office_out" in
    *"in the exe"*)
      if [ -f "$office_lib" ]; then
        echo "package-linux: ignoring stale $office_lib (binary has office engines linked in)" >&2
      fi
      ;;
    *)
      echo "package-linux: unexpected --office-check output before library install: $office_out" >&2
      exit 1
      ;;
  esac
else
  if [ ! -f "$office_lib" ]; then
    echo "package-linux: binary requires office library (--office-check: $office_out), but $office_lib is missing" >&2
    exit 1
  fi
  install -Dm755 "$office_lib" "$root/usr/lib/brocode/libopencode_office.so"
  if [ "${OPENCODE_RUNE_KEEP_SYMBOLS:-0}" != "1" ]; then
    if command -v strip >/dev/null 2>&1; then
      strip --strip-unneeded "$root/usr/lib/brocode/libopencode_office.so"
    else
      echo "package-linux: strip not found; shipping unstripped libopencode_office.so" >&2
    fi
  fi
  office_out=$("$root/usr/bin/brocode" --office-check 2>&1) || {
    echo "package-linux: staged --office-check failed: $office_out" >&2
    exit 1
  }
  [[ "$office_out" == *"in their library, loaded"* ]] || {
    echo "package-linux: expected office library to load, got: $office_out" >&2
    exit 1
  }
  office=1
  office_rpm_files=$'%dir /usr/lib/brocode\n/usr/lib/brocode/libopencode_office.so'
fi
cli_rpm_files=""
if [ "${OPENCODE_RUNE_BUNDLE_CLI:-1}" != "0" ]; then
  bash "$repo_root/scripts/guardrails/run-js.sh" "$repo_root/scripts/fetch-cli.mjs" \
    --target "linux-$arch" --dest "$root/usr/share/brocode" --channel "$OPENCODE_RUNE_BUILD_CHANNEL"
  cli_rpm_files=$'/usr/share/brocode/opencode-cli\n/usr/share/brocode/opencode-cli.version'
fi
install -Dm644 "$(dirname "$0")/../assets/icon/icon-1024.png" "$root/usr/$icon"
install -Dm644 "$repo_root/LICENSE" "$root/usr/$doc/LICENSE"
install -Dm644 "$repo_root/THIRD_PARTY_NOTICES.md" "$root/usr/$doc/THIRD_PARTY_NOTICES.md"
# Ubuntu 24.04+ restricts unprivileged user namespaces (`kernel.apparmor_restrict_unprivileged_userns=1`).
# Ship an AppArmor 4.0 profile (loaded in postinst only when apparmor_parser supports abi/4.0,
# matching official Electron's .deb template) so Chromium's Linux sandbox works without --no-sandbox.
apparmor_src=share/brocode/apparmor-profile
mkdir -p "$root/usr/share/brocode"
cat > "$root/usr/$apparmor_src" <<'EOF'
abi <abi/4.0>,
include <tunables/global>

profile brocode-browser-engine "@{HOME}/.local/state/co.anomaly.opencode/browser-engine/[0-9]*/OpenCode Browser Engine/opencode-browser-engine{,-helper}" flags=(unconfined) {
  userns,

  # Site-specific additions and overrides. See local/README for details.
  include if exists <local/brocode-browser-engine>
}
EOF
chmod 0644 "$root/usr/$apparmor_src"
mkdir -p "$root/usr/share/applications"
cat > "$root/usr/$desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Brocode
Exec=brocode %u
Icon=brocode
Categories=Development;
Terminal=false
MimeType=x-scheme-handler/$scheme;
EOF

# Type 2 AppImage: a pinned architecture-specific runtime followed by the AppDir as squashfs.
curl --fail --location --retry 3 "https://github.com/AppImage/type2-runtime/releases/download/20251108/runtime-$machine" -o "$work/runtime"
echo "$runtime_sha  $work/runtime" | sha256sum --check --status
mkdir "$work/AppDir"
cp -al "$root/usr" "$work/AppDir/"
ln -s usr/bin/brocode "$work/AppDir/AppRun"
ln -s "usr/$icon" "$work/AppDir/brocode.png"
ln -s "usr/$icon" "$work/AppDir/.DirIcon"
ln -s "usr/$desktop" "$work/AppDir/brocode.desktop"
mksquashfs "$work/AppDir" "$work/app.squashfs" -root-owned -noappend -comp gzip -no-progress
cat "$work/runtime" "$work/app.squashfs" > "$stem.AppImage"
chmod +x "$stem.AppImage"
[[ $(cd "$work" && "$stem.AppImage" --appimage-extract-and-run --version) == "$expected" ]]
if [ "$office" = "1" ]; then
  office_out=$(cd "$work" && "$stem.AppImage" --appimage-extract-and-run --office-check 2>&1) || {
    echo "package-linux: AppImage --office-check failed: $office_out" >&2
    exit 1
  }
  [[ "$office_out" == *"in their library, loaded"* ]] || {
    echo "package-linux: AppImage expected office library to load, got: $office_out" >&2
    exit 1
  }
fi

# Tilde sorts prereleases before stable; the run number stays numeric for upgrades.
# deb and rpm versions can't carry '-': 0.0.0-dev-484 is 0.0.0~dev.484, 0.0.0-pr-12-3 is 0.0.0~pr.12.3.
pkg_version=${version/-/\~}
pkg_version=${pkg_version//-/.}
mkdir -p "$work/rpm"
cat > "$work/app.spec" <<EOF
%global __provides_exclude_from ^/usr/lib/brocode/.*$
Name: brocode
Version: $pkg_version
Release: 1
Summary: Brocode desktop client
License: MIT
Requires: libxkbcommon, fontconfig

%description
Brocode native desktop client.

%install
mkdir -p "%{buildroot}"
cp -a "$root/usr" "%{buildroot}/"

%files
/usr/bin/brocode
/usr/$icon
/usr/$desktop
/usr/$doc/LICENSE
/usr/$doc/THIRD_PARTY_NOTICES.md
/usr/$apparmor_src
$office_rpm_files
$cli_rpm_files

%post
if command -v apparmor_status >/dev/null 2>&1 && apparmor_status --enabled >/dev/null 2>&1; then
  if command -v apparmor_parser >/dev/null 2>&1 && apparmor_parser --skip-kernel-load --debug /usr/$apparmor_src >/dev/null 2>&1; then
    install -Dm644 /usr/$apparmor_src /etc/apparmor.d/brocode-browser-engine || true
    if ! { [ -x /usr/bin/ischroot ] && /usr/bin/ischroot; }; then
      apparmor_parser --replace --write-cache --skip-read-cache /etc/apparmor.d/brocode-browser-engine || true
    fi
  fi
fi
# The desktop file's scheme, known to the desktop at once.
if command -v update-desktop-database >/dev/null 2>&1; then update-desktop-database -q /usr/share/applications || true; fi

%postun
if command -v update-desktop-database >/dev/null 2>&1; then update-desktop-database -q /usr/share/applications || true; fi
if [ "\$1" = "0" ] && [ -f /etc/apparmor.d/brocode-browser-engine ]; then
  if command -v apparmor_parser >/dev/null 2>&1 && ! { [ -x /usr/bin/ischroot ] && /usr/bin/ischroot; }; then
    apparmor_parser --remove /etc/apparmor.d/brocode-browser-engine >/dev/null 2>&1 || true
  fi
  rm -f /etc/apparmor.d/brocode-browser-engine
fi
EOF
# Keep __strip a no-op: opencode-cli is a Bun single-file executable whose payload sits at EOF.
rpmbuild -bb --define "_topdir $work/rpm" --define 'debug_package %{nil}' --define '__strip /bin/true' "$work/app.spec"
cp "$work/rpm/RPMS/$machine/"*.rpm "$stem.rpm"
rpm -qp --list "$stem.rpm" | grep -Fx /usr/bin/brocode
if [ "$office" = "1" ]; then
  rpm -qp --list "$stem.rpm" | grep -Fx /usr/lib/brocode/libopencode_office.so
fi

mkdir "$root/DEBIAN"
cat > "$root/DEBIAN/control" <<EOF
Package: brocode
Version: $pkg_version
Architecture: $deb_arch
Maintainer: API Brother <support@apibrother.com>
Depends: libc6, libgcc-s1, libstdc++6, libasound2, libfontconfig1, libxkbcommon0, libwayland-client0, libx11-6
Description: Brocode native desktop client
EOF
cat > "$root/DEBIAN/postinst" <<'EOF'
#!/bin/sh
set -e
if [ "${1:-}" = "configure" ]; then
  if command -v apparmor_status >/dev/null 2>&1 && apparmor_status --enabled >/dev/null 2>&1; then
    SRC=/usr/share/brocode/apparmor-profile
    DST=/etc/apparmor.d/brocode-browser-engine
    if command -v apparmor_parser >/dev/null 2>&1 && apparmor_parser --skip-kernel-load --debug "$SRC" >/dev/null 2>&1; then
      install -Dm644 "$SRC" "$DST"
      if ! { [ -x /usr/bin/ischroot ] && /usr/bin/ischroot; }; then
        apparmor_parser --replace --write-cache --skip-read-cache "$DST" || true
      fi
    fi
  fi
  # The desktop file's scheme, known to the desktop at once.
  if command -v update-desktop-database >/dev/null 2>&1; then update-desktop-database -q /usr/share/applications || true; fi
fi
EOF
cat > "$root/DEBIAN/postrm" <<'EOF'
#!/bin/sh
set -e
if [ "${1:-}" = "remove" ] || [ "${1:-}" = "purge" ]; then
  DST=/etc/apparmor.d/brocode-browser-engine
  if [ -f "$DST" ]; then
    if command -v apparmor_parser >/dev/null 2>&1 && ! { [ -x /usr/bin/ischroot ] && /usr/bin/ischroot; }; then
      apparmor_parser --remove "$DST" >/dev/null 2>&1 || true
    fi
    rm -f "$DST"
  fi
  if command -v update-desktop-database >/dev/null 2>&1; then update-desktop-database -q /usr/share/applications || true; fi
fi
EOF
chmod 0755 "$root/DEBIAN/postinst" "$root/DEBIAN/postrm"
dpkg-deb --root-owner-group --build "$root" "$stem.deb"
dpkg-deb --extract "$stem.deb" "$work/installed"
[[ $("$work/installed/usr/bin/brocode" --version) == "$expected" ]]
if [ "$office" = "1" ]; then
  office_out=$("$work/installed/usr/bin/brocode" --office-check 2>&1) || {
    echo "package-linux: DEB --office-check failed: $office_out" >&2
    exit 1
  }
  [[ "$office_out" == *"in their library, loaded"* ]] || {
    echo "package-linux: DEB expected office library to load, got: $office_out" >&2
    exit 1
  }
fi
# The installed desktop file takes a link and claims the app's scheme.
grep -Fxq "Exec=brocode %u" "$work/installed/usr/$desktop"
grep -Fxq "MimeType=x-scheme-handler/$scheme;" "$work/installed/usr/$desktop"
