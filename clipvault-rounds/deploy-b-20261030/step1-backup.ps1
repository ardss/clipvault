$ErrorActionPreference = 'Stop'
$src = Join-Path $env:APPDATA 'com.clipvault.app'
$dst = 'K:\Project\1-Active-Projects\clipvault\clipvault-rounds\backup-20261030-b'
New-Item -ItemType Directory -Force -Path $dst | Out-Null

Write-Output "=== SOURCE FILE INVENTORY (size + mtime) ==="
Get-ChildItem $src -Recurse -File | ForEach-Object {
  Write-Output ("SRC {0} SIZE={1} MTIME={2}" -f $_.FullName.Substring($src.Length).TrimStart('\'), $_.Length, $_.LastWriteTime.ToString('o'))
}

robocopy $src $dst /E /COPY:DAT /R:2 /W:2 /NFL /NDL /NP | Out-Null
$rc = $LASTEXITCODE
Write-Output "ROBOCOPY_EXIT=$rc"
if ($rc -ge 8) { exit $rc }

$srcFiles = @(Get-ChildItem $src -Recurse -File)
$dstFiles = @(Get-ChildItem $dst -Recurse -File)
Write-Output ("SRC_FILES={0} DST_FILES={1}" -f $srcFiles.Count, $dstFiles.Count)
if ($srcFiles.Count -ne $dstFiles.Count) { Write-Output "COUNT_MISMATCH"; exit 3 }
foreach ($f in $srcFiles) {
  $rel = $f.FullName.Substring($src.Length).TrimStart('\')
  $g = Get-Item (Join-Path $dst $rel) -ErrorAction SilentlyContinue
  if (-not $g) { Write-Output "MISSING_IN_BACKUP $rel"; exit 4 }
  if ($g.Length -ne $f.Length) { Write-Output "SIZE_MISMATCH $rel src=$($f.Length) dst=$($g.Length)"; exit 5 }
}
Write-Output "COPY_VERIFIED_OK"

foreach ($n in @('clips.db','clips.db-wal','clips.db-shm')) {
  $s = Get-Item (Join-Path $src $n) -ErrorAction SilentlyContinue
  $d = Get-Item (Join-Path $dst $n) -ErrorAction SilentlyContinue
  if ($s) { Write-Output ("SHA256 SRC {0} = {1}" -f $n, (Get-FileHash $s.FullName -Algorithm SHA256).Hash) }
  if ($d) { Write-Output ("SHA256 DST {0} = {1}" -f $n, (Get-FileHash $d.FullName -Algorithm SHA256).Hash) }
}
Write-Output "BACKUP_OK"
