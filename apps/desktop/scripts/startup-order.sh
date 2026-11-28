#!/usr/bin/env bash
# Makes apps/desktop/pgo/startup-order.txt: the functions a launch runs, in the order it first
# runs them, for the link (link-order.sh on macOS; the same list serves lld-link /order on Windows).
#
#   apps/desktop/scripts/startup-order.sh <launch-*.profraw ...>     or one merged .profdata
#
# The profiles come from launches of the training exe (build-app.sh with OPENCODE_RUNE_PGO_TRAIN=1,
# or a pull request from a branch with "order-train" in its name; run each launch to its content
# with LLVM_PROFILE_FILE=<dir>/launch-%p.profraw). apps/desktop/pgo/README.md has the whole way.
#
# Needs the llvm-profdata of the toolchain that built the exe (`rustup component add llvm-tools`,
# or LLVM_PROFDATA=<path>): the raw profile's version follows rustc's LLVM, and Xcode's cannot read it.
set -euo pipefail
[ $# -gt 0 ] || { sed -n '2,12p' "$0" >&2; exit 2; }
ROOT=$(cd "$(dirname "$0")/../../.." && pwd)
profdata=${LLVM_PROFDATA:-$(rustc --print sysroot)/lib/rustlib/$(rustc -vV | sed -n 's/^host: //p')/bin/llvm-profdata}
[ -x "$profdata" ] || { echo "no llvm-profdata at $profdata: rustup component add llvm-tools, or set LLVM_PROFDATA" >&2; exit 1; }
work=$(mktemp -d "${TMPDIR:-/tmp}/startup-order.XXXXXX")
trap 'rm -rf "$work"' EXIT

case "$1" in
  *.profdata) data=$1 ;;
  *)
    data=$work/train.profdata
    "$profdata" merge --temporal-profile-max-trace-length=200000 -o "$data" "$@"
    ;;
esac
"$profdata" order "$data" -o "$work/order.txt"

# Names without what changes from build to build: the crate hashes (Cs<hash>_), the back
# references that count bytes across them (B<n>_) and thin LTO's suffix. link-order.sh takes the
# same three out of a build's names before it looks them up here.
out=${OPENCODE_RUNE_ORDER_OUT:-$ROOT/apps/desktop/pgo/startup-order.txt}
reader="apps/desktop/scripts/link-order.sh"
case "$(basename "$out")" in
  *linux*) reader="apps/desktop/scripts/link-order-linux.sh" ;;
esac
{
  echo "# The functions a launch runs, in the order it first runs them: made by apps/desktop/scripts/startup-order.sh,"
  echo "# read by $reader. Names are Rust v0 symbols without crate hashes."
  echo "# trained at ${OPENCODE_RUNE_TRAINED_AT:-$(git -C "$ROOT" rev-parse HEAD)}, $(rustc -V), $(date -u +%Y-%m-%d), from $# profile file(s)"
  awk '
    /^#/ || !NF { next }
    {
      sub(/\.llvm\.[0-9]+$/, "")
      gsub(/Cs[0-9A-Za-z][0-9A-Za-z]?[0-9A-Za-z]?[0-9A-Za-z]?[0-9A-Za-z]?[0-9A-Za-z]?[0-9A-Za-z]?[0-9A-Za-z]?[0-9A-Za-z]?[0-9A-Za-z]?[0-9A-Za-z]?[0-9A-Za-z]?_/, "C")
      gsub(/B[0-9A-Za-z]?[0-9A-Za-z]?[0-9A-Za-z]?_/, "B_")
      if (!seen[$0]++) print
    }
  ' "$work/order.txt"
} > "$out"
echo "$out: $(grep -c -v '^#' "$out") functions" >&2
