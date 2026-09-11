# Drive ClipVault panel buttons via UI Automation and capture each state (window-scoped, no desktop).
param([string]$name, [string]$out)
Add-Type -AssemblyName UIAutomationClient
$root = [System.Windows.Automation.AutomationElement]::RootElement
$cond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ProcessIdProperty, 0)
# find windows of app.exe
$procs = Get-Process app -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Id
$found = $null
foreach ($w in $root.FindAll([System.Windows.Automation.TreeScope]::Children, [System.Windows.Automation.Condition]::TrueCondition)) {
  $pid2 = $w.GetCurrentPropertyValue([System.Windows.Automation.AutomationElement]::ProcessIdProperty)
  if ($procs -contains $pid2 -and $w.GetCurrentPropertyValue([System.Windows.Automation.AutomationElement]::NameProperty) -eq 'ClipVault') { $found = $w; break }
}
if (-not $found) { Write-Output 'NOWIN'; exit }
$btnCond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, $name)
$btn = $found.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $btnCond)
if (-not $btn) { Write-Output 'NOBTN:'+$name; exit }
$inv = $btn.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
$inv.Invoke()
Write-Output 'CLICKED:'+$name
