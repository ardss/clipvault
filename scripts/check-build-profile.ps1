# check-build-profile.ps1 — guard against the 0.1.6 incident: [profile.release]
# silently ignored in a non-root member, lto/strip vanishing from release builds.
# Implements §3.5 ② only (the source-side ① lives in guards.rs
# profile_release_only_in_root). Runs `cargo build --release -v` and asserts
# the rustc invocation for the app bin actually carries the profile flags
# (-C lto=fat / -C strip=... / -C codegen-units=1). If the build is fresh, the
# bin entry point is touched so the app crate recompiles and the flags become
# visible (deps stay cached).
# Wired into release.yml after `npx tauri build`.
param(
    [string]$Bin = 'app',
    [string]$MainSrc = 'src-tauri/src/main.rs'
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot

# capture via cmd: PS 5.1 turns native stderr into a terminating error under
# $ErrorActionPreference='Stop' when redirected with 2>&1. cargo -v writes the
# rustc invocations to stderr.
function Get-BuildOutput {
    param([string]$Inner)
    cmd /c "cargo build --release -v --bin $Inner 2>&1" | Out-String
}
function Get-BinRustcLine {
    param([string]$Output)
    ($Output -split "`n" | Select-String -Pattern ('--crate-name ' + $Bin + ' ') | Select-Object -Last 1).Line
}

Write-Host "building release (verbose) to capture rustc flags..."
$build = Get-BuildOutput $Bin
$line = Get-BinRustcLine $build

if (-not $line -or $line -notmatch '-C\s*(lto|strip)') {
    Write-Host "no profile flags visible (fresh build) — touching $MainSrc to force the app crate to recompile..."
    Push-Location $root
    (Get-Item $MainSrc).LastWriteTime = Get-Date
    Pop-Location
    $build = Get-BuildOutput $Bin
    $line = Get-BinRustcLine $build
}

if (-not $line) {
    Write-Error "could not find a rustc invocation for the $Bin crate in build output — cannot assert profile flags"
    exit 1
}
Write-Host "rustc line (truncated):"
Write-Host ($line.Substring(0, [Math]::Min(300, $line.Length)))

$fail = $false
foreach ($flag in @('-C\s*lto=fat', '-C\s*strip', 'codegen-units')) {
    if ($line -match $flag) {
        Write-Host "flag OK: $flag"
    } else {
        Write-Host "flag MISSING: $flag"
        $fail = $true
    }
}
if ($fail) {
    Write-Host 'FAIL: release build is not built with the workspace [profile.release] flags — the 0.1.6 regression (profile silently dropped) has recurred'
    exit 1
}
Write-Host 'PASS: release build carries lto/strip/codegen-units from root [profile.release]'
