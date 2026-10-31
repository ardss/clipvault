# check-dist-freshness.ps1 — scripted RELEASE_CHECKLIST §3 (timestamp part only).
# Asserts dist/ is OLDER than the bundle installer: packaging must consume the
# latest frontend build. The "key fix string grep" stays on the manual
# checklist by design (per plan §3.5 — scripting it would add a per-version
# manual input point, not remove one).
# Wired into release.yml after `npx tauri build`.
param(
    [string]$Dist = (Join-Path (Split-Path -Parent $PSScriptRoot) 'dist'),
    [string]$BundleDir = 'src-tauri/target/release/bundle/nsis'
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
if (-not (Test-Path $Dist)) { Write-Error "dist/ not found at $Dist — run the frontend build first"; exit 1 }

$distNewest = (Get-ChildItem $Dist -Recurse -File | Sort-Object LastWriteTime -Descending | Select-Object -First 1)
if (-not $distNewest) { Write-Error 'dist/ contains no files'; exit 1 }

$bundlePath = if (Test-Path (Join-Path $root $BundleDir)) { Join-Path $root $BundleDir } else { $BundleDir }
$installer = Get-ChildItem $bundlePath -Filter '*-setup.exe' | Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not $installer) { Write-Error "no *-setup.exe found under $bundlePath — run npx tauri build first"; exit 1 }

Write-Host ("dist newest file:    {0}  ({1})" -f $distNewest.FullName, $distNewest.LastWriteTime)
Write-Host ("installer:           {0}  ({1})" -f $installer.FullName, $installer.LastWriteTime)

if ($installer.LastWriteTime -lt $distNewest.LastWriteTime) {
    Write-Host 'FAIL: installer is OLDER than the newest dist file — the bundle was built from stale frontend output (RELEASE_CHECKLIST §3)'
    exit 1
}
Write-Host 'PASS: installer is newer than all dist files'
