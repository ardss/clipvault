# Idle-gated isolated paste regression watcher.
# Fires ONLY when the desktop has been idle >=10 minutes. Per-scenario foreground
# hard-verification stays in the driver (fail-safe aborts). Restores the real
# instance afterwards. No global synthetic keys are ever sent: the panel is driven
# through its own WebView via CDP trusted clicks; the sole keystroke is the app's
# own injector Ctrl+V guarded by its fail-closed focus contract, aimed at our
# controlled target that was foreground-verified milliseconds before.
$ErrorActionPreference = 'Continue'
$iso = 'C:\Users\13397\cv-iso-test'
$log = "$iso\watcher.log"
function L($m) { "$([DateTime]::Now.ToString('yyyy-MM-dd HH:mm:ss')) $m" | Tee-Object -FilePath $log -Append }

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public class IDLE {
  [StructLayout(LayoutKind.Sequential)] public struct LASTINPUTINFO { public uint cbSize; public uint dwTime; }
  [DllImport("user32.dll")] public static extern bool GetLastInputInfo(ref LASTINPUTINFO plii);
  public static uint IdleMs() {
    LASTINPUTINFO i = new LASTINPUTINFO(); i.cbSize = (uint)Marshal.SizeOf(i);
    if (!GetLastInputInfo(ref i)) return 0;
    unchecked { return (uint)Environment.TickCount - i.dwTime; }
  }
}
'@

L 'watcher armed; waiting for >=15min idle (uint-wrap safe)'
while ($true) {
  Start-Sleep -Seconds 60
  $idle = [IDLE]::IdleMs()
  L "idle=${idle}ms"
  if ($idle -ge 900000) { break }
}

L 'IDLE CONFIRMED - starting isolated acceptance'
try {
  # stop real instance (record for restore)
  Get-Process app -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq 'F:\Apps\ClipVault\app.exe' } | Stop-Process -Force
  Start-Sleep -Seconds 2
  L 'real instance stopped'
  # sweep orphaned resident injectors of the real exe: the injector exit check
  # (FindWindowW "ClipVaultListener") is class-global, not identifier-scoped, so
  # an orphan would stay alive next to the test instance and compete for the
  # same "<identifier>-inject" auto-reset event (paste lost ~50% of rounds)
  Get-CimInstance Win32_Process -Filter "Name='app.exe'" |
    Where-Object { $_.CommandLine -match '--injector' -and $_.ExecutablePath -eq 'F:\Apps\ClipVault\app.exe' } |
    ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
  L 'real injector orphans swept'

  # launch test instance with CDP + REDIRECTED APPDATA: the release exe is built
  # with the production identifier (com.clipvault.app), so without the redirect
  # the "isolated" instance would write captures straight into the REAL
  # %APPDATA%\com.clipvault.app (observed 2026-10-30: test marker rows in the
  # real clips.db). APPDATA override moves data dir, handshake dir and cvlog of
  # the whole process tree (main + spawned injector) into the isolation site.
  # SAVE the original: the finally block must RESTORE it - Remove-Item would
  # strip the variable from the environment and the restored real instance
  # would inherit NO APPDATA (observed 16:01:56: empty hotkey line).
  $realAppdata = $env:APPDATA
  $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = '--remote-debugging-port=9223'
  $env:APPDATA = 'C:\Users\13397\cv-iso-test\test-appdata'
  New-Item -ItemType Directory -Force -Path "$env:APPDATA" | Out-Null
  Start-Process 'K:\Project\1-Active-Projects\clipvault\src-tauri\target\release\app.exe'
  $env:APPDATA = $realAppdata
  Start-Sleep -Seconds 6
  $cdpReady = $false
  foreach ($k in 1..20) {
    Start-Sleep -Seconds 2
    try { $null = Invoke-WebRequest -Uri 'http://127.0.0.1:9223/json/version' -UseBasicParsing -TimeoutSec 3; $cdpReady = $true; break } catch { }
  }
  if (-not $cdpReady) { throw 'CDP 9223 never became ready' }
  L "test instance up (cdp ready after ~$($k*2)s)"

  # spawn targets
  Start-Process powershell -ArgumentList '-NoProfile','-ExecutionPolicy','Bypass','-File','K:\Project\1-Active-Projects\clipvault\tmp-ux\spawn-targets.ps1' -WindowStyle Hidden
  Start-Sleep -Seconds 12
  L 'targets spawned'

  # run driver (per-scenario foreground verification inside).
  # Bounded: a hung node (e.g. playwright close on headed browser) must never wedge
  # the watcher - kill after 700s and move on to teardown. Raised from 240s
  # (2026-10-30): patient focus-take retries (foreign UWP foreground holders such
  # as the Settings app can pin the foreground for ~60s) need the headroom.
  function Run-Driver {
    $p = Start-Process node -ArgumentList 'K:\Project\1-Active-Projects\clipvault\tmp-ux\driver.mjs' -PassThru -WindowStyle Hidden -RedirectStandardOutput "$iso\driver-stdout.txt" -RedirectStandardError "$iso\driver-stderr.txt"
    if (-not $p.WaitForExit(700000)) {
      Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
      L 'driver TIMEOUT (700s) - killed'
    }
    return (Get-Content "$iso\driver-stdout.txt" -Raw -ErrorAction SilentlyContinue)
  }
  $out = Run-Driver
  if (($out | Out-String) -match 'FATAL') {
    L 'driver FATAL - retrying once (re-check CDP, restart test instance if CDP died)'
    $cdp2 = $false
    try { $null = Invoke-WebRequest -Uri 'http://127.0.0.1:9223/json/version' -UseBasicParsing -TimeoutSec 3; $cdp2 = $true } catch { }
    if (-not $cdp2) {
      Get-Process app -ErrorAction SilentlyContinue | Where-Object { $_.Path -like '*target\release*' } | Stop-Process -Force -ErrorAction SilentlyContinue
      Start-Sleep -Seconds 2
      if (-not $realAppdata) { $realAppdata = $env:APPDATA }
      $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = '--remote-debugging-port=9223'
      $env:APPDATA = 'C:\Users\13397\cv-iso-test\test-appdata'
      New-Item -ItemType Directory -Force -Path "$env:APPDATA" | Out-Null
      Start-Process 'K:\Project\1-Active-Projects\clipvault\src-tauri\target\release\app.exe'
      $env:APPDATA = $realAppdata
      foreach ($k in 1..15) {
        Start-Sleep -Seconds 2
        try { $null = Invoke-WebRequest -Uri 'http://127.0.0.1:9223/json/version' -UseBasicParsing -TimeoutSec 3; $cdp2 = $true; break } catch { }
      }
      if ($cdp2) { L 'test instance relaunched (cdp ready)' } else { L 'CDP 9223 still down; retry aborted' }
    }
    if ($cdp2) { $out = Run-Driver; L 'driver retry finished' }
  }
  $out | Out-File "$iso\driver-output.txt" -Encoding UTF8
  L 'driver finished'
}
catch { L "WATCHER ERROR: $_" }
finally {
  # teardown: close targets, stop test instance, restore real app
  Get-Process | Where-Object { $_.MainWindowTitle -in 'CV-ISO-HOST','CV-ISO-BROWSER-TARGET' } | Stop-Process -Force -ErrorAction SilentlyContinue
  # notepad: never blanket-kill - the user's own notepad windows may share the process.
  # Kill only a fresh (own) notepad process. DO NOT UIA-close a shared window:
  # (2026-10-30) after Invoke("Don't Save") discards OUR tab, the same top-level
  # hwnd flips to the USER'S tabs, so any further close/invoke in the same script
  # runs against user buffers - one such invoke discarded a user's dirty tab.
  if ($realAppdata) { $env:APPDATA = $realAppdata }
  try { $t = Get-Content "$iso\targets.json" -Raw | ConvertFrom-Json } catch { $t = $null }
  if ($t -and $t.npPid -and $t.npPid -gt 0) {
    $pre = @(); if ($t.preNotepadPids) { $pre = ([string]$t.preNotepadPids) -split ',' }
    if ($pre -notcontains [string]$t.npPid) {
      Stop-Process -Id $t.npPid -Force -ErrorAction SilentlyContinue
      L "notepad target killed (own fresh process pid=$($t.npPid))"
    } else {
      L "notepad target window left open (shared pre-existing process pid=$($t.npPid); UIA close is unsafe on a shared WinUI3 window - user tabs may be active)"
    }
  }
  Get-CimInstance Win32_Process -Filter "Name='msedge.exe' OR Name='chrome.exe'" | Where-Object { $_.CommandLine -match 'cv-iso-test' } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
  Start-Sleep -Seconds 1
  Get-Process app -ErrorAction SilentlyContinue | Where-Object { $_.Path -like '*target\release*' } | Stop-Process -Force
  Start-Sleep -Seconds 2
  Start-Process 'F:\Apps\ClipVault\app.exe'
  Start-Sleep -Seconds 3
  $ok = Select-String -Path "$env:APPDATA\com.clipvault.app\cv.log" -Pattern 'hotkey registered' | Select-Object -Last 1
  L "real instance restored; last hotkey line: $($ok.Line)"
  L 'WATCHER COMPLETE'
}
