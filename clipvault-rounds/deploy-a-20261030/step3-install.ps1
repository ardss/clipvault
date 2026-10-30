$ErrorActionPreference = 'Stop'
$wd = 'C:\Users\13397\clipvault-deploy-20261030a'
$setup = Join-Path $wd 'ClipVault_0.1.6_x64-setup.exe'
Write-Output ("INSTALLER={0} EXISTS={1}" -f $setup, (Test-Path $setup))
if (-not (Test-Path $setup)) { exit 29 }
$p = Start-Process -FilePath $setup -ArgumentList '/S' -PassThru -Wait
Write-Output ("INSTALLER_EXIT={0}" -f $p.ExitCode)
Start-Sleep -Seconds 8
$exe = Get-Item 'C:\Users\13397\AppData\Local\ClipVault\app.exe' -ErrorAction SilentlyContinue
if ($exe) {
  Write-Output ("NEW_EXE_VERSION={0} NEW_EXE_MTIME={1}" -f $exe.VersionInfo.ProductVersion, $exe.LastWriteTime.ToString('o'))
} else {
  Write-Output "NEW_EXE_MISSING"
  exit 30
}
Write-Output "INSTALL_STEP_DONE"
