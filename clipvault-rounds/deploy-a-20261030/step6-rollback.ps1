$ErrorActionPreference = 'Continue'
$live = Join-Path $env:APPDATA 'com.clipvault.app'
$bk = Join-Path $env:APPDATA 'clipvault-backup-20261030-a'
$oldInstallBk = Join-Path $bk '_old_installdir'
$target = 'C:\Users\13397\AppData\Local\ClipVault\app.exe'
Write-Output "=== ROLLBACK START (failure path) ==="

Write-Output "--- R1: stop running app by path ---"
Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -eq $target } | ForEach-Object {
  Write-Output ("STOP PID={0} PATH={1}" -f $_.ProcessId, $_.ExecutablePath)
  Stop-Process -Id $_.ProcessId -Force
}
Start-Sleep -Seconds 3

Write-Output "--- R2: restore data dir from backup (exclude rollback artifacts) ---"
if (Test-Path $bk) {
  robocopy $bk $live /E /COPY:DAT /PURGE /R:2 /W:2 /NFL /NDL /NP /XD _old_installdir | Out-Null
  Write-Output ("RESTORE_DATA rc={0}" -f $LASTEXITCODE)
} else { Write-Output "BACKUP_DIR_MISSING - CANNOT restore data" }

Write-Output "--- R3: restore old install dir (old exe) ---"
if (Test-Path (Join-Path $oldInstallBk 'app.exe')) {
  robocopy $oldInstallBk 'C:\Users\13397\AppData\Local\ClipVault' /E /COPY:DAT /R:2 /W:2 /NFL /NDL /NP | Out-Null
  Write-Output ("RESTORE_INSTALLDIR rc={0}" -f $LASTEXITCODE)
} else { Write-Output "OLD_INSTALLDIR_BACKUP_MISSING" }

Write-Output "--- R4: relaunch old exe ---"
if (Test-Path $target) {
  Start-Process -FilePath $target
  Start-Sleep -Seconds 6
  $p = @(Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -eq $target })
  if ($p.Count -gt 0) { Write-Output ("RELAUNCHED PID={0}" -f $p[0].ProcessId) } else { Write-Output "RELAUNCH_NOT_RUNNING" }
  $log = Join-Path $env:APPDATA 'com.clipvault.app\cv.log'
  if (Test-Path $log) { Get-Content $log -Tail 8 }
} else { Write-Output "OLD_EXE_MISSING_AFTER_RESTORE" }
Write-Output "ROLLBACK_DONE"
