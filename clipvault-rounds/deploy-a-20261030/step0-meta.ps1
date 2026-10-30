$ErrorActionPreference = 'Continue'
Write-Output "=== HOST ==="
Write-Output ("HOSTNAME={0} USER={1} NOW={2}" -f $env:COMPUTERNAME, $env:USERNAME, (Get-Date).ToString('o'))

Write-Output "=== OLD EXE ==="
$exe = Get-Item 'C:\Users\13397\AppData\Local\ClipVault\app.exe' -ErrorAction SilentlyContinue
if ($exe) {
  Write-Output ("OLD_EXE={0}" -f $exe.FullName)
  Write-Output ("OLD_VERSION={0}" -f $exe.VersionInfo.ProductVersion)
  Write-Output ("OLD_EXE_MTIME={0}" -f $exe.LastWriteTime.ToString('o'))
} else { Write-Output "OLD_EXE_MISSING" }

Write-Output "=== RUNNING PROCS (ClipVault paths) ==="
Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -like '*ClipVault*' } | ForEach-Object { Write-Output ("PROC PID={0} PATH={1} CREATED={2}" -f $_.ProcessId, $_.ExecutablePath, $_.CreationDate) }

Write-Output "=== RUN KEY (pre-install) ==="
$key = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
if (Test-Path $key) {
  $v = (Get-ItemProperty $key).ClipVault
  if ($v) { Write-Output ("RUN_CLIPVAULT={0}" -f $v) } else { Write-Output "RUN_CLIPVAULT_MISSING" }
} else { Write-Output "RUN_KEY_MISSING" }

Write-Output "=== DB META (pre-install) ==="
$db = Join-Path $env:APPDATA 'com.clipvault.app\clips.db'
if (Test-Path $db) { $i = Get-Item $db; Write-Output ("DB_SIZE={0} DB_MTIME={1}" -f $i.Length, $i.LastWriteTime.ToString('o')) } else { Write-Output "DB_MISSING" }
$wal = Join-Path $env:APPDATA 'com.clipvault.app\clips.db-wal'
if (Test-Path $wal) { $w = Get-Item $wal; Write-Output ("WAL_SIZE={0} WAL_MTIME={1}" -f $w.Length, $w.LastWriteTime.ToString('o')) }

Write-Output "=== UNINSTALL REG ==="
$u = Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\ClipVault' -ErrorAction SilentlyContinue
if ($u) { ($u | Select-Object DisplayName,DisplayVersion,InstallLocation,UninstallString | Format-List | Out-String -Width 400 | Write-Output) } else { Write-Output "UNINSTALL_REG_MISSING" }

Write-Output "=== TOOLS ==="
foreach ($t in @('python','py','sqlite3','curl.exe','robocopy')) {
  $c = Get-Command $t -ErrorAction SilentlyContinue
  if ($c) { Write-Output ("TOOL {0} = {1}" -f $t, $c.Source) } else { Write-Output ("TOOL {0} = MISSING" -f $t) }
}
Write-Output "META_DONE"
