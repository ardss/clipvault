ROOT="$(cd "$(dirname "$0")/.." && pwd)"
#!/bin/bash
# Restart the CV-PROBE window and focus its textbox.
powershell -Command "Get-Process powershell -ErrorAction SilentlyContinue | Where-Object {\$_.MainWindowTitle -eq 'CV-PROBE'} | Stop-Process -Force" 2>/dev/null
powershell -Command "Start-Process powershell -ArgumentList '-STA','-ExecutionPolicy','Bypass','-File','$ROOT\\scripts\\paste-probe.ps1'"
sleep 3
powershell -ExecutionPolicy Bypass -File C:\tmp\click2.ps1 -x 2950 -y 450
sleep 1
