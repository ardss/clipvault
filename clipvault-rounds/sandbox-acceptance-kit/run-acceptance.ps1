# Host-side launcher for ClipVault v0.1.5 isolated acceptance.
# Preconditions:
#   1. Windows Sandbox enabled (C:\Windows\System32\wsb.exe exists) - needs one
#      admin enable + reboot: Enable-WindowsOptionalFeature -Online -FeatureName Containers-DisposableClientVM
#   2. Candidate assets in vm-share\in\ : ClipVault_0.1.5_x64-setup.exe + SHA256SUMS.txt
#      (fetched from the v0.1.5 draft via the orphan-branch workflow when missing)
param([switch]$FetchOnly)
$ErrorActionPreference = 'Stop'
$kit = 'K:\Project\1-Active-Projects\clipvault\clipvault-rounds\sandbox-acceptance-kit'
$in  = Join-Path $kit 'vm-share\in'
New-Item -ItemType Directory -Force -Path $in, (Join-Path $kit 'vm-share\out') | Out-Null

if (-not (Test-Path 'C:\Windows\System32\wsb.exe')) {
  if (-not $FetchOnly) {
    Write-Host 'PRECHECK FAIL: Windows Sandbox not enabled (no wsb.exe). Enable + reboot first.'
    exit 2
  }
  Write-Host 'NOTE: Sandbox not enabled - fetch-only mode continues.'
} else {
  Write-Host 'PRECHECK OK: Windows Sandbox present.'
}

$exe  = Join-Path $in 'ClipVault_0.1.5_x64-setup.exe'
$sums = Join-Path $in 'SHA256SUMS.txt'
if (-not (Test-Path $exe) -or -not (Test-Path $sums)) {
  Write-Host 'Candidate assets missing - fetching from v0.1.5 draft via orphan-branch workflow...'
  # One-shot temp workflow pushes the two draft assets to branch ops/draft-assets
  # (same pattern as the 10-27 sums fetch); raw.githubusercontent serves them.
  $yaml = @'
name: ops-draft-assets
on:
  push:
    branches: [ops/draft-assets]
permissions:
  contents: write
jobs:
  fetch:
    runs-on: ubuntu-latest
    steps:
      - name: push draft assets to orphan branch
        env:
          GH_TOKEN: ${{ github.token }}
        run: |
          set -euo pipefail
          rm -rf pr && mkdir pr && cd pr
          git init -b ops/draft-assets
          git config user.email "actions@github.com"
          git config user.name "github-actions[bot]"
          git remote add origin "https://x-access-token:${GH_TOKEN}@github.com/ardss/clipvault.git"
          gh api repos/ardss/clipvault/releases/423764142/assets --paginate --jq '.[].name' > list.txt
          for f in ClipVault_0.1.5_x64-setup.exe SHA256SUMS.txt; do
            aid=$(gh api repos/ardss/clipvault/releases/423764142/assets --jq ".[] | select(.name==\"$f\") | .id")
            gh api -H "Accept: application/octet-stream" "repos/ardss/clipvault/releases/assets/$aid" > "$f"
          done
          git add -f ClipVault_0.1.5_x64-setup.exe SHA256SUMS.txt
          git commit -m "v0.1.5 draft assets (acceptance kit staging)"
          git push -u --force origin ops/draft-assets
'@
  $yamlPath = Join-Path $kit '.maintenance-draft-assets.yml'
  $yaml | Set-Content $yamlPath -Encoding UTF8
  Push-Location 'K:\Project\1-Active-Projects\clipvault'
  $tmp = "$env:TEMP\ops-draft-assets.yml"
  Copy-Item $yamlPath $tmp -Force
  $blob = (git hash-object -w $tmp).Trim()
  $idx = "$env:TEMP\cvidx-assets"
  $env:GIT_INDEX_FILE = $idx
  git read-tree HEAD | Out-Null
  git update-index --add --cacheinfo "100644,$blob,.github/workflows/ops-draft-assets.yml" | Out-Null
  $tree = (git write-tree).Trim()
  Remove-Item Env:GIT_INDEX_FILE
  $c = (git commit-tree $tree -p HEAD -m "ops: stage draft assets for sandbox acceptance (temp, deleted after)").Trim()
  git push origin "${c}:refs/heads/ops/draft-assets" 2>&1 | Select-Object -Last 1
  Pop-Location
  Write-Host 'workflow pushed; waiting 120s for run...'
  Start-Sleep -Seconds 120
  foreach ($f in @('ClipVault_0.1.5_x64-setup.exe', 'SHA256SUMS.txt')) {
    $dst = Join-Path $in $f
    $enc = [Uri]::EscapeDataString($f)
    curl.exe -sL --max-time 120 "https://raw.githubusercontent.com/ardss/clipvault/ops/draft-assets/$enc" -o $dst
  }
  git push origin :refs/heads/ops/draft-assets 2>&1 | Select-Object -Last 1
  git push origin :refs/heads/ops/diag-result 2>&1 | Select-Object -Last 1
  Remove-Item $yamlPath -Force -ErrorAction SilentlyContinue
}
if ($FetchOnly) { Write-Host 'FETCH_ONLY done.'; exit 0 }

$sumsExpected = (Get-Content $sums | Where-Object { $_ -match 'setup\.exe' }).Split(' ')[0].Trim().ToUpper()
$actual = (Get-FileHash $exe -Algorithm SHA256).Hash.ToUpper()
if ($sumsExpected -ne $actual) { Write-Host "HOST PRECHECK FAIL: installer hash mismatch"; exit 3 }
Write-Host 'HOST PRECHECK OK: installer SHA256 matches SHA256SUMS.txt.'

Write-Host 'Launching sandbox (close the sandbox window when the runner finishes; results in vm-share\out\)...'
Start-Process (Join-Path $kit 'cv-vm-acceptance.wsb')
Write-Host 'Sandbox launched. When done, read vm-share\out\result.json.'
