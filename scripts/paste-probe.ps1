# Paste probe: a WinForms textbox that mirrors its content to a file on change.
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
$f = New-Object System.Windows.Forms.Form
$f.Text = 'CV-PROBE'
$f.Size = New-Object System.Drawing.Size(500,300)
$f.StartPosition = 'Manual'
$f.Location = New-Object System.Drawing.Point(2700,300)
$tb = New-Object System.Windows.Forms.TextBox
$tb.Multiline = $true
$tb.Dock = 'Fill'
$tb.Name = 'probe'
$f.Controls.Add($tb)
$tb.Add_TextChanged({ [System.IO.File]::WriteAllText('C:\tmp\cv-probe.txt', $tb.Text) })
$f.Add_Shown({ $f.Activate(); $tb.Focus() })
[System.Windows.Forms.Application]::Run($f)
