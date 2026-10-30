$ErrorActionPreference = 'Continue'
$target = 'C:\Users\13397\AppData\Local\ClipVault\app.exe'
Write-Output "=== CURRENT instances + session ==="
Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -eq $target } | ForEach-Object {
  $pr = Get-Process -Id $_.ProcessId -ErrorAction SilentlyContinue
  Write-Output ("INST PID={0} SESSION={1} CREATED={2}" -f $_.ProcessId, $pr.SessionId, $_.CreationDate)
}
Write-Output "=== interactive session (explorer) reference ==="
Get-Process explorer -ErrorAction SilentlyContinue | ForEach-Object { Write-Output ("EXPLORER PID={0} SESSION={1}" -f $_.Id, $_.SessionId) }
Write-Output "=== cv.log hotkey lines (last 8) ==="
$log = Join-Path $env:APPDATA 'com.clipvault.app\cv.log'
$hits = Select-String -Path $log -Pattern 'hotkey' | Select-Object -Last 8
if ($hits) { $hits | ForEach-Object { Write-Output ("LINE {0}: {1}" -f $_.LineNumber, $_.Line) } } else { Write-Output "NO_HOTKEY_LINES" }
Write-Output "=== cv.log tail (12) ==="
Get-Content $log -Tail 12
Write-Output "HOTKEY_CHECK_DONE"
