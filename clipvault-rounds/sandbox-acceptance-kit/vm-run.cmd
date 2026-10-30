@echo off
rem ClipVault v0.1.5 isolated acceptance - VM-side entry (launched by LogonCommand)
cd /d "C:\Users\WDAGUtilityAccount\Desktop\vm-share"
echo [%date% %time%] VM acceptance starting > out\vm-run.log
powershell -NoProfile -ExecutionPolicy Bypass -File "C:\Users\WDAGUtilityAccount\Desktop\vm-share\vm-acceptance.ps1" >> out\vm-run.log 2>&1
echo [%date% %time%] VM acceptance finished >> out\vm-run.log
