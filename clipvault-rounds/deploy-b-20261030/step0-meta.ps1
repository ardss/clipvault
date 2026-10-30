$ErrorActionPreference = 'Continue'
Write-Output "=== OLD EXE ==="
$exe = Get-Item 'F:\Apps\ClipVault\app.exe' -ErrorAction SilentlyContinue
if ($exe) {
  Write-Output ("OLD_EXE={0}" -f $exe.FullName)
  Write-Output ("OLD_VERSION={0}" -f $exe.VersionInfo.ProductVersion)
  Write-Output ("OLD_EXE_MTIME={0}" -f $exe.LastWriteTime.ToString('o'))
} else { Write-Output "OLD_EXE_MISSING" }

Write-Output "=== RUN KEY (pre-install) ==="
$key = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
if (Test-Path $key) { (Get-ItemProperty $key | Select-Object * -ExcludeProperty PS*) | Format-List | Out-String -Width 300 | Write-Output }
else { Write-Output "RUN_KEY_MISSING" }

Write-Output "=== DB META (pre-install) ==="
$db = Join-Path $env:APPDATA 'com.clipvault.app\clips.db'
if (Test-Path $db) { $i = Get-Item $db; Write-Output ("DB_SIZE={0} DB_MTIME={1}" -f $i.Length, $i.LastWriteTime.ToString('o')) } else { Write-Output "DB_MISSING" }
$wal = Join-Path $env:APPDATA 'com.clipvault.app\clips.db-wal'
if (Test-Path $wal) { $w = Get-Item $wal; Write-Output ("WAL_SIZE={0} WAL_MTIME={1}" -f $w.Length, $w.LastWriteTime.ToString('o')) }

Write-Output "=== UNINSTALL REG (install location evidence) ==="
$u = Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\ClipVault' -ErrorAction SilentlyContinue
if ($u) { ($u | Select-Object DisplayName,DisplayVersion,InstallLocation,UninstallString | Format-List | Out-String -Width 400 | Write-Output) } else { Write-Output "UNINSTALL_REG_MISSING" }
