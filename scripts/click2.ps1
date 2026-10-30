# Left-clicks at absolute screen coordinates (in-repo replacement for
# C:\tmp\click2.ps1). Usage: powershell -File click2.ps1 -x 2950 -y 450
param([int]$x, [int]$y)
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public class CVClick {
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint dwFlags, uint dx, uint dy, uint dwData, UIntPtr dwExtraInfo);
}
"@
[CVClick]::SetCursorPos($x, $y) | Out-Null
Start-Sleep -Milliseconds 80
[CVClick]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)  # LEFTDOWN
Start-Sleep -Milliseconds 40
[CVClick]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)  # LEFTUP
