#!/usr/bin/env bash
# The Linux link of brocode-desktop with the functions a launch runs laid out first and together,
# so a launch whose exe is not in memory reads few pages of it. Cargo calls this as the linker for
# the Linux targets (`.cargo/config.toml`); every link but the app's exe goes to cc untouched.
#
# It is link-order.sh's method (the macOS link) for lld: apps/desktop/pgo/startup-order.txt lists
# those functions in the order a training launch ran them, with the crate hashes taken out of the
# names (a hash changes with every Rune pin, the name without it does not). Here the list is
# matched against the names in the objects being linked, and the linker gets the real names
# (--symbol-ordering-file). A function the list names and this build lacks is skipped.
#
# The order is an optimisation and never fails a build: with no list, no nm, a linker that is not
# lld (it alone has the option), or a link that fails with the order, the exe is linked exactly as
# it would be without this script, and the build log says so in one line.
# OPENCODE_RUNE_LINK_ORDER=0 links without it. Beside the exe it writes <exe>.link-order.txt.
set -uo pipefail

# The output's name, from the arguments as given or from rustc's @file (one argument a line, in
# double quotes). Anything this script does not order is handed on at once.
out=
previous=
for arg in "$@"; do
  [ "$previous" = -o ] && out=$arg
  previous=$arg
done
response=
if [ -z "$out" ]; then
  for arg in "$@"; do
    case "$arg" in
      @*) response=${arg#@} ;;
    esac
  done
  [ -n "$response" ] && [ -f "$response" ] && out=$(grep -A1 -x -e '"-o"' -e '-o' "$response" | sed -n 2p | sed -e 's/^"//' -e 's/"$//')
fi
case "$(basename "${out:-none}")" in
  brocode-desktop | opencode_desktop-*) ;;
  *) exec cc "$@" ;;
esac
[ "${OPENCODE_RUNE_LINK_ORDER:-1}" = 1 ] || exec cc "$@"

note() {
  echo "link order: $1" >&2
  echo "link order: $1" >> "$out.link-order.txt" 2>/dev/null || true
}
rm -f "$out.link-order.txt"
list=${OPENCODE_RUNE_ORDER_LIST:-$(cd "$(dirname "$0")" && pwd)/../pgo/startup-order.txt}

args=()
for arg in "$@"; do
  case "$arg" in
    @*)
      while IFS= read -r line; do
        line=${line#\"}
        line=${line%\"}
        line=${line//\\\"/\"}
        args+=("${line//\\\\/\\}")
      done < "${arg#@}"
      ;;
    *) args+=("$arg") ;;
  esac
done
objects=()
lld=0
for arg in "${args[@]}"; do
  case "$arg" in
    *.o | *.rlib | *.a) [ -f "$arg" ] && objects+=("$arg") ;;
    -fuse-ld=lld | -fuse-ld=*lld*) lld=1 ;;
  esac
done
if [ ! -s "$list" ]; then
  note "no list at $list; linked without the order"
  exec cc "$@"
fi
if [ "$lld" != 1 ]; then
  note "this link does not go through lld (only it takes an order); linked without the order"
  exec cc "$@"
fi
if ! command -v nm >/dev/null 2>&1 || [ "${#objects[@]}" -eq 0 ]; then
  note "no nm, or no objects to read; linked without the order"
  exec cc "$@"
fi

order=$(mktemp "${TMPDIR:-/tmp}/opencode-order.XXXXXX") || exec cc "$@"
trap 'rm -f "$order" "$order.ranked"' EXIT
# nm prints an archive's member names and its complaints about members that are not objects: only
# lines that are Rust names (or main) pass. The two patterns are the list's own (startup-order.sh).
{ nm --defined-only --format=just-symbols "${objects[@]}" 2>/dev/null || true; } | awk -v list="$list" '
  function plain(name) {
    sub(/\.llvm\.[0-9]+$/, "", name)
    gsub(/Cs[0-9A-Za-z][0-9A-Za-z]?[0-9A-Za-z]?[0-9A-Za-z]?[0-9A-Za-z]?[0-9A-Za-z]?[0-9A-Za-z]?[0-9A-Za-z]?[0-9A-Za-z]?[0-9A-Za-z]?[0-9A-Za-z]?[0-9A-Za-z]?_/, "C", name)
    gsub(/B[0-9A-Za-z]?[0-9A-Za-z]?[0-9A-Za-z]?_/, "B_", name)
    return name
  }
  BEGIN {
    while ((getline line < list) > 0) if (line != "" && line !~ /^#/ && !(line in rank)) rank[line] = ++count
  }
  /^_R/ || $0 == "main" { key = plain($0); if (key in rank && !($0 in seen)) { seen[$0] = 1; print rank[key] "\t" $0 } }
' | sort -n -s -k1,1 > "$order.ranked"
cut -f2 "$order.ranked" > "$order"
found=$(cut -f1 "$order.ranked" | uniq | wc -l | tr -d ' ')
listed=$(grep -c -v -e '^#' -e '^$' "$list")
if [ ! -s "$order" ]; then
  note "none of the $listed listed functions are in this build; linked without the order"
  exec cc "$@"
fi
note "$found of $listed listed functions are in this build ($(wc -l < "$order" | tr -d ' ') symbols)"
# A link that fails with the order is made again without it, with the arguments as they came.
cc "$@" -Wl,--symbol-ordering-file="$order" -Wl,--no-warn-symbol-ordering && exit 0
note "THE LINK FAILED WITH THE ORDER FILE; linking without it"
exec cc "$@"
