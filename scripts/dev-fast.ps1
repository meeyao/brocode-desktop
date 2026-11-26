<#
.SYNOPSIS
  Fast edit-compile-run loop for Brocode on Windows.

.DESCRIPTION
  Runs `opencode-desktop` with the `dev-fast` profile (no PDB, 64 codegen units),
  skips Vello + BetterOffice + 6 MB font packs via `--no-default-features --features knobs`,
  and enables Luau hot reload (`--plugin-hot`).

.EXAMPLE
  pwsh scripts/dev-fast.ps1
  pwsh scripts/dev-fast.ps1 --server http://127.0.0.1:4096
  pwsh scripts/dev-fast.ps1 -Office
#>
param(
  [switch]$Office
)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$features = if ($Office -or $env:OPENCODE_DEV_OFFICE -eq '1') { 'knobs,office' } else { 'knobs' }

Push-Location $root
try {
  & cargo run --profile dev-fast -p brocode-desktop --no-default-features --features $features '--' --live --plugin-hot @Args
  exit $LASTEXITCODE
} finally {
  Pop-Location
}
