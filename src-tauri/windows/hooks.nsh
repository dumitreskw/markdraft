; Per-user setup.exe hooks for the `markdraft` command and folder menu entries
; (Settings > System). Updates keep whatever the user chose.

; After a fresh install, offer both (Yes is the default, also for silent /S).
!macro NSIS_HOOK_POSTINSTALL
  ${If} $UpdateMode <> 1
    MessageBox MB_YESNO|MB_ICONQUESTION "Add MarkDraft to your system?$\r$\n$\r$\n  - the markdraft command in terminals (markdraft . opens the current folder)$\r$\n  - $\"Open with MarkDraft$\" when you right-click a folder$\r$\n$\r$\nYou can change this later in MarkDraft under Settings > System." /SD IDYES IDNO markdraft_skip_integration
      ExecWait '"$INSTDIR\markdraft.exe" --install-integration'
    markdraft_skip_integration:
  ${EndIf}
!macroend

; Remove them on uninstall, but not while updating.
!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode <> 1
    ExecWait '"$INSTDIR\markdraft.exe" --remove-integration'
  ${EndIf}
!macroend
