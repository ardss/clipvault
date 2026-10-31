# smoke-install.ps1 — installer smoke: silent install -> launch -> process
# confirmation -> uninstall. Scripts the physical-evidence items of
# RELEASE_CHECKLIST §4 (LastWriteTime / StartTime / process confirmed);
# §8 sensory items stay manual by design.
#
# *** EXECUTION ENVIRONMENT PENDING PoC VERDICT (阶段1, plan §3.5) ***
# RELEASE_CHECKLIST §2 states the only legitimate source of candidate
# installers is GitHub Release CI, and machine B is forbidden from running
# local builds. The only candidate execution environment is therefore a GH
# windows runner (manual dispatch variant of release.yml, NOT the tag flow),
# but whether a runner can host a tray-icon + single-instance + POSTINSTALL
# GUI app is UNVERIFIED. This script is delivered now; it is NOT wired into
# CI until the PoC verdict lands. If the PoC fails: incident #4's machine
# gate is honestly downgraded to "no execution environment" and post-release
# A/B read-only checks carry the load (plan §3.5 / §4 stage 1).
#
# A-MACHINE FORBIDDEN: this script is destructive (uninstall + reinstall +
# launch). Machine A (production) only ever gets read-only checks over SSH
# (process confirmation, check-machine-config.ps1, cv.log grep).
# ROLLBACK PREREQUISITE: before installing any draft build in a real
# environment, back up %APPDATA%\com.clipvault.app\ and keep the current
# production installer at hand for re-install.
param(
    [Parameter(Mandatory = $true)][string]$InstallerPath,
    [string]$ProcessName = 'app',
    [int]$LaunchWaitSeconds = 20
)
$ErrorActionPreference = 'Stop'

if (-not (Test-Path $InstallerPath)) { Write-Error "installer not found: $InstallerPath"; exit 1 }

# 1. kill + confirm old process gone
$old = Get-Process -Name $ProcessName -ErrorAction SilentlyContinue
if ($old) {
    $old | Stop-Process -Force
    Start-Sleep -Seconds 2
    if (Get-Process -Name $ProcessName -ErrorAction SilentlyContinue) {
        Write-Error 'old process did not exit'; exit 1
    }
    Write-Host 'old process killed and confirmed gone'
} else {
    Write-Host 'no pre-existing process'
}

# 2. silent install
$exe = (Get-Item $InstallerPath).FullName
Write-Host "silent install: $exe /S"
$proc = Start-Process -FilePath $exe -ArgumentList '/S' -PassThru -Wait
if ($proc.ExitCode -ne 0) { Write-Error "installer exit code $($proc.ExitCode)"; exit 1 }

# locate installed exe (NSIS currentUser default: %LOCALAPPDATA%\ClipVault\app.exe)
$installed = Join-Path $env:LOCALAPPDATA 'ClipVault\app.exe'
if (-not (Test-Path $installed)) { Write-Error "installed exe not found at $installed"; exit 1 }
Write-Host ("installed exe LastWriteTime: {0}" -f (Get-Item $installed).LastWriteTime)

# 3. launch and confirm StartTime is fresh
$started = Start-Process -FilePath $installed -PassThru
Start-Sleep -Seconds $LaunchWaitSeconds
$running = Get-Process -Name $ProcessName -ErrorAction SilentlyContinue
if (-not $running) { Write-Error "process $ProcessName not running after launch — install smoke FAILED"; exit 1 }
Write-Host ("process StartTime: {0}" -f ($running | Select-Object -First 1).StartTime)
Write-Host 'PASS: silent install -> launch -> process confirmation'

# 4. uninstall (still NSIS /S with uninstaller)
# NOTE: PoC environment must decide uninstaller path handling; on the runner
# this step confirms a clean cycle. If uninstall leaves the process running,
# kill it first (it was started by this script, not by the user).
Get-Process -Name $ProcessName -ErrorAction SilentlyContinue | Stop-Process -Force
$uninst = Get-ChildItem (Join-Path $env:LOCALAPPDATA 'ClipVault') -Filter 'Uninstall *.exe' -ErrorAction SilentlyContinue | Select-Object -First 1
if ($uninst) {
    Start-Process -FilePath $uninst.FullName -ArgumentList '/S' -Wait
    Write-Host 'uninstall issued'
} else {
    Write-Host 'uninstaller not found — PoC should record this as a finding'
}
