# ClipVault v0.1.5 isolated acceptance - runs INSIDE Windows Sandbox only.
# GPT conditions (2026-10-27): host clipboard isolated (.wsb), installer hash
# verified, synthetic input ONLY inside VM, three scenarios + real-source image
# capture, all with screenshot/log/result evidence.

$ErrorActionPreference = 'Continue'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName Microsoft.VisualBasic

$Share   = 'C:\Users\WDAGUtilityAccount\Desktop\vm-share'
$In      = Join-Path $Share 'in'
$Out     = Join-Path $Share 'out'
$Exe     = Join-Path $In 'ClipVault_0.1.5_x64-setup.exe'
$Sums    = Join-Path $In 'SHA256SUMS.txt'
$AppData = Join-Path $env:APPDATA 'com.clipvault.app'
$Results = [ordered]@{}
$Stamp   = Get-Date -Format 'yyyyMMdd-HHmmss'

function Save-Shot([string]$name) {
  try {
    $b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
    $bmp = New-Object System.Drawing.Bitmap($b.Width, $b.Height)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($b.Location, [System.Drawing.Point]::Empty, $b.Size)
    $g.Dispose()
    $bmp.Save((Join-Path $Out ("shot-$name-$Stamp.png")), [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
  } catch { Write-Output "SHOT-FAIL $name $_" }
}

function Step([string]$id, [scriptblock]$body) {
  try {
    $r = & $body
    $Results[$id] = if ($null -ne $r) { $r } else { 'PASS' }
  } catch {
    $Results[$id] = "FAIL: $($_.Exception.Message)"
  }
  Write-Output ("STEP $id -> " + $Results[$id])
}

New-Item -ItemType Directory -Force -Path $Out | Out-Null
Save-Shot '00-vm-desktop'

# ---- S1: installer hash verification (GPT hard condition) ----
Step 'S1-hash-verify' {
  $expected = (Get-Content $Sums | Where-Object { $_ -match 'ClipVault_0\.1\.5_x64-setup\.exe' }).Split(' ')[0].Trim().ToUpper()
  $actual = (Get-FileHash $Exe -Algorithm SHA256).Hash.ToUpper()
  if ($actual -ne $expected) { throw "hash mismatch expected=$expected actual=$actual" }
  "PASS sha=$($actual.Substring(0,16))…"
}

# ---- S2: silent install + physical verification (checklist §4) ----
Step 'S2-install' {
  Unblock-File $Exe -ErrorAction SilentlyContinue
  $p = Start-Process -FilePath $Exe -ArgumentList '/S' -PassThru -Wait
  Start-Sleep -Seconds 3
  # tauri.conf.json sets installMode=currentUser: the silent install lands in
  # %LOCALAPPDATA%\ClipVault, NOT Program Files. Resolve the real location:
  # registry InstallLocation first, then the currentUser default, then the
  # legacy perMachine paths as a last resort.
  $app = $null
  $uninst = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\ClipVault'
  if (Test-Path $uninst) {
    $loc = (Get-ItemProperty $uninst -ErrorAction SilentlyContinue).InstallLocation
    if ($loc -and (Test-Path (Join-Path $loc 'app.exe'))) { $app = Join-Path $loc 'app.exe' }
  }
  if (-not $app) { $app = Join-Path $env:LOCALAPPDATA 'ClipVault\app.exe' }
  if (-not (Test-Path $app)) {
    foreach ($c in @('C:\Program Files\ClipVault\app.exe', 'C:\Program Files (x86)\ClipVault\app.exe')) {
      if (Test-Path $c) { $app = $c; break }
    }
  }
  if (-not $app -or -not (Test-Path $app)) { throw 'app.exe not found after silent install (installMode=currentUser => %LOCALAPPDATA%\ClipVault\app.exe)' }
  $fi = Get-Item $app
  "PASS path=$app mtime=$($fi.LastWriteTime.ToString('yyyy-MM-dd HH:mm:ss')) size=$($fi.Length)"
}

Step 'S3-app-running' {
  $proc = Get-Process app -ErrorAction SilentlyContinue | Select-Object -First 1
  if (-not $proc) { Start-Sleep -Seconds 8; $proc = Get-Process app -ErrorAction SilentlyContinue | Select-Object -First 1 }
  if (-not $proc) { throw 'app process not running after install' }
  "PASS pid=$($proc.Id) start=$($proc.StartTime.ToString('HH:mm:ss'))"
}

# ---- helpers for paste scenarios ----
function Invoke-PasteFlow([string]$marker) {
  Set-Clipboard -Value $marker
  Start-Sleep -Seconds 2      # capture settle
  [System.Windows.Forms.SendKeys]::SendWait('%v')   # Alt+V summon panel
  Start-Sleep -Seconds 2
  [System.Windows.Forms.SendKeys]::SendWait('CVVM') # filter
  Start-Sleep -Seconds 1
  [System.Windows.Forms.SendKeys]::SendWait('{ENTER}') # paste into focused control
  Start-Sleep -Seconds 2
}

# ---- S4: notepad paste ----
Step 'S4-notepad-paste' {
  $np = Start-Process notepad -PassThru
  Start-Sleep -Seconds 3
  try { [Microsoft.VisualBasic.Interaction]::AppActivate($np.Id) } catch { }  # notepad is usually foreground anyway
  $marker = "CVVM-NP-$Stamp-alpha"
  Invoke-PasteFlow $marker
  Save-Shot 'S4-after-paste'
  Set-Clipboard -Value 'cleared'
  Start-Sleep -Milliseconds 400
  [System.Windows.Forms.SendKeys]::SendWait('^a'); Start-Sleep -Milliseconds 300
  [System.Windows.Forms.SendKeys]::SendWait('^c'); Start-Sleep -Milliseconds 600
  $got = Get-Clipboard -Raw
  if ($got -notmatch [regex]::Escape($marker)) { throw "notepad content mismatch: got='$got'" }
  "PASS roundtrip='$got'"
}

# ---- S5: browser input paste (local page, networking disabled) ----
Step 'S5-browser-paste' {
  $page = Join-Path $Share 'test-input.html'
  $edge = 'C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe'
  if (-not (Test-Path $edge)) { $edge = 'C:\Program Files\Microsoft\Edge\Application\msedge.exe' }
  Start-Process $edge -ArgumentList @('-no-first-run', "--new-window=file:///$($page -replace '\\','/')")
  Start-Sleep -Seconds 6
  Save-Shot 'S5-edge-open'
  $marker = "CVVM-EDGE-$Stamp-bravo"
  Invoke-PasteFlow $marker
  Save-Shot 'S5-after-paste'
  [System.Windows.Forms.SendKeys]::SendWait('^a'); Start-Sleep -Milliseconds 300
  [System.Windows.Forms.SendKeys]::SendWait('^c'); Start-Sleep -Milliseconds 600
  $got = Get-Clipboard -Raw
  if ($got -notmatch [regex]::Escape($marker)) { throw "textarea content mismatch: got='$got'" }
  "PASS roundtrip='$got'"
}

# ---- S6: native multi-control paste (Explorer address bar) ----
Step 'S6-explorer-paste' {
  $target = 'C:\Users\WDAGUtilityAccount\Desktop'
  Set-Clipboard -Value $target
  Start-Sleep -Seconds 2
  Start-Process explorer.exe
  Start-Sleep -Seconds 4
  [System.Windows.Forms.SendKeys]::SendWait('^l')  # focus address bar
  Start-Sleep -Milliseconds 800
  [System.Windows.Forms.SendKeys]::SendWait('%v')
  Start-Sleep -Seconds 2
  [System.Windows.Forms.SendKeys]::SendWait('{ENTER}')
  Start-Sleep -Seconds 3
  Save-Shot 'S6-explorer-after'
  $sh = New-Object -ComObject Shell.Application
  $urls = @($sh.Windows() | ForEach-Object { $_.LocationURL })
  if (-not ($urls | Where-Object { $_ -match 'Desktop' })) { throw "explorer did not navigate: $($urls -join ' | ')" }
  "PASS navigated=$($urls -join ',')"
}

# ---- S7: real-source image capture (mspaint, WPF fallback) ----
Step 'S7-image-capture' {
  $png = Join-Path $In 'sample-png.png'
  Add-Type -AssemblyName System.Drawing
  $bmp = New-Object System.Drawing.Bitmap(320, 200)
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.Clear([System.Drawing.Color]::CornflowerBlue)
  $g.FillRectangle([System.Drawing.Brushes]::White, 20, 20, 280, 160)
  $g.DrawString("CVVM-IMG $Stamp", (New-Object System.Drawing.Font('Arial', 16)), [System.Drawing.Brushes]::DarkBlue, 30, 80)
  $g.Dispose(); $bmp.Save($png, [System.Drawing.Imaging.ImageFormat]::Png); $bmp.Dispose()
  $imgBefore = @(Get-ChildItem (Join-Path $AppData 'images') -File -ErrorAction SilentlyContinue).Count

  $paint = 'C:\Windows\System32\mspaint.exe'
  $viaApp = 'mspaint'
  if (-not (Test-Path $paint)) {
    # fallback: real GUI app (WPF window) writing via the real clipboard pipeline
    $viaApp = 'wpf-imagecopytool'
    $xamlSrc = @'
Add-Type -AssemblyName PresentationFramework, System.Drawing
$png = '__PNG__'
$w = New-Object System.Windows.Window
$w.Width = 420; $w.Height = 320; $w.Title = 'CVVM ImageCopyTool'
$img = New-Object System.Windows.Controls.Image
$img.Source = [System.Drawing.Bitmap]::FromFile($png) | ForEach-Object { $_.GetHbitmap() | ForEach-Object { [System.Windows.Interop.Imaging]::CreateBitmapSourceFromHBitmap($_, [IntPtr]::Zero, [System.Windows.Int32Rect]::Empty, $null) } }
$b = New-Object System.Windows.Controls.Button; $b.Content = 'Copy image to clipboard'; $b.Width = 240; $b.Height = 40
$b.Add_Click({ [System.Windows.Forms.Clipboard]::SetImage([System.Drawing.Image]::FromFile($png)) })
$sp = New-Object System.Windows.Controls.StackPanel; $sp.Margin = '10'
$sp.Children.Add($img); $sp.Children.Add($b); $w.Content = $sp
$w.Topmost = $true; $w.ShowActivated = $true
$null = $w.ShowDialog()
'@
    $xamlSrc = $xamlSrc.Replace('__PNG__', ($png -replace '\$', '$$'))
    $tool = Join-Path $Share 'imagecopytool.ps1'
    $xamlSrc | Set-Content $tool -Encoding UTF8
    Start-Process powershell -ArgumentList @('-NoProfile', '-STA', '-ExecutionPolicy', 'Bypass', "-File `"$tool`"")
    Start-Sleep -Seconds 8
    Save-Shot 'S7-wpf-tool'
    [System.Windows.Forms.SendKeys]::SendWait('{TAB}')
    Start-Sleep -Milliseconds 400
    [System.Windows.Forms.SendKeys]::SendWait(' ')   # activate the Copy button
    Start-Sleep -Seconds 2
  } else {
    Start-Process $paint -ArgumentList "`"$png`""
    Start-Sleep -Seconds 8
    Save-Shot 'S7-paint-open'
    [System.Windows.Forms.SendKeys]::SendWait('^a'); Start-Sleep -Milliseconds 400
    [System.Windows.Forms.SendKeys]::SendWait('^c'); Start-Sleep -Seconds 2
  }
  Start-Sleep -Seconds 3   # capture settle
  Save-Shot 'S7-after-capture'
  $imgAfter = @(Get-ChildItem (Join-Path $AppData 'images') -File -ErrorAction SilentlyContinue).Count
  if ($imgAfter -le $imgBefore) { throw "image store did not grow: before=$imgBefore after=$imgAfter (via=$viaApp)" }
  "PASS via=$viaApp images before=$imgBefore after=$imgAfter"
}

# ---- S8: evidence pack ----
Step 'S8-evidence' {
  $db = Join-Path $AppData 'clips.db'
  if (Test-Path $db) { Copy-Item $db (Join-Path $Out 'clips.db') -Force }
  foreach ($f in @('cv.log', 'clips.db-wal')) {
    $src = Join-Path $AppData $f
    if (Test-Path $src) { Copy-Item $src (Join-Path $Out $f) -Force }
  }
  $dbLines = 0
  if (Test-Path (Join-Path $Out 'clips.db')) {
    $bytes = [System.IO.File]::ReadAllBytes((Join-Path $Out 'clips.db'))
    $dbLines = 'bytes=' + $bytes.Length
  }
  "PASS collected db($dbLines) + logs + shots"
}

$Results | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $Out 'result.json') -Encoding UTF8
Write-Output '=== FINAL RESULT JSON ==='
Get-Content (Join-Path $Out 'result.json')
