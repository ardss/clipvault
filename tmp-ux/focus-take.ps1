param([string]$title, [int]$pid2)
# Deterministic foreground activation for OUR OWN test windows via the documented
# AttachThreadInput technique (window management API - no key/mouse injection).
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public class FT {
  [DllImport("user32.dll")] public static extern IntPtr FindWindowW([MarshalAs(UnmanagedType.LPWStr)] string cls, [MarshalAs(UnmanagedType.LPWStr)] string title);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
  [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint a, uint b, bool attach);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);

  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr lp);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  public delegate bool EnumProc(IntPtr h, IntPtr lp);
  [DllImport("user32.dll")] public static extern int GetWindowTextW(IntPtr h, [MarshalAs(UnmanagedType.LPWStr)] System.Text.StringBuilder sb, int max);
  public static IntPtr ByPid(uint wantPid) {
    IntPtr found = IntPtr.Zero;
    EnumWindows((h, lp) => {
      uint pid; GetWindowThreadProcessId(h, out pid);
      if (pid == wantPid && IsWindowVisible(h)) { found = h; return false; }
      return true;
    }, IntPtr.Zero);
    return found;
  }
  public static IntPtr ByTitlePart(string part) {
    IntPtr found = IntPtr.Zero;
    EnumWindows((h, lp) => {
      if (!IsWindowVisible(h)) return true;
      var sb = new System.Text.StringBuilder(256);
      GetWindowTextW(h, sb, 256);
      if (sb.ToString().Contains(part)) { found = h; return false; }
      return true;
    }, IntPtr.Zero);
    return found;
  }
  public static IntPtr Take(string title, uint wantPid) {
    IntPtr h;
    if (title == "-") h = ByPid(wantPid);
    else if (title == "none") h = ByTitlePart("notepad-target");
    else h = FindWindowW(null, title);
    if (h == IntPtr.Zero) h = ByTitlePart(title == "none" ? "notepad-target" : title);
    // pid enforcement only when discovered by pid; title-discovered windows may be
    // owned by a different process (notepad single-instance handoff)
    if (h != IntPtr.Zero && (title == "-")) { uint gp; GetWindowThreadProcessId(h, out gp); if (gp != wantPid) h = IntPtr.Zero; }
    if (h == IntPtr.Zero) return IntPtr.Zero;
    uint pid; GetWindowThreadProcessId(h, out pid);
    if (wantPid != 0 && pid != wantPid) return IntPtr.Zero;
    IntPtr fg = GetForegroundWindow();
    uint fgThread = fg != IntPtr.Zero ? GetWindowThreadProcessId(fg, out pid) : 0;
    uint myThread = GetCurrentThreadId();
    if (fgThread != 0 && fgThread != myThread) AttachThreadInput(myThread, fgThread, true);
    ShowWindow(h, 9); // SW_RESTORE
    BringWindowToTop(h);
    SetForegroundWindow(h);
    if (fgThread != 0 && fgThread != myThread) AttachThreadInput(myThread, fgThread, false);
    return GetForegroundWindow();
  }
}
'@
$r = [FT]::Take($title, $pid2)
"FG_HWND=$r"
if ($r -ne [IntPtr]::Zero) { $p=0; $null=[FT]::GetWindowThreadProcessId($r,[ref]$p); "FG_PID=$p" } else { "FG_PID=0" }
