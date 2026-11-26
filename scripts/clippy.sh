#!/usr/bin/env bash
# Runs `cargo clippy --no-deps --all-targets -- -D warnings` across the workspace
# and every actively used non-default feature set that compiles distinct code:
#   - workspace (default features)
#   - brocode-desktop (--no-default-features --features knobs)  [dev-fast / no office or stories]
#   - brocode-desktop (--features alloc-sites)                  [heap-dump measuring build]
#   - session-view (--features open-bench)                       [open_bench example]
#   - browser-pane (--no-default-features)                       [no-engine fallback branches]
#   - browser-pane (--features faults)                           [harness fault injection + engine]
#   - win-install (--features resources)                         [build-script Windows resources]
# Extra cargo arguments (such as --target, -p, --locked) pass through; passing -p
# scopes both the default run and the matching feature-set checks to those packages.
# Usage: scripts/clippy.sh [cargo args...] [-- rustc/clippy args...]
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

cargo_args=()
extra_args=()
rustc_args=()
pkgs=()
has_pkg=0
has_workspace=0
has_features=0
in_rustc=0

while [ "$#" -gt 0 ]; do
  if [ "$in_rustc" -eq 1 ]; then
    rustc_args+=("$1")
    shift
    continue
  fi
  case "$1" in
    -h|--help)
      sed -n '2,13s/^# \{0,1\}//p' "$0"
      exit 0
      ;;
    --)
      in_rustc=1
      shift
      ;;
    --workspace)
      has_workspace=1
      cargo_args+=("$1")
      shift
      ;;
    -p|--package)
      if [ "$#" -lt 2 ]; then
        echo "clippy: $1 requires a package name" >&2
        exit 2
      fi
      has_pkg=1
      pkgs+=("${2%%@*}")
      cargo_args+=("$1" "$2")
      shift 2
      ;;
    -p=*)
      has_pkg=1
      val="${1#-p=}"
      pkgs+=("${val%%@*}")
      cargo_args+=("$1")
      shift
      ;;
    --package=*)
      has_pkg=1
      val="${1#--package=}"
      pkgs+=("${val%%@*}")
      cargo_args+=("$1")
      shift
      ;;
    -p*)
      has_pkg=1
      val="${1#-p}"
      pkgs+=("${val%%@*}")
      cargo_args+=("$1")
      shift
      ;;
    -F|--features)
      if [ "$#" -lt 2 ]; then
        echo "clippy: $1 requires a feature list" >&2
        exit 2
      fi
      has_features=1
      cargo_args+=("$1" "$2")
      extra_args+=("$1" "$2")
      shift 2
      ;;
    -F*|--features=*|--all-features|--no-default-features)
      has_features=1
      cargo_args+=("$1")
      extra_args+=("$1")
      shift
      ;;
    *)
      cargo_args+=("$1")
      extra_args+=("$1")
      shift
      ;;
  esac
done

passed=0
failed_count=0
failed=()

want_pkg() {
  local target="$1"
  [ "$has_pkg" -eq 0 ] && return 0
  local p
  for p in "${pkgs[@]}"; do
    [ "$p" = "$target" ] && return 0
  done
  return 1
}

run_pass() {
  local label="$1"
  shift
  echo "==> clippy: $label"
  if cargo clippy --no-deps --all-targets "$@" -- -D warnings ${rustc_args[@]+"${rustc_args[@]}"}; then
    passed=$((passed + 1))
  else
    failed+=("$label")
    failed_count=$((failed_count + 1))
  fi
}

run_feature_pass() {
  local pkg="$1"
  local label="$2"
  shift 2
  want_pkg "$pkg" || return 0
  run_pass "$label" -p "$pkg" "$@" ${extra_args[@]+"${extra_args[@]}"}
}

if [ "$has_pkg" -eq 0 ] && [ "$has_workspace" -eq 0 ]; then
  run_pass "workspace (default features)" --workspace ${cargo_args[@]+"${cargo_args[@]}"}
elif [ "$has_pkg" -eq 1 ]; then
  run_pass "selected packages (${pkgs[*]})" ${cargo_args[@]+"${cargo_args[@]}"}
else
  run_pass "workspace" ${cargo_args[@]+"${cargo_args[@]}"}
fi

if [ "$has_features" -eq 0 ]; then
  run_feature_pass opencode-desktop "brocode-desktop (--no-default-features --features knobs)" --no-default-features --features knobs
  run_feature_pass opencode-desktop "brocode-desktop (--features alloc-sites)" --features alloc-sites
  run_feature_pass session-view "session-view (--features open-bench)" --features open-bench
  run_feature_pass browser-pane "browser-pane (--no-default-features)" --no-default-features
  run_feature_pass browser-pane "browser-pane (--features faults)" --features faults
  run_feature_pass win-install "win-install (--features resources)" --features resources
fi

if [ "$failed_count" -gt 0 ]; then
  for label in "${failed[@]}"; do
    echo "  ERROR  clippy failed: $label"
  done
  echo "clippy: $passed passed, $failed_count failed"
  exit 1
fi

echo "clippy: ok ($passed passed, 0 failed)"
