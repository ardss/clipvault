param([int]$pid2)
$ErrorActionPreference='SilentlyContinue'
Add-Type -AssemblyName Microsoft.VisualBasic
$p = Get-Process -Id $pid2
if ($p -and $p.MainWindowTitle) { [Microsoft.VisualBasic.Interaction]::AppActivate($p.MainWindowTitle) | Out-Null }
