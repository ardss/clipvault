param([long]$hwnd = 0)
# Put UIA (accessibility) keyboard focus on the Notepad content editor.
# Win32 GetGUIThreadInfo only proves the WinUI island hwnd has focus; the XAML-
# internal focus can rest on the tab strip, where an injected Ctrl+V is swallowed.
# AutomationElement.Focus() is the accessibility SetFocus route - no synthetic
# key/mouse input. Prints FOCUSED:<snippet> or FOCUS-FAIL:<reason>.
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
try {
  $w = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]$hwnd)
  if (-not $w) { Write-Output 'FOCUS-FAIL:no-window'; exit 0 }
  $e = $w.FindFirst([System.Windows.Automation.TreeScope]::Descendants,
    (New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::IsValuePatternAvailableProperty, $true)))
  if (-not $e) {
    $e = $w.FindFirst([System.Windows.Automation.TreeScope]::Descendants,
      (New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::IsTextPatternAvailableProperty, $true)))
  }
if (-not $e) { Write-Output 'FOCUS-FAIL:no-edit-element'; exit 0 }
$e.SetFocus()
# verify: SetFocus returning is no proof - read back the accessibility-focused
# element and compare identity (2026-10-30: WinUI3 swallowed injected Ctrl+V even
# after a reported-OK SetFocus, so the verdict must be evidence, not the call)
Start-Sleep -Milliseconds 120
$fid = [System.Windows.Automation.AutomationElement]::FocusedElement
$verd = 'UNVERIFIED'
try {
  if ($fid -and ($fid.GetRuntimeId() -join ',' ) -eq ($e.GetRuntimeId() -join ',')) { $verd = 'VERIFIED' }
  else { $verd = 'SET-BUT-FOCUSED-ELSEWHERE:' + $(if ($fid) { $fid.Current.Name } else { '<null>' }) }
} catch { $verd = 'VERIFY-ERROR:' + $_.Exception.Message }
Write-Output ("FOCUSED:${verd}:" + $e.Current.Name)
} catch {
  Write-Output ("FOCUS-FAIL:" + $_.Exception.Message)
}
