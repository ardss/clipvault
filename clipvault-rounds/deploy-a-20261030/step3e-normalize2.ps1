$ErrorActionPreference = 'Continue'
$target = 'C:\Users\13397\AppData\Local\ClipVault\app.exe'
Write-Output "=== Stop only the session-0 leftover instance (by PID+path verify) ==="
Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -eq $target } | ForEach-Object {
  $pr = Get-Process -Id $_.ProcessId -ErrorAction SilentlyContinue
  if ($pr.SessionId -eq 0) {
    Write-Output ("STOP_SESSION0 PID={0}" -f $_.ProcessId)
    Stop-Process -Id $_.ProcessId -Force
  } else {
    Write-Output ("KEEP_SESSION1 PID={0} SESSION={1}" -f $_.ProcessId, $pr.SessionId)
  }
}
Write-Output "=== wait 75s to observe respawn behavior ==="
Start-Sleep -Seconds 75
Write-Output "=== FINAL instances ==="
Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -eq $target } | ForEach-Object {
  $pr = Get-Process -Id $_.ProcessId -ErrorAction SilentlyContinue
  Write-Output ("INST PID={0} SESSION={1} CREATED={2}" -f $_.ProcessId, $pr.SessionId, $_.CreationDate)
}
Write-Output "=== cv.log tail (10) ==="
$log = Join-Path $env:APPDATA 'com.clipvault.app\cv.log'
Get-Content $log -Tail 10
Write-Output "NORMALIZE2_DONE"
