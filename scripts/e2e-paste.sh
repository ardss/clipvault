ROOT='$(cd "$(dirname "$0")/.." && pwd)'
# Automated paste-landing E2E: kill app -> build -> start -> AltV -> Down -> Enter -> verify probe.
MARKER="E2E-MARKER-$(date +%s)"

taskkill //IM app.exe //F 2>/dev/null
# wait until the exe file is writable
for i in $(seq 1 20); do
  (mv "$ROOT/src-tauri/target/debug/app.exe" "$ROOT/src-tauri/target/debug/app.exe" 2>/dev/null) || { sleep 0.5; continue; }
  break
done
cd "$ROOT/src-tauri"
cargo build 2>&1 | tail -1
("$ROOT\src-tauri\target\debug\app.exe" > /tmp/cv3-app.log 2>&1 &)
# wait for the listener to be up
for i in $(seq 1 20); do
  grep -q "listener thread started" /tmp/cv3-app.log && break
  sleep 0.5
done
sleep 2

bash '$ROOT\\scripts\\probe-reset.sh'
rm -f /c/tmp/cv-probe.txt
powershell -Command "Set-Clipboard -Value '$MARKER'"
sleep 2
powershell -Command "(New-Object -ComObject WScript.Shell).AppActivate('CV-PROBE')"
sleep 2
powershell -ExecutionPolicy Bypass -File C:\tmp\esc.ps1; sleep 0.5
powershell -ExecutionPolicy Bypass -File "$ROOT\scripts\keys.ps1" AltV; sleep 1.5
powershell -ExecutionPolicy Bypass -File "$ROOT\scripts\keys.ps1" Down; sleep 0.5
powershell -ExecutionPolicy Bypass -File "$ROOT\scripts\keys.ps1" Enter; sleep 3

echo "=== probe content:"; cat /c/tmp/cv-probe.txt 2>/dev/null || echo "(missing)"
if grep -q "$MARKER" /c/tmp/cv-probe.txt 2>/dev/null; then
  echo "=== RESULT: PASS"
else
  echo "=== RESULT: FAIL"; grep -a "cv\] paste\|cv\] capture\|cv\] restore" /tmp/cv3-app.log
fi
