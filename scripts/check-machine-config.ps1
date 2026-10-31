# check-machine-config.ps1 — multi-machine config drift detector
# (TESTING_ARCHITECTURE_PLAN §3.5, incident #7: machine A inherited a stale
# hotkey at install time). HONEST POSITIONING: this is a post-deployment
# DETECTION layer, not an install-time prevention layer — the drift happened
# during install/POSTINSTALL, before any check could run.
#
# Architecture (per plan: expected values are passed AS PARAMETERS from machine
# B's docs/machine-registry.json; machine A only reads its LOCAL
# %APPDATA%\com.clipvault.app\settings.json and returns the measured values as
# JSON; the comparison happens on B's side). A never reads the K$ share and
# the escaping Git-Bash -> ssh -> cmd is confined to one -EncodedCommand
# blob, which survives cmd's quoting rules untouched.
#
# Modes:
#   -Collect            run ON the target machine: read local settings.json,
#                       print measured JSON (this is what the SSH one-liner runs)
#   -PrintSshCommand    print the exact SSH one-liner to paste from B
#   -Compare <name> [-SshHost <alias>] [-ActualJson '<json>']
#                       run on B: compare registry expectations against
#                       measured values. With -SshHost it executes the
#                       one-liner over SSH (deployment time only); with
#                       -ActualJson it compares a previously collected blob.
#
# DEPLOYMENT SOP entry (added in DEPLOYMENT.md): read-only check, safe to run
# over SSH (single-line read-only mode only — NEVER run install/uninstall or
# synthetic-input operations this way). NOT wired into CI: the target machine
# is not on the runner.
param(
    [switch]$Collect,
    [switch]$PrintSshCommand,
    [string]$Compare,
    [string]$SshHost,
    [string]$ActualJson
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot

# Collector source: ONE line, no quotes that cmd could mangle, embedded in a
# base64 EncodedCommand so Git Bash -> ssh -> cmd -> powershell involves zero
# manual escaping.
$collector = '$p = Join-Path $env:APPDATA "com.clipvault.app\settings.json"; if (-not (Test-Path $p)) { Write-Output "MEASURE_ERROR settings.json missing" } else { $s = Get-Content $p -Raw | ConvertFrom-Json; $o = [ordered]@{ hotkey = $s.hotkey; autostart = $s.autostart; panel_height = $s.panel_height }; (ConvertTo-Json $o -Compress) }'
$encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($collector))

if ($Collect) {
    Invoke-Expression $collector
    exit 0
}

if ($PrintSshCommand) {
    if (-not $SshHost) { Write-Error '-PrintSshCommand requires -SshHost'; exit 2 }
    Write-Output "ssh $SshHost `"powershell -NoProfile -EncodedCommand $encoded`""
    exit 0
}

if (-not $Compare) {
    Write-Host 'usage: check-machine-config.ps1 -Collect | -PrintSshCommand -SshHost <alias> | -Compare <machine-name> [-SshHost <alias>] [-ActualJson <json>]'
    exit 2
}

# -Encoding UTF8 is required: Windows PowerShell 5.1 defaults to ANSI for
# BOM-less files and the registry file carries Chinese comments
$registry = Get-Content "$root/docs/machine-registry.json" -Raw -Encoding UTF8 | ConvertFrom-Json
if (-not $registry.machines.$Compare) { Write-Error "machine '$Compare' not in docs/machine-registry.json"; exit 2 }
$expect = $registry.machines.$Compare

if ($ActualJson) {
    $actual = $ActualJson | ConvertFrom-Json
} elseif ($SshHost) {
    $one = (& $PSCommandPath -PrintSshCommand -SshHost $SshHost)
    Write-Host "SSH (read-only): $one"
    $out = Invoke-Expression $one
    Write-Host "measured: $out"
    if ($out -match 'MEASURE_ERROR') { Write-Error "collection failed on target: $out"; exit 1 }
    $actual = $out | ConvertFrom-Json
} else {
    Write-Error 'either -SshHost or -ActualJson is required for -Compare'; exit 2
}

$fail = $false
foreach ($field in @('hotkey', 'autostart', 'panel_height')) {
    $e = $expect.$field; $a = $actual.$field
    # panel_height arrives as e.g. 676 vs 676.0 — compare numerically
    $ok = if ($field -eq 'panel_height') {
        [Math]::Abs([double]$e - [double]$a) -lt 0.01
    } else {
        ("$e" -eq "$a")
    }
    Write-Host ("{0,-14} expect={1,-8} actual={2,-8} {3}" -f $field, $e, $a, $(if ($ok) { 'OK' } else { 'DRIFT' }))
    if (-not $ok) { $fail = $true }
}
if ($fail) {
    Write-Host "FAIL: machine '$Compare' config drifted from docs/machine-registry.json — fix the machine OR update the registry if the user changed a preference (registry lag is a process defect, see plan §3.5). Incident #7 precedent: hotkey inherited Ctrl+Alt+V on install."
    exit 1
}
Write-Host "PASS: machine '$Compare' matches registry"
