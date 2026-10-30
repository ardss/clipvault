$ErrorActionPreference = 'Continue'
Write-Output "=== CHECK1: process by path ==="
$procs = @(Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -eq 'C:\Users\13397\AppData\Local\ClipVault\app.exe' })
if ($procs.Count -gt 0) {
  foreach ($p in $procs) { Write-Output ("RUNNING PID={0} PATH={1} CREATED={2}" -f $p.ProcessId, $p.ExecutablePath, $p.CreationDate) }
} else { Write-Output "NOT_RUNNING" }

Write-Output "=== CHECK1b: installed exe version ==="
$exe = Get-Item 'C:\Users\13397\AppData\Local\ClipVault\app.exe'
Write-Output ("EXE_VERSION={0} EXE_MTIME={1}" -f $exe.VersionInfo.ProductVersion, $exe.LastWriteTime.ToString('o'))

Write-Output "=== CHECK2: HKCU Run key (autostart) ==="
$rp = (Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -ErrorAction SilentlyContinue).ClipVault
if ($rp) { Write-Output ("RUN_CLIPVAULT={0}" -f $rp) } else { Write-Output "RUN_CLIPVAULT_MISSING" }

Write-Output "=== CHECK2b: uninstall reg ==="
$u = Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\ClipVault' -ErrorAction SilentlyContinue
if ($u) { Write-Output ("UNINSTALL DisplayVersion={0} InstallLocation={1}" -f $u.DisplayVersion, $u.InstallLocation) } else { Write-Output "UNINSTALL_REG_MISSING" }

Write-Output "=== CHECK3: cv.log tail (30) ==="
$log = Join-Path $env:APPDATA 'com.clipvault.app\cv.log'
if (Test-Path $log) { Get-Content $log -Tail 30 } else { Write-Output "CVLOG_MISSING" }

Write-Output "=== CHECK3b: hotkey grep (last 6) ==="
if (Test-Path $log) {
  $hits = Select-String -Path $log -Pattern 'hotkey' | Select-Object -Last 6
  if ($hits) { $hits | ForEach-Object { Write-Output ("LINE {0}: {1}" -f $_.LineNumber, $_.Line) } } else { Write-Output "NO_HOTKEY_LINES" }
}
Write-Output "ACCEPT_STEP_DONE"
