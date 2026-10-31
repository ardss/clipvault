# run-native-probes.ps1 — isolation executor for the L3 native probes
# (TESTING_ARCHITECTURE_PLAN §3.3, phase 2).
#
# WHAT IT RUNS
#   cargo test --test <target> -- --ignored
#   Allowed targets (by --test target name, NEVER --exact/--skip on test
#   names — cargo matches full test names, target-name filters match 0 tests
#   and silently pass; plan §3.3):
#     - hook_latency_probe        (real WH_MOUSE_LL hook + synthetic mouse input)
#     - clipboard_listener_probe  (real system clipboard writes)
#   Run ONE target per invocation. The probes refuse to start unless
#   CV_E2E_ISOLATED=1 (checked in-process at probe start AND enforced here —
#   belt and braces; removing either gate is a gate-integrity defect).
#
# ISOLATION FLOW — the four steps below MUST be performed in order around the
# probe run, inside a pre-approved execution window (stopping the production
# instance is a destructive operation: user explicit authorization is required
# BEFORE the window, and the window + outcome must be recorded in
# MAINTENANCE_LOG; plan §1.3-3, §3.4):
#
#   STEP 1 — STOP the production instance at the EXPLICIT path:
#            F:\Apps\ClipVault\app.exe   (DEPLOYMENT.md B-machine install
#            path; never taskkill //IM app.exe — the debug probe build also
#            has app.exe as its image name). Never run these probes via SSH.
#   STEP 2 — PRESERVE the user's clipboard (TESTING.md §6: save the current
#            clipboard content before any test write; never clear history).
#   STEP 3 — RUN the probe:
#            powershell -File scripts\run-native-probes.ps1 -Test hook_latency_probe
#   STEP 4 — RESTORE the clipboard content saved in STEP 2, then RESTART the
#            production instance from F:\Apps\ClipVault\app.exe AND VERIFY:
#            retry start up to 3 times, confirm the process is running
#            (Get-Process by path); on failure exit non-zero with an explicit
#            RED marker — "production instance not restored" MUST be written
#            to the checklist / MAINTENANCE_LOG, never closed silently.
#
# This script itself does NOT perform steps 1/2/4 — they touch the user's
# real desktop and production instance and are executed by the operator (or a
# dedicated gated wrapper) inside the authorized window. Its hard
# responsibilities: force the env gate, validate the target, run the probe.

param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("hook_latency_probe", "clipboard_listener_probe")]
    [string]$Test
)

$ErrorActionPreference = "Stop"

# ── gate 1: environment flag ────────────────────────────────────────────────
if ($env:CV_E2E_ISOLATED -ne "1") {
    Write-Error (@"
REFUSED: CV_E2E_ISOLATED=1 is required. These probes write the real system
clipboard and/or send synthetic input, and the production instance must be
stopped first (single-instance plugin would otherwise take over the probe).
See TESTING_ARCHITECTURE_PLAN §1.3-3 and TESTING.md.
"@)
    exit 1
}

# ── gate 2: run from the repo, probes present ───────────────────────────────
$repo = Split-Path -Parent $PSScriptRoot
$srcTauri = Join-Path $repo "src-tauri"
if (-not (Test-Path (Join-Path $srcTauri "tests\$Test.rs"))) {
    Write-Error "REFUSED: probe source tests/$Test.rs not found under $srcTauri"
    exit 1
}

Write-Host "run-native-probes: target=$Test gate=CV_E2E_ISOLATED=1"
Write-Host "REMINDER: steps 1-2 (stop F:\Apps\ClipVault\app.exe, preserve clipboard)" -ForegroundColor Yellow
Write-Host "must already be done, and step 4 (restore clipboard, restart & VERIFY" -ForegroundColor Yellow
Write-Host "the production instance) must follow this run. Record in MAINTENANCE_LOG." -ForegroundColor Yellow

# ── run the probe by --test target (never --exact on test names) ────────────
Push-Location $srcTauri
try {
    # the probe itself re-checks CV_E2E_ISOLATED at startup and exits non-zero
    # if absent — the gate lives in both layers on purpose
    & cargo test --test $Test -- --ignored --nocapture
    $code = $LASTEXITCODE
} finally {
    Pop-Location
}

if ($code -ne 0) {
    Write-Host "RED: probe $Test FAILED (exit $code)" -ForegroundColor Red
} else {
    Write-Host "probe $Test passed" -ForegroundColor Green
}
Write-Host "NOW EXECUTE STEP 4: restore clipboard, restart F:\Apps\ClipVault\app.exe, VERIFY process is running." -ForegroundColor Yellow
exit $code
