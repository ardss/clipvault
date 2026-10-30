# Sends ESC to the foreground window (in-repo replacement for C:\tmp\esc.ps1).
Add-Type -AssemblyName System.Windows.Forms
[System.Windows.Forms.SendKeys]::SendWait('{ESC}')
