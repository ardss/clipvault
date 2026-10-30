param([long]$hwnd = 0, [int]$pid2 = 0)
# Read a Notepad window's text via UI Automation (accessibility API - no input injection).
# Preferred: hwnd-scoped (AutomationElement.FromHandle) because Win11 Notepad may host
# several windows (incl. user tabs) inside ONE process - a pid lookup can grab the wrong
# window. Fallback: pid lookup on the top-level window. Prints value or NO-UIA-* marker.
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$w = $null
if ($hwnd -ne 0) {
  try { $w = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]$hwnd) } catch {}
  if (-not $w) { Write-Output 'NO-UIA-WINDOW'; exit 0 }
} else {
  $root = [System.Windows.Automation.AutomationElement]::RootElement
  $w = $root.FindFirst([System.Windows.Automation.TreeScope]::Children,
    (New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ProcessIdProperty, $pid2)))
  if (-not $w) { Write-Output 'NO-UIA-WINDOW'; exit 0 }
}
$val = $null
try { $val = $w.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).Current.Value } catch {}
if (-not $val) {
  $e = $w.FindFirst([System.Windows.Automation.TreeScope]::Descendants,
    (New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::IsValuePatternAvailableProperty, $true)))
  if ($e) { try { $val = $e.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).Current.Value } catch {} }
}
if (-not $val) {
  $e2 = $w.FindFirst([System.Windows.Automation.TreeScope]::Descendants,
    (New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::IsTextPatternAvailableProperty, $true)))
  if ($e2) { try { $val = $e2.GetCurrentPattern([System.Windows.Automation.TextPattern]::Pattern).DocumentRange.GetText(4000) } catch {} }
}
if ($val) { Write-Output $val } else { Write-Output 'NO-UIA-VALUE' }
