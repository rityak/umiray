; umiray installer hooks (B-044). ASCII only: makensis reads a file without BOM in the
; system codepage.

; Before uninstalling. The hook runs before the installer closes the client by force,
; without any cleanup: a running client is asked to leave and restores the Windows proxy
; and firewall itself; a snapshot left by a crashed one is restored by the same call.
!macro NSIS_HOOK_PREUNINSTALL
  ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --uninstall'
!macroend
