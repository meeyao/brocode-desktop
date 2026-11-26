<#
.SYNOPSIS
  Zero-thought feature scaffolder for anomalyco/opencode-rune (Lane A Luau plugin or Lane B Rust feature).

.DESCRIPTION
  Creates a ready-to-run Lane A Luau plugin under plugins/<id>/ (default) or a decoupled
  Lane B Rust feature crate under features/<id>/ plus its desktop host adapter under
  apps/desktop/src/surfaces/<snake>.rs and wires it into the workspace and kernel.

.EXAMPLE
  pwsh scripts/new-feature.ps1 plugin my-panel "My Panel"
  pwsh scripts/new-feature.ps1 feature my-engine "My Engine"
#>
param(
  [Parameter(Position = 0)]
  [string]$Lane,
  [Parameter(Position = 1)]
  [string]$Id,
  [Parameter(Position = 2, ValueFromRemainingArguments = $true)]
  [string[]]$TitleWords,
  [Alias('h')]
  [switch]$Help
)

$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent

function Show-Usage {
  $text = @'
Usage:
  pwsh scripts/new-feature.ps1 plugin <id> [Title]   # Lane A (default): Luau plugin under plugins/<id>/
  pwsh scripts/new-feature.ps1 feature <id> [Title]  # Lane B: Rust feature crate + desktop adapter + wiring

Rules:
  - <id> must be lowercase kebab-case (^[a-z][a-z0-9-]*$, max 64 chars)
  - Existing target directories and adapter files are never overwritten
'@
  Write-Host $text
}

function Write-Utf8Lf {
  param(
    [Parameter(Mandatory = $true)]
    [string]$Path,
    [Parameter(Mandatory = $true)]
    [string]$Content
  )
  $normalized = $Content -replace "`r`n", "`n"
  if (-not $normalized.EndsWith("`n")) {
    $normalized += "`n"
  }
  $utf8NoBom = [System.Text.UTF8Encoding]::new($false)
  [System.IO.File]::WriteAllText($Path, $normalized, $utf8NoBom)
}

function Read-LinesLf {
  param(
    [Parameter(Mandatory = $true)]
    [string]$Path
  )
  $raw = [System.IO.File]::ReadAllText($Path, [System.Text.UTF8Encoding]::new($false))
  $normalized = $raw -replace "`r`n", "`n"
  if ($normalized.EndsWith("`n")) {
    $normalized = $normalized.Substring(0, $normalized.Length - 1)
  }
  if ($normalized.Length -eq 0) {
    return @()
  }
  return $normalized.Split("`n")
}

function Expand-Template {
  param(
    [string]$Template,
    [hashtable]$Tokens
  )
  $result = $Template
  foreach ($key in $Tokens.Keys) {
    $result = $result.Replace($key, [string]$Tokens[$key])
  }
  return $result
}

if ($Help -or ($Lane -in @('-h', '--help', 'help')) -or ([string]::IsNullOrEmpty($Lane) -and ($TitleWords -contains '--help' -or $TitleWords -contains '-h'))) {
  Show-Usage
  exit 0
}

if ([string]::IsNullOrEmpty($Lane)) {
  Show-Usage
  exit 1
}

if ($Lane -cne 'plugin' -and $Lane -cne 'feature') {
  [Console]::Error.WriteLine("error: unknown lane '$Lane' (expected 'plugin' or 'feature')")
  Show-Usage
  exit 1
}

if ([string]::IsNullOrEmpty($Id)) {
  [Console]::Error.WriteLine("error: missing <id> for lane '$Lane'")
  Show-Usage
  exit 1
}

if (($Id -cnotmatch '^[a-z][a-z0-9-]*$') -or $Id.EndsWith('-') -or $Id.Contains('--')) {
  [Console]::Error.WriteLine("error: id '$Id' must be lowercase kebab-case (^[a-z][a-z0-9-]*$, no trailing or repeated hyphens)")
  exit 1
}

if ($Id.Length -gt 64) {
  [Console]::Error.WriteLine("error: id '$Id' exceeds 64 characters")
  exit 1
}

$parts = $Id.Split('-')
$capParts = foreach ($part in $parts) {
  $part.Substring(0, 1).ToUpperInvariant() + $part.Substring(1)
}
$pascal = ($capParts -join '')
$defaultTitle = ($capParts -join ' ')
$snake = $Id.Replace('-', '_')
$upper = $snake.ToUpperInvariant()

$title = if ($TitleWords -and $TitleWords.Count -gt 0) {
  ($TitleWords -join ' ').Trim()
} else {
  $defaultTitle
}
if ([string]::IsNullOrWhiteSpace($title)) {
  $title = $defaultTitle
}
$titleEsc = $title.Replace('\', '\\').Replace('"', '\"')

$tokens = @{
  '__ID__'        = $Id
  '__SNAKE__'     = $snake
  '__UPPER__'     = $upper
  '__PASCAL__'    = $pascal
  '__TITLE__'     = $title
  '__TITLE_ESC__' = $titleEsc
}

if ($Lane -ceq 'plugin') {
  $target = Join-Path (Join-Path $root 'plugins') $Id
  if (Test-Path -LiteralPath $target) {
    [Console]::Error.WriteLine("error: '$target' already exists; refusing to overwrite")
    exit 1
  }

  $localesDir = Join-Path $target 'locales'
  [void](New-Item -ItemType Directory -Path $localesDir -Force)

  $pluginToml = @'
#:schema ../plugin.schema.json
id = "__ID__"
name = "__TITLE_ESC__"
version = "0.1.0"
contract = "1.4"
description = "__TITLE_ESC__ panel and command"
'@

  $enJson = @'
{
  "title": "__TITLE_ESC__",
  "description": "Open the __TITLE_ESC__ panel",
  "category.view": "View",
  "command.open": "Open __TITLE_ESC__",
  "status.count": "Clicks: {{count}}",
  "action.increment": "Increment"
}
'@

  $initLuau = @'
--!strict
-- __TITLE__ plugin (`plugins/__ID__`): Lane A UI feature hot-reloaded under `--plugin-hot`.

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
            local version = `{changed:Version()}:{I18n:Locale().id}:{Theme:Current().id}:{props.sessionId}`
            return view:Memo(`__ID__-panel-{props.sessionId}`, version, function()
                return ui.col({ key = "__ID__-root", w = "full", h = "full", pad = 16, gap = 12 }, {
                    ui.text(I18n:T("title"), { size = 15, weight = "semibold", color = "text-strong" }),
                    ui.text(I18n:T("status.count", { count = count }), { size = 13, color = "text-weak" }),
                    ui.row({ key = "__ID__-actions", gap = 8 }, {
                        ui.button({
                            key = "__ID__-increment",
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
'@

  Write-Utf8Lf -Path (Join-Path $target 'plugin.toml') -Content (Expand-Template $pluginToml $tokens)
  Write-Utf8Lf -Path (Join-Path $localesDir 'en.json') -Content (Expand-Template $enJson $tokens)
  Write-Utf8Lf -Path (Join-Path $target 'init.luau') -Content (Expand-Template $initLuau $tokens)

  Write-Host "Created Lane A Luau plugin:"
  Write-Host "  plugins/$Id/plugin.toml"
  Write-Host "  plugins/$Id/init.luau"
  Write-Host "  plugins/$Id/locales/en.json"
  Write-Host ""
  Write-Host "Next step (0 ms Rust rebuild; hot reloads on save):"
  Write-Host "  pwsh scripts/dev-fast.ps1"
  exit 0
}

$target = Join-Path (Join-Path $root 'features') $Id
$adapter = Join-Path $root "apps/desktop/src/surfaces/$snake.rs"
$adapterDir = Join-Path $root "apps/desktop/src/surfaces/$snake"
$rootToml = Join-Path $root 'Cargo.toml'
$desktopToml = Join-Path $root 'apps/desktop/Cargo.toml'
$surfacesMod = Join-Path $root 'apps/desktop/src/surfaces/mod.rs'
$exportsRs = Join-Path $root 'apps/desktop/src/exports.rs'
$kernelRs = Join-Path $root 'apps/desktop/src/kernel.rs'

if (Test-Path -LiteralPath $target) {
  [Console]::Error.WriteLine("error: '$target' already exists; refusing to overwrite")
  exit 1
}
if ((Test-Path -LiteralPath $adapter) -or (Test-Path -LiteralPath $adapterDir)) {
  [Console]::Error.WriteLine("error: desktop adapter for '$snake' already exists under apps/desktop/src/surfaces/; refusing to overwrite")
  exit 1
}
if ((Test-Path -LiteralPath $rootToml) -and ([System.IO.File]::ReadAllText($rootToml).Contains("`"features/$Id`""))) {
  [Console]::Error.WriteLine("error: 'features/$Id' is already listed in Cargo.toml")
  exit 1
}
if ((Test-Path -LiteralPath $desktopToml) -and ([regex]::IsMatch([System.IO.File]::ReadAllText($desktopToml), "(?m)^opencode-feature-$([regex]::Escape($Id))\s*="))) {
  [Console]::Error.WriteLine("error: 'opencode-feature-$Id' is already listed in apps/desktop/Cargo.toml")
  exit 1
}

$srcDir = Join-Path $target 'src'
[void](New-Item -ItemType Directory -Path $srcDir -Force)

$cargoToml = @'
[package]
name = "opencode-feature-__ID__"
version = "0.0.1"
description = "__TITLE_ESC__ feature crate for Brocode"
edition.workspace = true
license.workspace = true
publish = false

[dependencies]
opencode-kernel = { path = "../../crates/opencode-kernel" }
rune.workspace = true

[lints]
workspace = true
'@

$libRs = @'
//! __TITLE__ feature crate (`features/__ID__`).
//!
//! Keep the host boundary object-safe (`dyn __PASCAL__Host`) so `apps/desktop` does not
//! re-monomorphize this crate's codegen, and keep per-window state in
//! `opencode_kernel::StateSlots` instead of adding fields to `struct App`.
//! Desktop wires `__PASCAL__Host` and `Feature<App>` in `apps/desktop/src/surfaces/__SNAKE__.rs`.

#![forbid(unsafe_code)]

/// Explicit state machine phase for __TITLE__.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum __PASCAL__Phase {
    #[default]
    Idle,
    NeedsRedraw,
}

/// Inputs that advance [`__PASCAL__Phase`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum __PASCAL__Event {
    RequestRedraw,
    Tick,
}

/// Effects returned by [`step`] for the caller to apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum __PASCAL__Effect {
    Redraw,
}

/// Advances [`__PASCAL__Phase`] by one event and returns any effect for the host to perform.
pub fn step(phase: &mut __PASCAL__Phase, event: __PASCAL__Event) -> Option<__PASCAL__Effect> {
    match (*phase, event) {
        (_, __PASCAL__Event::RequestRedraw) => {
            *phase = __PASCAL__Phase::NeedsRedraw;
            None
        }
        (__PASCAL__Phase::NeedsRedraw, __PASCAL__Event::Tick) => {
            *phase = __PASCAL__Phase::Idle;
            Some(__PASCAL__Effect::Redraw)
        }
        (__PASCAL__Phase::Idle, __PASCAL__Event::Tick) => None,
    }
}

/// Per-window tracked state stored in `opencode_kernel::StateSlots`.
#[derive(Debug, Default)]
pub struct __PASCAL__State {
    pub phase: __PASCAL__Phase,
}

/// Object-safe host boundary implemented by the desktop adapter (`apps/desktop/src/surfaces/__SNAKE__.rs`).
pub trait __PASCAL__Host: opencode_kernel::AppEnv + opencode_kernel::AppFeedback + opencode_kernel::AppOverlays {
    fn state(&self) -> &__PASCAL__State;
    fn state_mut(&mut self) -> &mut __PASCAL__State;
}

/// Non-generic per-frame poll wired into `opencode_kernel::Phase::Ingest`.
///
/// Returns `true` when state changed and the window should redraw.
pub fn poll(host: &mut dyn __PASCAL__Host, tx: &mut rune::Tx) -> bool {
    match step(&mut host.state_mut().phase, __PASCAL__Event::Tick) {
        Some(__PASCAL__Effect::Redraw) => {
            tx.instant();
            true
        }
        None => false,
    }
}
'@

$adapterRs = @'
//! Desktop host adapter for `opencode-feature-__ID__` (`features/__ID__`).

pub use opencode_feature___SNAKE__::*;

use opencode_kernel::{Feature, KernelBuilder, Slot};

use crate::App;

static __UPPER__: Slot<__PASCAL__State> = Slot::new();

impl __PASCAL__Host for App {
    fn state(&self) -> &__PASCAL__State {
        self.kernel.slots.get(&__UPPER__)
    }

    fn state_mut(&mut self) -> &mut __PASCAL__State {
        self.kernel.slots.get_mut(&__UPPER__)
    }
}

/// Kernel feature registration for __TITLE__.
pub struct __PASCAL__Feature;

impl Feature<App> for __PASCAL__Feature {
    fn id(&self) -> &'static str {
        "feature.__ID__"
    }

    fn register(&self, builder: &mut KernelBuilder<App>) {
        builder.register_slot(&__UPPER__);
        // Pick the stage matching this poll's inputs/consumers and chain `.after("<dep>")` if it
        // shares state with another system in the same stage.
        builder.schedule(crate::tick::stage::INGEST_LOCAL, "__ID__.poll", |s, _, tx| poll(s, tx));
        // Register commands, modal overlays, or Escape layers here without editing central switchboards:
        // builder.add_command(opencode_kernel::CommandEntry::<App>::from_key(...));
        // builder.register_overlay(opencode_kernel::OverlayRegistration::<App>::dialog("dialog.$Id", crate::overlays::layer::DIALOG, ...));
        // builder.register_escape_layer(opencode_kernel::EscapeLayer::new("escape.$Id", crate::overlays::escape_band::PAGE_POPOVER, ...));
    }
}
'@

Write-Utf8Lf -Path (Join-Path $target 'Cargo.toml') -Content (Expand-Template $cargoToml $tokens)
Write-Utf8Lf -Path (Join-Path $srcDir 'lib.rs') -Content (Expand-Template $libRs $tokens)
Write-Utf8Lf -Path $adapter -Content (Expand-Template $adapterRs $tokens)

$updated = [System.Collections.Generic.List[string]]::new()
$manual = [System.Collections.Generic.List[string]]::new()
$ord = [System.StringComparer]::Ordinal

# 1. Cargo.toml ([workspace] members)
if (Test-Path -LiteralPath $rootToml) {
  $lines = Read-LinesLf $rootToml
  $out = [System.Collections.Generic.List[string]]::new()
  $inMembers = $false
  $seenFeat = $false
  $inserted = $false
  $entry = "    `"features/$Id`","
  foreach ($line in $lines) {
    if ($line -cmatch '^[ \t]*members[ \t]*=[ \t]*\[') {
      $inMembers = $true
      $out.Add($line)
      continue
    }
    if ($inMembers -and $line -cmatch '^[ \t]*"features/([a-z0-9-]+)",[ \t]*$') {
      $cur = $Matches[1]
      if (-not $inserted -and ($ord.Compare($cur, $Id) -gt 0)) {
        $out.Add($entry)
        $inserted = $true
      }
      $seenFeat = $true
      $out.Add($line)
      continue
    }
    if ($inMembers -and $seenFeat -and -not $inserted) {
      $out.Add($entry)
      $inserted = $true
    }
    if ($inMembers -and $line -cmatch '^[ \t]*\]') {
      $inMembers = $false
    }
    $out.Add($line)
  }
  if ($inserted) {
    Write-Utf8Lf -Path $rootToml -Content ($out -join "`n")
    $updated.Add('Cargo.toml')
  } else {
    $manual.Add("Cargo.toml ([workspace] members): add `` `"features/$Id`", ``")
  }
} else {
  $manual.Add("Cargo.toml ([workspace] members): add `` `"features/$Id`", ``")
}

# 2. apps/desktop/Cargo.toml ([dependencies])
if (Test-Path -LiteralPath $desktopToml) {
  $lines = Read-LinesLf $desktopToml
  $out = [System.Collections.Generic.List[string]]::new()
  $seenFeat = $false
  $inserted = $false
  $entry = "opencode-feature-$Id = { path = `"../../features/$Id`" }"
  foreach ($line in $lines) {
    if ($line -cmatch '^opencode-feature-([a-z0-9-]+)[ \t]*=[ \t]*\{[ \t]*path[ \t]*=[ \t]*"\.\./\.\./features/') {
      $cur = $Matches[1]
      if (-not $inserted -and ($ord.Compare($cur, $Id) -gt 0)) {
        $out.Add($entry)
        $inserted = $true
      }
      $seenFeat = $true
      $out.Add($line)
      continue
    }
    if ($seenFeat -and -not $inserted) {
      $out.Add($entry)
      $inserted = $true
    }
    $out.Add($line)
  }
  if ($inserted) {
    Write-Utf8Lf -Path $desktopToml -Content ($out -join "`n")
    $updated.Add('apps/desktop/Cargo.toml')
  } else {
    $manual.Add("apps/desktop/Cargo.toml ([dependencies]): add ``opencode-feature-$Id = { path = `"../../features/$Id`" }``")
  }
} else {
  $manual.Add("apps/desktop/Cargo.toml ([dependencies]): add ``opencode-feature-$Id = { path = `"../../features/$Id`" }``")
}

# 3. apps/desktop/src/surfaces/mod.rs
if (Test-Path -LiteralPath $surfacesMod) {
  $lines = Read-LinesLf $surfacesMod
  $out = [System.Collections.Generic.List[string]]::new()
  $buf = [System.Collections.Generic.List[string]]::new()
  $inserted = $false
  $entry = "pub mod $snake;"
  foreach ($line in $lines) {
    if ($line -cmatch '^#\[.*\]$') {
      $buf.Add($line)
      continue
    }
    if ($line -cmatch '^pub mod ([a-z0-9_]+);$') {
      $cur = $Matches[1]
      if (-not $inserted -and ($ord.Compare($cur, $snake) -gt 0)) {
        $out.Add($entry)
        $inserted = $true
      }
      foreach ($b in $buf) { $out.Add($b) }
      $buf.Clear()
      $out.Add($line)
      continue
    }
    foreach ($b in $buf) { $out.Add($b) }
    $buf.Clear()
    $out.Add($line)
  }
  foreach ($b in $buf) { $out.Add($b) }
  if (-not $inserted) {
    $out.Add($entry)
    $inserted = $true
  }
  Write-Utf8Lf -Path $surfacesMod -Content ($out -join "`n")
  $updated.Add('apps/desktop/src/surfaces/mod.rs')
} else {
  $manual.Add("apps/desktop/src/surfaces/mod.rs: add ``pub mod $snake;``")
}

# 4. apps/desktop/src/exports.rs
if (Test-Path -LiteralPath $exportsRs) {
  $lines = Read-LinesLf $exportsRs
  $out = [System.Collections.Generic.List[string]]::new()
  $patched = $false
  foreach ($line in $lines) {
    if ($line -cmatch '^pub\(crate\) use crate::surfaces::\{([^}]+)\};$') {
      $items = [System.Collections.Generic.List[string]]::new()
      foreach ($part in $Matches[1].Split(',')) {
        $trimmed = $part.Trim()
        if ($trimmed.Length -gt 0) {
          $items.Add($trimmed)
        }
      }
      if (-not $items.Contains($snake)) {
        $items.Add($snake)
      }
      $items.Sort($ord)
      $out.Add("pub(crate) use crate::surfaces::{$($items -join ', ')};")
      $patched = $true
      continue
    }
    $out.Add($line)
  }
  if ($patched) {
    Write-Utf8Lf -Path $exportsRs -Content ($out -join "`n")
    $updated.Add('apps/desktop/src/exports.rs')
  } else {
    $manual.Add("apps/desktop/src/exports.rs: add ``$snake`` to ``pub(crate) use crate::surfaces::{...};``")
  }
} else {
  $manual.Add("apps/desktop/src/exports.rs: add ``$snake`` to ``pub(crate) use crate::surfaces::{...};``")
}

# 5. apps/desktop/src/kernel.rs
if (Test-Path -LiteralPath $kernelRs) {
  $lines = Read-LinesLf $kernelRs
  $out = [System.Collections.Generic.List[string]]::new()
  $inserted = $false
  $entry = "    b.with_feature(&crate::surfaces::${snake}::${pascal}Feature);"
  foreach ($line in $lines) {
    if ($line -ceq '    b.with_feature(&crate::plugin_host::PluginHostFeature);') {
      if (-not $inserted) {
        $out.Add($entry)
        $inserted = $true
      }
      $out.Add($line)
      continue
    }
    $out.Add($line)
  }
  if ($inserted) {
    Write-Utf8Lf -Path $kernelRs -Content ($out -join "`n")
    $updated.Add('apps/desktop/src/kernel.rs')
  } else {
    $manual.Add("apps/desktop/src/kernel.rs (inside build()): add ``b.with_feature(&crate::surfaces::${snake}::${pascal}Feature);``")
  }
} else {
  $manual.Add("apps/desktop/src/kernel.rs (inside build()): add ``b.with_feature(&crate::surfaces::${snake}::${pascal}Feature);``")
}

Write-Host "Created Lane B Rust feature:"
Write-Host "  features/$Id/Cargo.toml"
Write-Host "  features/$Id/src/lib.rs"
Write-Host "  apps/desktop/src/surfaces/$snake.rs"
if ($updated.Count -gt 0) {
  Write-Host ""
  Write-Host "Wired into:"
  foreach ($f in $updated) {
    Write-Host "  $f"
  }
}
if ($manual.Count -gt 0) {
  Write-Host ""
  Write-Host "Manual steps remaining:"
  foreach ($step in $manual) {
    Write-Host "  - $step"
  }
}
Write-Host ""
Write-Host "Note: if this feature belongs under dialogs/, session/, shell/, or workspace/ instead of surfaces/,"
Write-Host "move apps/desktop/src/surfaces/$snake.rs there and update its mod.rs, exports.rs, and kernel.rs paths."
Write-Host ""
Write-Host "Verify with:"
Write-Host "  cargo check --workspace"
Write-Host "  scripts/arch-check.sh"
