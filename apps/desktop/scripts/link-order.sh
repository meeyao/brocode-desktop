#!/usr/bin/env bash
# The macOS link of brocode-desktop with the functions a launch runs laid out first and together,
# so a launch whose exe is not in memory reads few pages of it. Cargo calls this as the linker
# (build-app.sh sets CARGO_TARGET_<TRIPLE>_LINKER); so are the app's code libraries when a build
# has them (see below); every other link goes to cc untouched.
#
# apps/desktop/pgo/startup-order.txt lists those functions in the order a training launch ran them
# (apps/desktop/scripts/startup-order.sh makes it), with the crate hashes taken out of the names:
# a hash changes with every Rune pin, the name without it does not. Here the list is matched
# against the names in the objects being linked, and the linker gets the real names
# (-order_file). A function the list names and this build lacks is skipped. A failure to make the
# order never fails the link: the exe is then linked as it would be without this script.
#
# Beside the exe it writes <exe>.link-order.txt: how many of the listed functions this build has.
set -uo pipefail
list=${OPENCODE_RUNE_ORDER_LIST:-}

# rustc passes a long command line as @file: one argument a line, in double quotes.
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

out=
previous=
objects=()
for arg in "${args[@]}"; do
  [ "$previous" = -o ] && out=$arg
  previous=$arg
  case "$arg" in
    *.o | *.rlib | *.a) [ -f "$arg" ] && objects+=("$arg") ;;
  esac
done
link=("$@")
keep=
case "$(basename "${out:-none}")" in
  brocode-desktop | opencode_desktop-*) ;;
  libopencode_layer_*.dylib)
    # One of the app's code libraries (build-app.sh, OPENCODE_RUNE_LAYERS). rustc exports a
    # library's Rust names only, but a higher library also calls the C code linked into this one
    # (Luau, SQLite, zstd: generic Rust wrappers are compiled where they are used). So every global
    # name is exported (rustc's list is dropped), and c-names.py has the archives with C code loaded
    # whole (a function only a higher library calls would otherwise not be in the file at all), from
    # a copy with its hidden names visible where the C code was compiled that way.
    link=()
    skip=0
    for arg in "${args[@]}"; do
      if [ "$skip" = 1 ]; then
        skip=0
        continue
      fi
      case "$arg" in
        -Wl,-exported_symbols_list) skip=1 ;;
        -Wl,-exported_symbols_list,*) ;;
        *) link+=("$arg") ;;
      esac
    done
    visible=()
    while IFS= read -r arg; do visible+=("$arg"); done < <(python3 "$(dirname "$0")/c-names.py" "$out.c-names" "${link[@]}")
    [ "${#visible[@]}" -eq 0 ] || link=("${visible[@]}")
    # The link is kept, with the objects rustc deletes after it: layers-exports.py makes it again
    # once the whole app is built, exporting only the names another file takes.
    case "$out" in /*) keep="$out.relink" ;; *) keep="$PWD/$out.relink" ;; esac
    rm -rf "$keep"
    mkdir -p "$keep"
    kept=()
    count=0
    for arg in "${link[@]}"; do
      case "$arg" in
        *.o)
          if [ -f "$arg" ]; then
            count=$((count + 1))
            cp "$arg" "$keep/$count-${arg##*/}"
            arg="$keep/$count-${arg##*/}"
          fi
          ;;
      esac
      kept+=("$arg")
    done
    printf '%s\n' "${kept[@]}" > "$keep/args"
    pwd > "$keep/cwd"
    ;;
  *) exec cc "$@" ;;
esac
[ -n "$list" ] && [ -s "$list" ] && [ "${#objects[@]}" -gt 0 ] || exec cc "${link[@]}"

order=$(mktemp -t opencode-order)
trap 'rm -f "$order"' EXIT
# nm prints an archive's member names and its complaints about members that are not objects:
# only lines that are symbols pass. The two patterns are the list's own (startup-order.sh).
{ nm -j -U "${objects[@]}" 2>/dev/null || true; } | awk -v list="$list" '
  function plain(name) {
    sub(/^_/, "", name)
    sub(/\.llvm\.[0-9]+$/, "", name)
    gsub(/Cs[0-9A-Za-z][0-9A-Za-z]?[0-9A-Za-z]?[0-9A-Za-z]?[0-9A-Za-z]?[0-9A-Za-z]?[0-9A-Za-z]?[0-9A-Za-z]?[0-9A-Za-z]?[0-9A-Za-z]?[0-9A-Za-z]?[0-9A-Za-z]?_/, "C", name)
    gsub(/B[0-9A-Za-z]?[0-9A-Za-z]?[0-9A-Za-z]?_/, "B_", name)
    return name
  }
  BEGIN {
    while ((getline line < list) > 0) if (line != "" && line !~ /^#/ && !(line in rank)) rank[line] = ++count
  }
  /^_/ { key = plain($0); if (key in rank && !($0 in seen)) { seen[$0] = 1; print rank[key] "\t" $0 } }
' | sort -n -s -k1,1 > "$order.ranked"
cut -f2 "$order.ranked" > "$order"
found=$(cut -f1 "$order.ranked" | uniq | wc -l | tr -d ' ')
listed=$(grep -c -v -e '^#' -e '^$' "$list")
rm -f "$order.ranked"
echo "link order: $found of $listed listed functions are in this build ($(wc -l < "$order" | tr -d ' ') symbols)" | tee "$out.link-order.txt" >&2
[ -s "$order" ] || exec cc "${link[@]}"
if [ -n "$keep" ]; then
  cp "$order" "$keep/order"
  printf '%s\n' "-Wl,-order_file,$keep/order" >> "$keep/args"
fi
# The order is an optimisation: a link that fails with it is made again without it.
cc "${link[@]}" -Wl,-order_file,"$order" && exit 0
echo "link order: the link failed with the order file; linking without it" | tee -a "$out.link-order.txt" >&2
exec cc "${link[@]}"
