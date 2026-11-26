#!/usr/bin/env bash
# Zero-thought feature scaffolder for anomalyco/opencode-rune (Lane A Luau plugin or Lane B Rust feature).
#
# Usage:
#   scripts/new-feature.sh plugin <id> [Title]   # Lane A (default): Luau plugin under plugins/<id>/
#   scripts/new-feature.sh feature <id> [Title]  # Lane B: Rust feature crate + desktop adapter + wiring
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

usage() {
  cat <<'EOF'
Usage:
  scripts/new-feature.sh plugin <id> [Title]   # Lane A (default): Luau plugin under plugins/<id>/
  scripts/new-feature.sh feature <id> [Title]  # Lane B: Rust feature crate + desktop adapter + wiring

Rules:
  - <id> must be lowercase kebab-case (^[a-z][a-z0-9-]*$, max 64 chars)
  - Existing target directories and adapter files are never overwritten
EOF
}

capitalize() {
  local word="$1"
  local first rest
  first="$(printf '%s' "${word:0:1}" | LC_ALL=C tr '[:lower:]' '[:upper:]')"
  rest="${word:1}"
  printf '%s%s' "$first" "$rest"
}

escape_quoted() {
  printf '%s' "$1" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g'
}

if [ "$#" -lt 1 ]; then
  usage >&2
  exit 1
fi

case "$1" in
  -h|--help|help)
    usage
    exit 0
    ;;
  plugin|feature)
    LANE="$1"
    ;;
  *)
    echo "error: unknown lane '$1' (expected 'plugin' or 'feature')" >&2
    usage >&2
    exit 1
    ;;
esac

if [ "$#" -lt 2 ]; then
  echo "error: missing <id> for lane '$LANE'" >&2
  usage >&2
  exit 1
fi

ID="$2"
shift 2

if ! (LC_ALL=C; [[ "$ID" =~ ^[a-z][a-z0-9-]*$ ]]) || [[ "$ID" == *- ]] || [[ "$ID" == *--* ]]; then
  echo "error: id '$ID' must be lowercase kebab-case (^[a-z][a-z0-9-]*$, no trailing or repeated hyphens)" >&2
  exit 1
fi

if [ "${#ID}" -gt 64 ]; then
  echo "error: id '$ID' exceeds 64 characters" >&2
  exit 1
fi

IFS='-' read -r -a PARTS <<< "$ID"
PASCAL=""
DEFAULT_TITLE=""
for part in "${PARTS[@]}"; do
  cap="$(capitalize "$part")"
  PASCAL+="$cap"
  if [ -n "$DEFAULT_TITLE" ]; then
    DEFAULT_TITLE+=" "
  fi
  DEFAULT_TITLE+="$cap"
done

SNAKE="${ID//-/_}"
UPPER="$(printf '%s' "$SNAKE" | LC_ALL=C tr '[:lower:]' '[:upper:]')"
TITLE="${*:-$DEFAULT_TITLE}"
TITLE_ESC="$(escape_quoted "$TITLE")"

if [ "$LANE" = "plugin" ]; then
  TARGET="$ROOT/plugins/$ID"
  if [ -e "$TARGET" ]; then
    echo "error: '$TARGET' already exists; refusing to overwrite" >&2
    exit 1
  fi

  mkdir -p "$TARGET/locales"

  cat <<EOF > "$TARGET/plugin.toml"
#:schema ../plugin.schema.json
id = "${ID}"
name = "${TITLE_ESC}"
version = "0.1.0"
contract = "1.4"
description = "${TITLE_ESC} panel and command"
EOF

  cat <<EOF > "$TARGET/locales/en.json"
{
  "title": "${TITLE_ESC}",
  "description": "Open the ${TITLE_ESC} panel",
  "category.view": "View",
  "command.open": "Open ${TITLE_ESC}",
  "status.count": "Clicks: {{count}}",
  "action.increment": "Increment"
}
EOF

  cat <<EOF > "$TARGET/init.luau"
--!strict
-- ${TITLE} plugin (\`plugins/${ID}\`): Lane A UI feature hot-reloaded under \`--plugin-hot\`.

local plugin = require("@opencode/plugin")
local ui = require("@opencode/ui")

local PANEL = "main"

return plugin.defineWindow(function(ctx)
    local I18n = ctx:GetService("I18n")
    local Layout = ctx:GetService("Layout")
    local Theme = ctx:GetService("Theme")
    local changed = ctx:CreateSignal()
    local count = 0

    ctx:BindHotState(function()
        return { count = count }
    end, function(saved)
        if type(saved) == "table" and type(saved.count) == "number" then
            count = saved.count
        end
    end)

    ctx:AddCommand({
        id = "open",
        title = I18n:T("command.open"),
        description = I18n:T("description"),
        group = I18n:T("category.view"),
        section = "session",
        requiresSession = true,
        run = function()
            Layout:OpenPanel(PANEL)
            return nil
        end,
    })

    ctx:AddPanel({
        id = PANEL,
        title = I18n:T("title"),
        description = I18n:T("description"),
        icon = "info",
        render = function(view, props)
            local version = \`{changed:Version()}:{I18n:Locale().id}:{Theme:Current().id}:{props.sessionId}\`
            return view:Memo(\`${ID}-panel-{props.sessionId}\`, version, function()
                return ui.col({ key = "${ID}-root", w = "full", h = "full", pad = 16, gap = 12 }, {
                    ui.text(I18n:T("title"), { size = 15, weight = "semibold", color = "text-strong" }),
                    ui.text(I18n:T("status.count", { count = count }), { size = 13, color = "text-weak" }),
                    ui.row({ key = "${ID}-actions", gap = 8 }, {
                        ui.button({
                            key = "${ID}-increment",
                            text = I18n:T("action.increment"),
                            variant = "primary",
                            size = "small",
                            onPress = view:Handler(function()
                                count += 1
                                changed:Fire()
                            end),
                        }),
                    }),
                })
            end)
        end,
    })
end)
EOF

  cat <<EOF
Created Lane A Luau plugin:
  plugins/${ID}/plugin.toml
  plugins/${ID}/init.luau
  plugins/${ID}/locales/en.json

Next step (0 ms Rust rebuild; hot reloads on save):
  scripts/dev-fast.sh
EOF
  exit 0
fi

TARGET="$ROOT/features/$ID"
ADAPTER="$ROOT/apps/desktop/src/surfaces/${SNAKE}.rs"
ADAPTER_DIR="$ROOT/apps/desktop/src/surfaces/${SNAKE}"
ROOT_TOML="$ROOT/Cargo.toml"
DESKTOP_TOML="$ROOT/apps/desktop/Cargo.toml"
SURFACES_MOD="$ROOT/apps/desktop/src/surfaces/mod.rs"
EXPORTS_RS="$ROOT/apps/desktop/src/exports.rs"
KERNEL_RS="$ROOT/apps/desktop/src/kernel.rs"

if [ -e "$TARGET" ]; then
  echo "error: '$TARGET' already exists; refusing to overwrite" >&2
  exit 1
fi
if [ -e "$ADAPTER" ] || [ -e "$ADAPTER_DIR" ]; then
  echo "error: desktop adapter for '${SNAKE}' already exists under apps/desktop/src/surfaces/; refusing to overwrite" >&2
  exit 1
fi
if [ -f "$ROOT_TOML" ] && grep -Fq "\"features/${ID}\"" "$ROOT_TOML"; then
  echo "error: 'features/${ID}' is already listed in Cargo.toml" >&2
  exit 1
fi
if [ -f "$DESKTOP_TOML" ] && grep -Eq "^opencode-feature-${ID}[[:space:]]*=" "$DESKTOP_TOML"; then
  echo "error: 'opencode-feature-${ID}' is already listed in apps/desktop/Cargo.toml" >&2
  exit 1
fi

mkdir -p "$TARGET/src"

cat <<EOF > "$TARGET/Cargo.toml"
[package]
name = "opencode-feature-${ID}"
version = "0.0.1"
description = "${TITLE_ESC} feature crate for Brocode"
edition.workspace = true
license.workspace = true
publish = false

[dependencies]
opencode-kernel = { path = "../../crates/opencode-kernel" }
rune.workspace = true

[lints]
workspace = true
EOF

cat <<EOF > "$TARGET/src/lib.rs"
//! ${TITLE} feature crate (\`features/${ID}\`).
//!
//! Keep the host boundary object-safe (\`dyn ${PASCAL}Host\`) so \`apps/desktop\` does not
//! re-monomorphize this crate's codegen, and keep per-window state in
//! \`opencode_kernel::StateSlots\` instead of adding fields to \`struct App\`.
//! Desktop wires \`${PASCAL}Host\` and \`Feature<App>\` in \`apps/desktop/src/surfaces/${SNAKE}.rs\`.

#![forbid(unsafe_code)]

/// Explicit state machine phase for ${TITLE}.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ${PASCAL}Phase {
    #[default]
    Idle,
    NeedsRedraw,
}

/// Inputs that advance [\`${PASCAL}Phase\`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ${PASCAL}Event {
    RequestRedraw,
    Tick,
}

/// Effects returned by [\`step\`] for the caller to apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ${PASCAL}Effect {
    Redraw,
}

/// Advances [\`${PASCAL}Phase\`] by one event and returns any effect for the host to perform.
pub fn step(phase: &mut ${PASCAL}Phase, event: ${PASCAL}Event) -> Option<${PASCAL}Effect> {
    match (*phase, event) {
        (_, ${PASCAL}Event::RequestRedraw) => {
            *phase = ${PASCAL}Phase::NeedsRedraw;
            None
        }
        (${PASCAL}Phase::NeedsRedraw, ${PASCAL}Event::Tick) => {
            *phase = ${PASCAL}Phase::Idle;
            Some(${PASCAL}Effect::Redraw)
        }
        (${PASCAL}Phase::Idle, ${PASCAL}Event::Tick) => None,
    }
}

/// Per-window tracked state stored in \`opencode_kernel::StateSlots\`.
#[derive(Debug, Default)]
pub struct ${PASCAL}State {
    pub phase: ${PASCAL}Phase,
}

/// Object-safe host boundary implemented by the desktop adapter (\`apps/desktop/src/surfaces/${SNAKE}.rs\`).
pub trait ${PASCAL}Host: opencode_kernel::AppEnv + opencode_kernel::AppFeedback + opencode_kernel::AppOverlays {
    fn state(&self) -> &${PASCAL}State;
    fn state_mut(&mut self) -> &mut ${PASCAL}State;
}

/// Non-generic per-frame poll wired into \`opencode_kernel::Phase::Ingest\`.
///
/// Returns \`true\` when state changed and the window should redraw.
pub fn poll(host: &mut dyn ${PASCAL}Host, tx: &mut rune::Tx) -> bool {
    match step(&mut host.state_mut().phase, ${PASCAL}Event::Tick) {
        Some(${PASCAL}Effect::Redraw) => {
            tx.instant();
            true
        }
        None => false,
    }
}
EOF

cat <<EOF > "$ADAPTER"
//! Desktop host adapter for \`opencode-feature-${ID}\` (\`features/${ID}\`).

pub use opencode_feature_${SNAKE}::*;

use opencode_kernel::{Feature, KernelBuilder, Slot};

use crate::App;

static ${UPPER}: Slot<${PASCAL}State> = Slot::new();

impl ${PASCAL}Host for App {
    fn state(&self) -> &${PASCAL}State {
        self.kernel.slots.get(&${UPPER})
    }

    fn state_mut(&mut self) -> &mut ${PASCAL}State {
        self.kernel.slots.get_mut(&${UPPER})
    }
}

/// Kernel feature registration for ${TITLE}.
pub struct ${PASCAL}Feature;

impl Feature<App> for ${PASCAL}Feature {
    fn id(&self) -> &'static str {
        "feature.${ID}"
    }

    fn register(&self, builder: &mut KernelBuilder<App>) {
        builder.register_slot(&${UPPER});
        // Pick the stage matching this poll's inputs/consumers and chain \`.after("<dep>")\` if it
        // shares state with another system in the same stage.
        builder.schedule(crate::tick::stage::INGEST_LOCAL, "${ID}.poll", |s, _, tx| poll(s, tx));
        // Register commands, modal overlays, or Escape layers here without editing central switchboards:
        // builder.add_command(opencode_kernel::CommandEntry::<App>::from_key(...));
        // builder.register_overlay(opencode_kernel::OverlayRegistration::<App>::dialog("dialog.${ID}", crate::overlays::layer::DIALOG, ...));
        // builder.register_escape_layer(opencode_kernel::EscapeLayer::new("escape.${ID}", crate::overlays::escape_band::PAGE_POPOVER, ...));
    }
}
EOF

UPDATED=()
MANUAL=()

# 1. Cargo.toml ([workspace] members)
tmp="$(mktemp)"
if [ -f "$ROOT_TOML" ] && LC_ALL=C awk -v id="$ID" '
  BEGIN { in_members = 0; seen_feat = 0; inserted = 0; entry = "    \"features/" id "\"," }
  /^[ \t]*members[ \t]*=[ \t]*\[/ { in_members = 1; print; next }
  in_members && /^[ \t]*"features\/[a-z0-9-]+",[ \t]*$/ {
    cur = $0
    sub(/^[ \t]*"features\//, "", cur)
    sub(/",[ \t]*$/, "", cur)
    if (!inserted && cur > id) {
      print entry
      inserted = 1
    }
    seen_feat = 1
    print
    next
  }
  in_members && seen_feat && !inserted {
    print entry
    inserted = 1
  }
  in_members && /^[ \t]*\]/ { in_members = 0 }
  { print }
  END { if (!inserted) exit 1 }
' "$ROOT_TOML" > "$tmp"; then
  mv "$tmp" "$ROOT_TOML"
  UPDATED+=("Cargo.toml")
else
  rm -f "$tmp"
  MANUAL+=("Cargo.toml ([workspace] members): add \`\"features/${ID}\",\`")
fi

# 2. apps/desktop/Cargo.toml ([dependencies])
tmp="$(mktemp)"
if [ -f "$DESKTOP_TOML" ] && LC_ALL=C awk -v id="$ID" '
  BEGIN { seen_feat = 0; inserted = 0; entry = "opencode-feature-" id " = { path = \"../../features/" id "\" }" }
  /^opencode-feature-[a-z0-9-]+[ \t]*=[ \t]*\{[ \t]*path[ \t]*=[ \t]*"\.\.\/\.\.\/features\// {
    cur = $0
    sub(/^opencode-feature-/, "", cur)
    sub(/[ \t]*=.*/, "", cur)
    if (!inserted && cur > id) {
      print entry
      inserted = 1
    }
    seen_feat = 1
    print
    next
  }
  seen_feat && !inserted {
    print entry
    inserted = 1
  }
  { print }
  END { if (!inserted) exit 1 }
' "$DESKTOP_TOML" > "$tmp"; then
  mv "$tmp" "$DESKTOP_TOML"
  UPDATED+=("apps/desktop/Cargo.toml")
else
  rm -f "$tmp"
  MANUAL+=("apps/desktop/Cargo.toml ([dependencies]): add \`opencode-feature-${ID} = { path = \"../../features/${ID}\" }\`")
fi

# 3. apps/desktop/src/surfaces/mod.rs
tmp="$(mktemp)"
if [ -f "$SURFACES_MOD" ] && LC_ALL=C awk -v snake="$SNAKE" '
  BEGIN { inserted = 0; buf_n = 0; entry = "pub mod " snake ";" }
  /^#\[.*\]$/ {
    buf[++buf_n] = $0
    next
  }
  /^pub mod [a-z0-9_]+;$/ {
    cur = $0
    sub(/^pub mod /, "", cur)
    sub(/;$/, "", cur)
    if (!inserted && cur > snake) {
      print entry
      inserted = 1
    }
    for (i = 1; i <= buf_n; i++) print buf[i]
    buf_n = 0
    print
    next
  }
  {
    for (i = 1; i <= buf_n; i++) print buf[i]
    buf_n = 0
    print
  }
  END {
    for (i = 1; i <= buf_n; i++) print buf[i]
    if (!inserted) {
      print entry
      inserted = 1
    }
  }
' "$SURFACES_MOD" > "$tmp"; then
  mv "$tmp" "$SURFACES_MOD"
  UPDATED+=("apps/desktop/src/surfaces/mod.rs")
else
  rm -f "$tmp"
  MANUAL+=("apps/desktop/src/surfaces/mod.rs: add \`pub mod ${SNAKE};\`")
fi

# 4. apps/desktop/src/exports.rs
tmp="$(mktemp)"
if [ -f "$EXPORTS_RS" ] && LC_ALL=C awk -v snake="$SNAKE" '
  BEGIN { updated = 0 }
  /^pub\(crate\) use crate::surfaces::\{[^}]+\};$/ {
    inner = $0
    sub(/^pub\(crate\) use crate::surfaces::\{/, "", inner)
    sub(/\};$/, "", inner)
    n = split(inner, raw, ",")
    count = 0
    found = 0
    for (i = 1; i <= n; i++) {
      item = raw[i]
      gsub(/^[ \t]+|[ \t]+$/, "", item)
      if (item == "") continue
      if (item == snake) found = 1
      items[++count] = item
    }
    if (!found) items[++count] = snake
    for (i = 2; i <= count; i++) {
      key = items[i]
      j = i - 1
      while (j >= 1 && items[j] > key) {
        items[j + 1] = items[j]
        j--
      }
      items[j + 1] = key
    }
    line = "pub(crate) use crate::surfaces::{"
    for (i = 1; i <= count; i++) {
      if (i > 1) line = line ", "
      line = line items[i]
    }
    print line "};"
    updated = 1
    next
  }
  { print }
  END { if (!updated) exit 1 }
' "$EXPORTS_RS" > "$tmp"; then
  mv "$tmp" "$EXPORTS_RS"
  UPDATED+=("apps/desktop/src/exports.rs")
else
  rm -f "$tmp"
  MANUAL+=("apps/desktop/src/exports.rs: add \`${SNAKE}\` to \`pub(crate) use crate::surfaces::{...};\`")
fi

# 5. apps/desktop/src/kernel.rs
tmp="$(mktemp)"
if [ -f "$KERNEL_RS" ] && LC_ALL=C awk -v snake="$SNAKE" -v pascal="$PASCAL" '
  BEGIN { inserted = 0; entry = "    b.with_feature(&crate::surfaces::" snake "::" pascal "Feature);" }
  /^    b\.with_feature\(&crate::plugin_host::PluginHostFeature\);$/ {
    if (!inserted) {
      print entry
      inserted = 1
    }
    print
    next
  }
  { print }
  END { if (!inserted) exit 1 }
' "$KERNEL_RS" > "$tmp"; then
  mv "$tmp" "$KERNEL_RS"
  UPDATED+=("apps/desktop/src/kernel.rs")
else
  rm -f "$tmp"
  MANUAL+=("apps/desktop/src/kernel.rs (inside build()): add \`b.with_feature(&crate::surfaces::${SNAKE}::${PASCAL}Feature);\`")
fi

echo "Created Lane B Rust feature:"
echo "  features/${ID}/Cargo.toml"
echo "  features/${ID}/src/lib.rs"
echo "  apps/desktop/src/surfaces/${SNAKE}.rs"
if [ "${#UPDATED[@]}" -gt 0 ]; then
  echo ""
  echo "Wired into:"
  for f in "${UPDATED[@]}"; do
    echo "  ${f}"
  done
fi
if [ "${#MANUAL[@]}" -gt 0 ]; then
  echo ""
  echo "Manual steps remaining:"
  for step in "${MANUAL[@]}"; do
    echo "  - ${step}"
  done
fi
echo ""
echo "Note: if this feature belongs under dialogs/, session/, shell/, or workspace/ instead of surfaces/,"
echo "move apps/desktop/src/surfaces/${SNAKE}.rs there and update its mod.rs, exports.rs, and kernel.rs paths."
echo ""
echo "Verify with:"
echo "  cargo check --workspace"
echo "  scripts/arch-check.sh"
