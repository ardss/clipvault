# ClipVault v0.1.5 candidate — paste & focus regression, contained to self-spawned windows.
$ErrorActionPreference = 'Stop'
$iso = 'C:\Users\13397\cv-iso-test'
$log = "$iso\paste-regression.log"
function L($m) { "$([DateTime]::Now.ToString('HH:mm:ss')) $m" | Tee-Object -FilePath $log -Append }

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public class FG {
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  public static uint Pid() { uint p; GetWindowThreadProcessId(GetForegroundWindow(), out p); return p; }
}
'@
Add-Type -AssemblyName System.Windows.Forms

$marker = 'CV-ISO-FICTIONAL-20261028 文本回归样本'
$form = New-Object System.Windows.Forms.Form
$form.Text = 'CV-ISO-HOST'
$form.Size = New-Object System.Drawing.Size(600, 300)
$tb = New-Object System.Windows.Forms.TextBox
$tb.Multiline = $true; $tb.Size = New-Object System.Drawing.Size(280, 200); $tb.Location = New-Object System.Drawing.Point(10, 20); $tb.Text = 'BEFORE'
$rtb = New-Object System.Windows.Forms.RichTextBox
$rtb.Size = New-Object System.Drawing.Size(280, 200); $rtb.Location = New-Object System.Drawing.Point(300, 20); $rtb.Text = 'BEFORE'
$form.Controls.Add($tb); $form.Controls.Add($rtb)
$null = $form.Handle
$form.Show()
[System.Windows.Forms.Application]::DoEvents()
Start-Sleep -Milliseconds 500
$tb.Focus() | Out-Null
[System.Windows.Forms.Application]::DoEvents()
L ("A: foreground pid before hotkey = " + [FG]::Pid() + " (me=$PID)")

Set-Clipboard -Value $marker
Start-Sleep -Milliseconds 800
L 'A: fictional text on clipboard'

[System.Windows.Forms.SendKeys]::SendWait('%v')
Start-Sleep -Milliseconds 900
L 'A: Alt+V sent'

[System.Windows.Forms.SendKeys]::SendWait('{DOWN}{ENTER}')
Start-Sleep -Milliseconds 1500
L 'A: DOWN+ENTER sent'

$fgp = [FG]::Pid()
$fg = Get-Process -Id $fgp -ErrorAction SilentlyContinue
L ("A: foreground after paste: " + $fg.ProcessName + " pid=" + $fgp)
L ("A: TextBox now: " + $tb.Text)
L ("A: focus returned to host: " + ($fgp -eq $PID))
$form.Close()
L 'A: done'
