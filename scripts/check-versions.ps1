# check-versions.ps1 — five-way version consistency (TESTING_ARCHITECTURE_PLAN §3.5, RELEASE_CHECKLIST §1).
# Compares: package.json / src-tauri/Cargo.toml / Cargo.lock (root, package "app")
# / src-tauri/tauri.conf.json / CHANGELOG.md latest *published* section.
# CHANGELOG rule: "current version" = the most recent section whose header is
#   "## [x.y.z] - <date>" AND is neither [Unreleased] (no version) nor marked
#   Unpublished (CHANGELOG carries both, e.g. "[0.1.5] - Unpublished (...)").
# Exits non-zero on any mismatch. Wired into release.yml (tag flow).
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot

$versions = @{}

$versions['package.json'] = (Get-Content "$root/package.json" -Raw | ConvertFrom-Json).version
$versions['tauri.conf.json'] = (Get-Content "$root/src-tauri/tauri.conf.json" -Raw | ConvertFrom-Json).version

$cargoToml = Get-Content "$root/src-tauri/Cargo.toml"
# first `version = "..."` after `[package]`
$pkgIdx = ($cargoToml | Select-String -SimpleMatch '[package]' | Select-Object -First 1).LineNumber
for ($i = $pkgIdx; $i -lt $cargoToml.Count; $i++) {
    if ($cargoToml[$i] -match '^\s*version\s*=\s*"([^"]+)"') { $versions['Cargo.toml'] = $Matches[1]; break }
    if ($cargoToml[$i] -match '^\s*\[') { break }
}

$cargoLock = Get-Content "$root/Cargo.lock"
$inApp = $false
foreach ($line in $cargoLock) {
    if ($line -match '^\[\[package\]\]') { $inApp = $false; continue }
    if ($line -match '^\s*name\s*=\s*"app"') { $inApp = $true; continue }
    if ($inApp -and $line -match '^\s*version\s*=\s*"([^"]+)"') { $versions['Cargo.lock'] = $Matches[1]; break }
}

# CHANGELOG: first header line "## [x.y.z]" whose line is not Unpublished
foreach ($line in (Get-Content "$root/CHANGELOG.md")) {
    if ($line -match '^##\s*\[(\d+\.\d+\.\d+)\]') {
        if ($line -match 'Unpublished') { continue }
        $versions['CHANGELOG.md'] = $Matches[1]
        break
    }
}

$fail = $false
$expected = $versions['tauri.conf.json']
foreach ($k in ($versions.Keys | Sort-Object)) {
    $ok = ($versions[$k] -eq $expected)
    Write-Host ("{0,-18} {1}  {2}" -f $k, $versions[$k], $(if ($ok) { 'OK' } else { "MISMATCH (expected $expected)" }))
    if (-not $ok) { $fail = $true }
}
if ($versions.Count -ne 5) {
    Write-Error "expected 5 version sources, parsed $($versions.Count) — one is missing/unparseable"
    $fail = $true
}
if ($fail) { exit 1 }
Write-Host "all 5 version sources agree: $expected"
