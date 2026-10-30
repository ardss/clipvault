Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public class IDLEC {
  [StructLayout(LayoutKind.Sequential)] public struct LII { public uint cbSize; public uint dwTime; }
  [DllImport("user32.dll")] public static extern bool GetLastInputInfo(ref LII p);
  public static uint I() { LII i = new LII(); i.cbSize = (uint)Marshal.SizeOf(i); if (!GetLastInputInfo(ref i)) return 0; unchecked { return (uint)Environment.TickCount - i.dwTime; } }
}
'@
[idlec]::I()
