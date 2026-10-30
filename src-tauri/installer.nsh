; Custom NSIS hooks for the ClipVault installer.
; Auto-launch the app after a successful install/update, so updating is
; always "run installer -> app is back in the tray" with no extra clicks.
; ${MAINBINARYNAME} is the packager's own define (installer.nsi, sourced from
; the crate name) — a hardcoded "app.exe" silently broke the auto-launch the
; moment the main binary was renamed, a rename the NSIS template explicitly
; supports (OldMainBinaryName migration). Hook insertion happens after the
; define, so ${MAINBINARYNAME} is in scope here.
!macro NSIS_HOOK_POSTINSTALL
  Exec '"$INSTDIR\${MAINBINARYNAME}.exe"'
!macroend
