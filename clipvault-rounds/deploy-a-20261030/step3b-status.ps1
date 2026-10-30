$ErrorActionPreference = 'Continue'
Write-Output "=== INSTALL STATUS CHECK ==="
$exe = Get-Item 'C:\Users\13397\AppData\Local\ClipVault\app.exe' -ErrorAction SilentlyContinue
if ($exe) {
  Write-Output ("EXE_VERSION={0} MTIME={1}" -f $exe.VersionInfo.ProductVersion, $exe.LastWriteTime.ToString('o'))
} else { Write-Output "EXE_MISSING" }

Write-Output "=== app.exe process by path ==="
$p = @(Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -eq 'C:\Users\13397\AppData\Local\ClipVault\app.exe' })
if ($p.Count -gt 0) { foreach ($x in $p) { Write-Output ("RUNNING PID={0} CREATED={1}" -f $x.ProcessId, $x.CreationDate) } } else { Write-Output "APP_NOT_RUNNING" }

Write-Output "=== installer processes still alive? ==="
Get-Process | Where-Object { $_.ProcessName -like '*ClipVault*' -or $_.ProcessName -eq 'app' } | ForEach-Object { Write-Output ("PROC NAME={0} PID={1} PATH={2}" -f $_.ProcessName, $_.Id, $_.Path) }
Write-Output "STATUS_DONE"
