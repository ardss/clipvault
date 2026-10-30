$ErrorActionPreference = 'Continue'
$target = 'C:\Users\13397\AppData\Local\ClipVault\app.exe'
Write-Output "=== BEFORE: instances with SessionId ==="
Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -eq $target } | ForEach-Object {
  $pr = Get-Process -Id $_.ProcessId -ErrorAction SilentlyContinue
  Write-Output ("INST PID={0} SESSION={1} CREATED={2}" -f $_.ProcessId, $pr.SessionId, $_.CreationDate)
}

Write-Output "=== STOP ALL by path ==="
Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -eq $target } | ForEach-Object {
  Write-Output ("STOP PID={0}" -f $_.ProcessId)
  Stop-Process -Id $_.ProcessId -Force
}
Start-Sleep -Seconds 3
$left = @(Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -eq $target })
Write-Output ("REMAINING_AFTER_STOP={0}" -f $left.Count)

Write-Output "=== RELAUNCH single instance via WMI (escapes ssh session) ==="
$cmd = '"{0}" /autostart' -f $target
$r = Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{ CommandLine = $cmd }
Write-Output ("WMI_CREATE ReturnValue={0} PID={1}" -f $r.ReturnValue, $r.ProcessId)
Start-Sleep -Seconds 8

Write-Output "=== AFTER: instances with SessionId ==="
Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -eq $target } | ForEach-Object {
  $pr = Get-Process -Id $_.ProcessId -ErrorAction SilentlyContinue
  Write-Output ("INST PID={0} SESSION={1} CREATED={2}" -f $_.ProcessId, $pr.SessionId, $_.CreationDate)
}
Write-Output "=== cv.log tail (15) ==="
$log = Join-Path $env:APPDATA 'com.clipvault.app\cv.log'
Get-Content $log -Tail 15
Write-Output "NORMALIZE_DONE"
