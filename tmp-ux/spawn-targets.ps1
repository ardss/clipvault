# Controlled paste targets for the isolated acceptance run.
# Zero global synthetic input: the panel is driven via WebView2 CDP (in-page trusted
# clicks); the only keystroke anywhere is the app's own injector Ctrl+V, which the
# app fires ONLY after its fail-closed focus verification against our target.
#
# Win11 fix: System32 notepad.exe and msedge.exe both hand off to another process,
# so the Start-Process pid is NOT the window-owner pid (focus-take pid enforcement
# then fails with FG_HWND=0). We resolve the REAL owner pid + hwnd of each target
# window by enumeration and record those in targets.json. Notepad may additionally
# hand off into the user's already-running Notepad process; we snapshot pre-existing
# notepad pids so teardown never force-kills a process holding user buffers.
$ErrorActionPreference = 'Stop'
$iso = 'C:\Users\13397\cv-iso-test'
$log = "$iso\targets.log"
function L($m) { "$([DateTime]::Now.ToString('HH:mm:ss')) $m" | Tee-Object -FilePath $log -Append }

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public class FG2 {
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  public static uint ForegroundPid() { uint p; GetWindowThreadProcessId(GetForegroundWindow(), out p); return p; }
  public delegate bool EnumProc(IntPtr h, IntPtr lp);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr lp);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern int GetWindowTextW(IntPtr h, [MarshalAs(UnmanagedType.LPWStr)] StringBuilder sb, int max);
  public static IntPtr FindByTitlePart(string part) {
    IntPtr found = IntPtr.Zero;
    EnumWindows((h, lp) => {
      if (!IsWindowVisible(h)) return true;
      var sb = new StringBuilder(256); GetWindowTextW(h, sb, 256);
      if (sb.ToString().Contains(part)) { found = h; return false; }
      return true;
    }, IntPtr.Zero);
    return found;
  }
  public static IntPtr WaitForWindow(string part, int timeoutMs) {
    var dl = DateTime.UtcNow.AddMilliseconds(timeoutMs);
    while (DateTime.UtcNow < dl) {
      var h = FindByTitlePart(part);
      if (h != IntPtr.Zero) return h;
      System.Threading.Thread.Sleep(400);
    }
    return IntPtr.Zero;
  }
}
'@

function Resolve-Target([string]$titlePart, [int]$timeoutMs, [int]$fallbackPid, [string]$label) {
  $h = [FG2]::WaitForWindow($titlePart, $timeoutMs)
  if ($h -eq [IntPtr]::Zero) {
    L "$label window '$titlePart' NOT FOUND after ${timeoutMs}ms; falling back to launcher pid=$fallbackPid"
    return @{ hwnd = [int64]0; pid = $fallbackPid }
  }
  $p = 0; $null = [FG2]::GetWindowThreadProcessId($h, [ref]$p)
  L "$label window hwnd=$h owner-pid=$p (launcher pid=$fallbackPid)"
  return @{ hwnd = [int64]$h; pid = $p }
}

# ---- target 1: WinForms host (classic edit + riched) ----
Add-Type -AssemblyName System.Windows.Forms
$f = New-Object System.Windows.Forms.Form
$f.Text = 'CV-ISO-HOST'
$f.Size = New-Object System.Drawing.Size(620, 320)
$tb = New-Object System.Windows.Forms.TextBox
$tb.Multiline = $true; $tb.Size = New-Object System.Drawing.Size(280, 220); $tb.Location = New-Object System.Drawing.Point(10, 20); $tb.Text = 'BEFORE'
$rtb = New-Object System.Windows.Forms.RichTextBox
$rtb.Size = New-Object System.Drawing.Size(280, 220); $rtb.Location = New-Object System.Drawing.Point(300, 20); $rtb.Text = 'BEFORE'
$f.Controls.Add($tb); $f.Controls.Add($rtb)
$null = $f.Handle
$f.Show()
[System.Windows.Forms.Application]::DoEvents()
$null = $tb.Focus(); $formPid = $PID
L "HOST pid=$formPid tbHandle=$($tb.Handle) rtbHandle=$($rtb.Handle)"
$tb | ConvertTo-Json | Set-Content "$iso\host-ref.json" -Encoding UTF8

# ---- target 2: notepad with a temp file ----
# Snapshot pre-existing notepad pids BEFORE spawning so teardown can distinguish
# our own fresh notepad process from a handoff into the user's running instance.
$preNp = @(Get-Process notepad -ErrorAction SilentlyContinue | ForEach-Object { [string]$_.Id })
$npFile = "$iso\notepad-target.txt"
Set-Content $npFile 'BEFORE' -Encoding UTF8
$np = Start-Process 'C:\Windows\System32\notepad.exe' -ArgumentList $npFile -PassThru
$npR = Resolve-Target 'notepad-target' 10000 $np.Id 'NOTEPAD'
if ($preNp -contains [string]$npR.pid) {
  L "NOTEPAD WARNING: window owned by PRE-EXISTING notepad process pid=$($npR.pid) (single-instance handoff); teardown will not kill it"
} else {
  L "NOTEPAD fresh own process pid=$($npR.pid)"
}

# ---- target 3: Edge (no updater nag under throwaway profile), CDP-readable ----
$edge = 'C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe'
if (-not (Test-Path $edge)) { $edge = 'C:\Program Files\Microsoft\Edge\Application\msedge.exe' }
$ch = Start-Process $edge -ArgumentList @("--user-data-dir=$iso\edge-profile", '--no-first-run', '--no-default-browser-check', '--remote-debugging-port=9224', "--app=file:///$($iso -replace '\\','/')/target.html", '--window-size=700,500') -PassThru
$chR = Resolve-Target 'CV-ISO-BROWSER-TARGET' 12000 $ch.Id 'EDGE'

# persist for the driver script
@{
  hostPid = $PID
  npPid = $npR.pid; npHwnd = $npR.hwnd; preNotepadPids = ($preNp -join ',')
  chromePid = $chR.pid; chromeHwnd = $chR.hwnd
} | ConvertTo-Json | Set-Content "$iso\targets.json" -Encoding UTF8
L 'targets ready; staying alive (timer pump). Close via driver.'
while ($true) {
  [System.Windows.Forms.Application]::DoEvents()
  try { "TB=$($tb.Text)`nRTB=$($rtb.Text)`nAC=$($f.ActiveControl.Name)`nTBH=$($tb.Handle)`nRTBH=$($rtb.Handle)" | Set-Content "$iso\host-state.txt" -Encoding UTF8 } catch {}
  try {
    $sig = Get-Content "$iso\host-focus.txt" -ErrorAction SilentlyContinue
    if ($sig -eq 'tb') { $null = $tb.Focus() }
    elseif ($sig -eq 'rtb') { $null = $rtb.Focus() }
    if ($sig) { Remove-Item "$iso\host-focus.txt" -ErrorAction SilentlyContinue }
  } catch {}
  Start-Sleep -Milliseconds 150
}
