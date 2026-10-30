$ErrorActionPreference = 'Stop'
# Target selection strictly by executable path (no name-based blanket kill)
$target = 'F:\Apps\ClipVault\app.exe'
$procs = Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -eq $target }
if (-not $procs) { Write-Output "NO_PROCESS_MATCHING_PATH=$target"; exit 1 }
foreach ($p in $procs) {
  Write-Output ("STOP PID={0} PATH={1}" -f $p.ProcessId, $p.ExecutablePath)
  Stop-Process -Id $p.ProcessId -Force
}
Start-Sleep -Seconds 3
$left = @(Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -eq $target })
Write-Output ("REMAINING={0}" -f $left.Count)
if ($left.Count -ne 0) { exit 2 }
Write-Output "STOP_OK"
