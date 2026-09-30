; Custom NSIS hooks for the ClipVault installer.
; Auto-launch the app after a successful install/update, so updating is
; always "run installer -> app is back in the tray" with no extra clicks.
!macro NSIS_HOOK_POSTINSTALL
  Exec '"$INSTDIR\app.exe"'
!macroend
