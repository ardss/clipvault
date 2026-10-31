# paste-e2e-gated.ps1 — in-place isolation wrapper for the paste-chain e2e
# (TESTING_ARCHITECTURE_PLAN §3.4, phase 3).
#
# WHAT IT WRAPS
#   scripts/e2e-paste.sh — the full paste landing chain (injector -> real
#   landing point, incident #3/#9). It operates the DEBUG probe build at
#   src-tauri/target/debug/app.exe and sends synthetic keyboard input on the
#   interactive desktop.
#
# THE GATE (mandatory, same door as the L3 probes)
#   CV_E2E_ISOLATED=1 — missing or not "1": the script REFUSES to run.
#   TESTING.md:31 (no synthetic input on the user's real desktop) is NOT
#   relaxed; the in-place isolation window is the only compliant path.
#
# AUTHORIZATION RED LINE
#   Stopping the production instance is destructive. The execution window
#   (unattended period) requires EXPLICIT user approval BEFORE running this
#   script, and the window + outcome must be recorded in MAINTENANCE_LOG.
#   This script does not pick its own window.
#
# FOUR-STEP FLOW (plan §3.4; path discipline: the PRODUCTION instance is
# F:\Apps\ClipVault\app.exe per DEPLOYMENT.md B-section, the DEBUG probe
# instance is src-tauri\target\debug\app.exe — never conflate the two, and
# never leave the debug instance in place of production):
#   1. STOP the production instance (explicit path F:\Apps\ClipVault\app.exe;
#      verify by Get-Process -Path, never taskkill //IM app.exe).
#   2. PRESERVE the user's clipboard (TESTING.md §6: retain before test
#      writes, restore afterwards, never clear history).
#      KNOWN LIMITATION (plan §3.3 "留存剪贴板" degraded implementation):
#      preservation covers CF_UNICODETEXT only. Image / file-list / custom
#      formats are NOT captured (Win32 clipboard format cloning from
#      PowerShell would require a format-enumeration + native-handle copy
#      helper). If the user's clipboard held a non-text format at window
#      start, the post-run restore is PARTIAL — the RED banner below says so
#      and the operator must disclose the partial restore in the
#      MAINTENANCE_LOG entry. This is a declared limitation, not a closed
#      loop, until the format-aware helper exists.
#   3. RUN the paste e2e (bash scripts/e2e-paste.sh) + collect evidence.
#   4. RESTORE the clipboard content saved in step 2, then RESTART the
#      production instance from F:\Apps\ClipVault\app.exe AND VERIFY: retry
#      start up to 3 times; if the process is still not confirmed running,
#      exit non-zero with an explicit RED marker — "production instance not
#      restored" must be written back to the checklist / MAINTENANCE_LOG,
#      never silently closed (autostart=true does not self-heal this session).
#
# USAGE (inside an authorized window):
#   $env:CV_E2E_ISOLATED = "1"
#   powershell -ExecutionPolicy Bypass -File scripts\paste-e2e-gated.ps1
#
# SCOPE NOTE: this script drives the real desktop and the real clipboard in
# a destructive-adjacent way. It was WRITTEN but NOT RUN in the phase-3
# workflow session (the isolation window itself requires the user's explicit
# authorization and is outside that session's boundaries).

param(
    [string]$ProdExe = "F:\Apps\ClipVault\app.exe",
    [string]$E2eScript = "scripts\e2e-paste.sh"
)

$ErrorActionPreference = "Stop"

# ── gate: isolation flag ────────────────────────────────────────────────────
if ($env:CV_E2E_ISOLATED -ne "1") {
    Write-Error (@"
REFUSED: CV_E2E_ISOLATED=1 is required. The paste e2e sends synthetic input
on the interactive desktop, writes the real clipboard and stops the
production instance (TESTING.md:31; TESTING_ARCHITECTURE_PLAN 1.3-3 / 3.4).
Set the flag only inside a pre-approved, unattended execution window.
"@)
    exit 1
}

# ── context: run from repo root ─────────────────────────────────────────────
$repoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $repoRoot
if (-not (Test-Path $ProdExe)) {
    Write-Error "REFUSED: production instance not found at $ProdExe - refusing to proceed without the explicit path."
    exit 1
}

Write-Host "[gated] STEP 1: stopping production instance at $ProdExe"
$prodProc = Get-Process | Where-Object { $_.Path -eq $ProdExe }
if ($prodProc) {
    $prodProc | Stop-Process -Force
    Start-Sleep -Seconds 2
    if (Get-Process | Where-Object { $_.Path -eq $ProdExe }) {
        Write-Error "RED: production instance at $ProdExe did not stop - aborting before any input is sent."
        exit 1
    }
    Write-Host "[gated] production instance stopped."
} else {
    Write-Host "[gated] production instance not currently running (nothing to stop)."
}

# ── clipboard preservation (TESTING.md 6) ───────────────────────────────────
Write-Host "[gated] STEP 2: preserving clipboard content"
$clipSavedFile = Join-Path $env:TEMP ("cv-clip-backup-" + (Get-Date -Format "yyyyMMdd-HHmmss") + ".txt")
$clipHadText = $false
try {
    $clipText = Get-Clipboard -Raw -ErrorAction Stop
    if ($null -ne $clipText) {
        Set-Content -Path $clipSavedFile -Value $clipText -Encoding UTF8
        $clipHadText = $true
        Write-Host "[gated] clipboard text saved to $clipSavedFile"
    } else {
        Write-Host "[gated] clipboard empty (text) at start - noting absence"
    }
} catch {
    Write-Host "RED: clipboard preservation is TEXT-ONLY (known limitation) - non-text formats (images/files) present at window start were NOT saved and cannot be restored. Disclose the partial restore in MAINTENANCE_LOG."
}

# ── run the paste e2e ───────────────────────────────────────────────────────
Write-Host "[gated] STEP 3: running paste e2e ($E2eScript)"
$e2eExit = -1
try {
    bash $E2eScript
    $e2eExit = $LASTEXITCODE
} catch {
    Write-Host "[gated] e2e runner threw: $_"
}
Write-Host "[gated] e2e exit code: $e2eExit"

# ── restore + restart + VERIFY (the step that must never silently fail) ─────
Write-Host "[gated] STEP 4: restoring clipboard, restarting production instance, verifying"
if ($clipHadText) {
    try {
        Set-Clipboard -Value (Get-Content $clipSavedFile -Raw)
        Write-Host "[gated] clipboard content restored."
    } catch {
        Write-Host "RED: clipboard restore failed - manual restore from $clipSavedFile required."
    }
} else {
    Write-Host "[gated] no clipboard text was saved at start - nothing to restore."
}

$started = $false
for ($i = 1; $i -le 3; $i++) {
    try {
        Start-Process -FilePath $ProdExe
    } catch {
        Write-Host "[gated] start attempt $i threw: $_"
    }
    Start-Sleep -Seconds 4
    if (Get-Process | Where-Object { $_.Path -eq $ProdExe }) {
        $started = $true
        Write-Host "[gated] production instance verified running (attempt $i)."
        break
    }
    Write-Host "[gated] start attempt ${i}: process not confirmed yet."
}

if (-not $started) {
    Write-Host "RED: PRODUCTION INSTANCE NOT RESTORED - $ProdExe failed to start after 3 attempts."
    Write-Host "RED: 'production instance not restored' MUST be recorded in the release checklist and MAINTENANCE_LOG."
    exit 1
}

if ($e2eExit -ne 0) {
    Write-Host "RED: paste e2e FAILED (exit $e2eExit) - but the production instance IS restored."
    exit $e2eExit
}

Write-Host "PASS: paste e2e passed; clipboard restored; production instance restarted and verified."
exit 0
