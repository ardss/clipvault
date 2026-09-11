# Dump names of all clickable elements in the ClipVault panel.
Add-Type -AssemblyName UIAutomationClient
$root = [System.Windows.Automation.AutomationElement]::RootElement
$procs = Get-Process app -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Id
$found = $null
foreach ($w in $root.FindAll([System.Windows.Automation.TreeScope]::Children, [System.Windows.Automation.Condition]::TrueCondition)) {
  $p = $w.GetCurrentPropertyValue([System.Windows.Automation.AutomationElement]::ProcessIdProperty)
  if ($procs -contains $p -and $w.GetCurrentPropertyValue([System.Windows.Automation.AutomationElement]::NameProperty) -eq 'ClipVault') { $found = $w; break }
}
if (-not $found) { Write-Output 'NOWIN'; exit }
$btnCond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::Button)
$btns = $found.FindAll([System.Windows.Automation.TreeScope]::Descendants, $btnCond)
foreach ($b in $btns) {
  Write-Output ('BTN: [' + $b.GetCurrentPropertyValue([System.Windows.Automation.AutomationElement]::NameProperty) + ']')
}
Write-Output ('total: ' + $btns.Count)
