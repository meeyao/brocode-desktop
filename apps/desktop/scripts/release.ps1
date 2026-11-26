# Builds Brocode as it ships: `cargo build --release` (fat LTO and one codegen unit, from
# Cargo.toml's release profile) with the profile-guided optimisation profile of the target,
# `apps/desktop/pgo/<target>.profdata`, when there is one. package.ps1 builds the app through this,
# and so should any release build that times the app: without the profile it's 14-22 % slower per
# frame (notes in apps/desktop/pgo/README.md).
#
#   pwsh apps/desktop/scripts/release.ps1 [-Target <triple>] [-Cargo <command>] [-Package <name>] [-Features <list>] [-StaleProfileOk]
#
# A profile 20 or more app commits older than the code (or one whose training commit isn't in this
# clone's history) is stale: it keeps a quarter of its gain (a day of commits gave 4-8 % per frame
# instead of 23-30 %). Outside the release workflow this refuses to build with one: retrain it first
# (apps/desktop/scripts/pgo.ps1), or pass
# -StaleProfileOk (or set OPENCODE_RUNE_STALE_PROFILE_OK=1, which reaches it through package.ps1) for a
# build whose speed doesn't matter. The release workflow (OPENCODE_RUNE_BUILD_CHANNEL set) only warns.
#
# -Cargo runs cargo another way, such as a build throttle ("pwsh tools/cargo.ps1"). The profile
# rides on CARGO_TARGET_<TRIPLE>_RUSTFLAGS, which cargo joins with .cargo/config.toml's flags for
# that target. The target is always named (`--target`), as pgo.ps1 names it for the training
# build: cargo gives a crate built for a named target other symbol hashes than one built for the
# host, and a profile only applies to the functions it names. So the exe is in
# target/<triple>/release/.
#
# Two builds of the same code for people who measure it, chosen by the pull request's branch name
# (GITHUB_HEAD_REF) or a variable, and never by a cargo profile setting: cargo hashes the profile
# (lto, codegen units, debug info) into every symbol's name, and RUSTFLAGS are not in that hash.
#   - `pgo-train/...` (or OPENCODE_RUNE_PGO_GENERATE=1): the exe is instrumented (`-Cprofile-generate`)
#     instead of optimised with the profile. Its CI artifact is what a training run drives
#     (apps/desktop/pgo/README.md); never install it.
#   - a name with `symbols` (or OPENCODE_RUNE_SYMBOLS=1): line tables and a PDB beside the exe, with
#     the profile applied as in the build that ships.
param(
    [string] $Target = '',
    [string] $Cargo = 'cargo',
    [string] $Package = 'opencode-desktop',
    [string] $Features = '',
    [switch] $StaleProfileOk
)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '../../..')).Path
$hostTriple = ((rustc -vV | Select-String '^host: ').Line -replace '^host: ', '').Trim()
$triple = if ($Target) { $Target } else { $hostTriple }
$profdata = Join-Path $root "apps/desktop/pgo/$triple.profdata"
$name = 'CARGO_TARGET_' + ($triple.ToUpperInvariant() -replace '[-.]', '_') + '_RUSTFLAGS'
$before = [Environment]::GetEnvironmentVariable($name)
# Only a target that has a profile is instrumented (Windows ARM64 has none and stays as it is).
$instrument = ($env:OPENCODE_RUNE_PGO_GENERATE -eq '1' -or $env:GITHUB_HEAD_REF -match '^pgo-train/') -and (Test-Path $profdata)
$symbols = $env:OPENCODE_RUNE_SYMBOLS -eq '1' -or $env:GITHUB_HEAD_REF -match 'symbols'
if ($instrument) {
    Write-Host "profile-guided: an instrumented build of $triple for a training run (not for use)"
} elseif (Test-Path $profdata) {
    # Functions changed since the training run keep no counts and build as without a profile; LLVM
    # would warn once per such function.
    $stamp = Join-Path $root "apps/desktop/pgo/$triple.txt"
    $trained = if (Test-Path $stamp) { (Get-Content $stamp | Select-String '^commit ').Line -replace '^commit ', '' }
    # Commits to the app since the training commit; none when the stamp or that commit is missing
    # (a shallow clone, as CI's checkout is, has no history to count).
    $known = $false
    if ($trained) { git -C $root cat-file -e "$trained^{commit}" 2>$null; $known = $LASTEXITCODE -eq 0 }
    $since = if ($known) { [int](git -C $root rev-list --count "$trained..HEAD" -- apps features crates Cargo.toml Cargo.lock ':(exclude)apps/desktop/pgo/**') } else { $null }
    $age = if ($null -ne $since) { "$since commits to the app since" } else { 'its age unknown (the training commit is not in this clone)' }
    # The Rune rev the profile was trained at against the one this build pins. A profile names functions by their mangled
    # symbols, and the symbol hash of every crate from the Rune repo (rune, winit, wgpu, ...) and of every crate depending
    # on it changes with the pinned rev, so a profile trained at another rev names none of them: about 96 % of its counts.
    # Measured on Windows (shipping builds of one commit): such a build is as fast as one without a profile, and a profile
    # trained on the code it ships with is 11-14 % faster per stream commit.
    $runeTrained = if (Test-Path $stamp) { ((Get-Content $stamp | Select-String '^rune ').Line -replace '^rune ', '').Trim() }
    $runeNow = @([regex]::Matches((Get-Content (Join-Path $root 'Cargo.lock') -Raw), 'anomalyco/rune\?rev=([0-9a-f]{40})') | ForEach-Object { $_.Groups[1].Value } | Sort-Object -Unique)
    $orphaned = $runeTrained -and ($runeNow.Count -ne 1 -or $runeNow[0] -ne $runeTrained)
    if ($orphaned) { $age = "trained at Rune $($runeTrained.Substring(0, 8)), but this build pins Rune $(($runeNow | ForEach-Object { $_.Substring(0, 8) }) -join ', '), so it names none of Rune's functions or the app's; $age" }
    Write-Host "profile-guided: $triple.profdata, trained at $(if ($trained) { $trained.Substring(0, 12) } else { '?' }), $age"
    if ($null -eq $since -or $since -ge 20 -or $orphaned) {
        $why = "the profile-guided optimisation profile is stale ($age): it no longer names most of the code, so the build keeps a quarter of its speed-up. Retrain it (apps/desktop/scripts/pgo.ps1, apps/desktop/pgo/README.md)"
        $ok = $StaleProfileOk -or $env:OPENCODE_RUNE_STALE_PROFILE_OK -eq '1'
        if ($env:GITHUB_ACTIONS -eq 'true' -and $env:OPENCODE_RUNE_BUILD_CHANNEL) {
            # The release workflow's call (docs/releasing.md); refusing there is its owner's decision.
            if ($env:GITHUB_ACTIONS) { Write-Host "::warning::$why" } else { Write-Warning $why }
        } elseif ($ok) {
            Write-Warning "$why (building anyway: -StaleProfileOk)"
        } else {
            throw "$why; or pass -StaleProfileOk (OPENCODE_RUNE_STALE_PROFILE_OK=1) for a build whose speed doesn't matter"
        }
    }
} else {
    Write-Host "no profile for ${triple}: building without profile-guided optimisation"
}
try {
    $flags = @(
        if ($instrument) { '-Cprofile-generate=target/pgo-raw' } elseif (Test-Path $profdata) { "-Cprofile-use=$profdata"; '-Cllvm-args=-no-pgo-warn-mismatch' }
        # After cargo's own flags, so these win over the release profile's "no debug info".
        if ($symbols) { '-Cdebuginfo=line-tables-only'; '-Cstrip=none' }
    )
    if ($flags) { [Environment]::SetEnvironmentVariable($name, (@($before) + $flags | Where-Object { $_ }) -join ' ') }
    $parts = $Cargo -split ' '
    $targetArgs = @('--target', $triple)
    $featureArgs = if ($Features) { @('--features', $Features) } else { @() }
    & $parts[0] @($parts | Select-Object -Skip 1) build --release -p $Package @targetArgs @featureArgs
    if ($LASTEXITCODE -ne 0) { throw "cargo build --release failed" }
} finally {
    [Environment]::SetEnvironmentVariable($name, $before)
}
