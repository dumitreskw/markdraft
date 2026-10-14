; Removes the optional `markdraft` command and folder menu entries
; (Settings > System) on uninstall, but not during updates.
!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode <> 1
    ExecWait '"$INSTDIR\markdraft.exe" --remove-integration'
  ${EndIf}
!macroend
