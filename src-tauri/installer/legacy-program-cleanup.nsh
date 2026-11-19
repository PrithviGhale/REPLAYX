; Older REPLAYX installers included diagnostic executables that are no longer shipped.
; Limit removal to a recognized REPLAYX installation in the selected directory.
Var ReplayxExistingInstall

!macro NSIS_HOOK_PREINSTALL
  Push $0
  StrCpy $ReplayxExistingInstall 0
  ReadRegStr $0 SHCTX "${UNINSTKEY}" "InstallLocation"
  ${If} $0 == "$\"$INSTDIR$\""
    StrCpy $ReplayxExistingInstall 1
  ${EndIf}
  Pop $0
  ClearErrors
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ${If} $ReplayxExistingInstall == 1
    Delete "$INSTDIR\replayx-cli.exe"
    Delete "$INSTDIR\replayx-dda-profile.exe"
    Delete "$INSTDIR\replayx-gameproxy.exe"
    Delete "$INSTDIR\replayx-wgc-profile.exe"
  ${EndIf}
!macroend
