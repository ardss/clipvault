$ErrorActionPreference = 'Continue'
$live = Join-Path $env:APPDATA 'com.clipvault.app'
$bk = Join-Path $env:APPDATA 'clipvault-backup-20261030-a'
Write-Output "=== POST-STOP DB SECOND COPY (consistent snapshot, SOP) ==="
foreach ($n in @('clips.db','clips.db-wal','clips.db-shm')) {
  $s = Join-Path $live $n
  if (Test-Path $s) {
    Copy-Item $s (Join-Path $bk $n) -Force
    $a = Get-FileHash $s -Algorithm SHA256
    $b = Get-FileHash (Join-Path $bk $n) -Algorithm SHA256
    Write-Output ("RECOPIED {0} SIZE={1} SRC_SHA={2} DST_SHA={3} MATCH={4}" -f $n, (Get-Item $s).Length, $a.Hash, $b.Hash, ($a.Hash -eq $b.Hash))
  } else { Write-Output ("NOT_PRESENT {0}" -f $n) }
}
Write-Output "=== OLD INSTALL DIR BACKUP (rollback artifact) ==="
$oldInstall = 'C:\Users\13397\AppData\Local\ClipVault'
$exeDir = Join-Path $bk '_old_installdir'
if (Test-Path (Join-Path $oldInstall 'app.exe')) {
  robocopy $oldInstall $exeDir /E /COPY:DAT /R:2 /W:2 /NFL /NDL /NP | Out-Null
  Write-Output ("OLD_INSTALLDIR_ROBOCOPY rc={0} -> {1}" -f $LASTEXITCODE, $exeDir)
  $srcN = @(Get-ChildItem $oldInstall -Recurse -File).Count
  $dstN = @(Get-ChildItem $exeDir -Recurse -File).Count
  Write-Output ("OLD_INSTALLDIR SRC_FILES={0} DST_FILES={1}" -f $srcN, $dstN)
  if ($srcN -eq $dstN) { Write-Output "OLD_INSTALLDIR_BACKUP_OK" } else { Write-Output "OLD_INSTALLDIR_COUNT_MISMATCH" }
} else { Write-Output "OLD_INSTALLDIR_MISSING" }
Write-Output "POSTSTOP_AND_ROLLBACK_ARTIFACTS_DONE"
