$ErrorActionPreference = 'Continue'
$py = Get-Command python -ErrorAction SilentlyContinue
if (-not $py) { $py = Get-Command py -ErrorAction SilentlyContinue }
if (-not $py) { Write-Output "NO_PYTHON_FOUND"; exit 40 }
$script = 'C:\Users\13397\clipvault-deploy-20261030a\step1-rowcount.py'
$exePath = $py.Source
Write-Output ("PY_INTERP={0}" -f $exePath)
if ($exePath -like '*py.exe') { & $exePath -3 $script } else { & $exePath $script }
Write-Output ("PY_EXIT={0}" -f $LASTEXITCODE)
