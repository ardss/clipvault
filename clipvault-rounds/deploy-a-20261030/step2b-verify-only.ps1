$ErrorActionPreference = 'Stop'
$wd = 'C:\Users\13397\clipvault-deploy-20261030a'
Write-Output "=== SIZES ==="
foreach ($f in @('ClipVault_0.1.6_x64-setup.exe','ClipVault_0.1.6-portable.exe','SHA256SUMS.txt')) {
  $i = Get-Item (Join-Path $wd $f)
  Write-Output ("FILE {0} SIZE={1}" -f $i.Name, $i.Length)
}
Write-Output "=== SHA256SUMS.txt content (raw) ==="
Get-Content (Join-Path $wd 'SHA256SUMS.txt') | ForEach-Object { Write-Output ("RAWLINE [{0}]" -f $_) }
Write-Output "=== VERIFY vs known-good (independently verified on B + main session 2026-10-30) ==="
$expect = @{
  'ClipVault_0.1.6_x64-setup.exe' = '79FCC5C783B8233493C574C503F7C5941AD863F295A93F5BCAF8F465F142F751'
  'ClipVault_0.1.6-portable.exe'  = '815FEF1E4D163FA971452EC849715BE86DB042421C5D423E4C9B1644B0875B1E'
}
$allOk = $true
foreach ($k in $expect.Keys) {
  $h = (Get-FileHash (Join-Path $wd $k) -Algorithm SHA256).Hash
  $ok = ($h -eq $expect[$k])
  Write-Output ("HASH {0} = {1} MATCH={2}" -f $k, $h, $ok)
  if (-not $ok) { $allOk = $false }
}
$sumsSha = (Get-FileHash (Join-Path $wd 'SHA256SUMS.txt') -Algorithm SHA256).Hash
Write-Output ("SUMS_FILE_SHA256={0}" -f $sumsSha)
Write-Output ("SUMS_FILE_EXPECTED=923291C098DCE72ECA3DEC4881E57F73A949811ACE3CE6820F3CB59469A14752")
Write-Output "=== VERIFY vs downloaded SHA256SUMS.txt entries (CRLF-normalized) ==="
$lines = Get-Content (Join-Path $wd 'SHA256SUMS.txt')
foreach ($line in $lines) {
  $t = ($line -replace "`r",'').Trim()
  if ($t -eq '') { continue }
  $parts = $t -split '\s+',2
  if ($parts.Count -eq 2) {
    $want = $parts[0].ToUpper()
    $name = $parts[1].Trim()
    $p = Join-Path $wd $name
    if (Test-Path $p) {
      $h = (Get-FileHash $p -Algorithm SHA256).Hash
      $ok = ($want -eq $h)
      Write-Output ("SUMSLINE {0} want={1} got={2} MATCH={3}" -f $name, $want, $h, $ok)
      if (-not $ok) { $allOk = $false }
    } else {
      Write-Output ("SUMSLINE {0} FILE_NOT_DOWNLOADED" -f $name)
    }
  }
}
if (-not $allOk) { Write-Output "SHA256_MISMATCH"; exit 20 }
Write-Output "SHA256_ALL_MATCH"
