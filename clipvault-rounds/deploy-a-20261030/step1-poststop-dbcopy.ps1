$ErrorActionPreference = 'Stop'
$src = Join-Path $env:APPDATA 'com.clipvault.app'
$dst = Join-Path $env:APPDATA 'clipvault-backup-20261030-a'
Write-Output "=== POST-STOP DB SECOND COPY (consistent snapshot, SOP) ==="
foreach ($n in @('clips.db','clips.db-wal','clips.db-shm')) {
  $s = Join-Path $src $n
  if (Test-Path $s) {
    Copy-Item $s (Join-Path $dst $n) -Force
    $a = Get-FileHash $s -Algorithm SHA256
    $b = Get-FileHash (Join-Path $dst $n) -Algorithm SHA256
    Write-Output ("RECOPIED {0} SIZE={1} SRC_SHA={2} DST_SHA={3} MATCH={4}" -f $n, (Get-Item $s).Length, $a.Hash, $b.Hash, ($a.Hash -eq $b.Hash))
  } else { Write-Output ("NOT_PRESENT {0}" -f $n) }
}
Write-Output "POSTSTOP_COPY_DONE"
