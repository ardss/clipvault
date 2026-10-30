param([long]$hwnd, [int]$pid2 = 0, [int]$killIfOwnProcess = 0)
# Graceful close of OUR test notepad window via UI Automation WindowPattern.Close
# (accessibility API - no synthetic key/mouse input). If a save prompt appears,
# dismiss it by UIA-invoking the Don't Save button. Never force-kills a process
# unless -killIfOwnProcess names a process we spawned ourselves (fresh notepad).
# Prints: CLOSED | RESIDUE | KILLED
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$el = $null
try { $el = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]$hwnd) } catch {}
if (-not $el) { Write-Output 'CLOSED'; exit 0 }   # window already gone

function Test-Alive([System.Windows.Automation.AutomationElement]$e) {
  if (-not $e) { return $false }
  try { $h2 = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]$hwnd); return ($null -ne $h2) } catch { return $false }
}

try { $el.GetCurrentPattern([System.Windows.Automation.WindowPattern]::Pattern).Close() } catch {}
for ($i = 0; $i -lt 6 -and (Test-Alive $el); $i++) { Start-Sleep -Milliseconds 500 }
if (-not (Test-Alive $el)) { Write-Output 'CLOSED'; exit 0 }

# save prompt likely showing: UIA-invoke Don't Save (不保存 / Don't Save)
try {
  $btn = $el.FindFirst([System.Windows.Automation.TreeScope]::Descendants,
    (New-Object System.Windows.Automation.AndCondition(
      (New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::Button)),
      (New-Object System.Windows.Automation.OrCondition(
        (New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, '不保存')),
        (New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, "Don't Save")))))))
  if ($btn) { $btn.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke() }
} catch {}
for ($i = 0; $i -lt 6 -and (Test-Alive $el); $i++) { Start-Sleep -Milliseconds 500 }
if (-not (Test-Alive $el)) { Write-Output 'CLOSED'; exit 0 }

# last resort: if the window-owning process is one we spawned ourselves, kill it
if ($killIfOwnProcess -gt 0) {
  Stop-Process -Id $killIfOwnProcess -Force -ErrorAction SilentlyContinue
  Start-Sleep -Milliseconds 800
  if (-not (Test-Alive $el)) { Write-Output 'KILLED'; exit 0 }
}
Write-Output 'RESIDUE'
