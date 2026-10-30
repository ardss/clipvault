Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public class FGX {
  [DllImport("user32.dll")] public static extern bool GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  public static uint Go() { uint p; GetWindowThreadProcessId(GetForegroundWindow(), out p); return p; }
}
'@
[FGX]::Go()
