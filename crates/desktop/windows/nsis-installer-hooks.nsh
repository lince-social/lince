!include LogicLib.nsh
!include nsDialogs.nsh

Var LinceSetupArgs
Var LinceAuthEnabled
Var LincePassword
Var LincePasswordField
Var LinceDialog

Function LincePromptPassword
  nsDialogs::Create 1018
  Pop $LinceDialog
  ${If} $LinceDialog == error
    Abort
  ${EndIf}

  ${NSD_CreateLabel} 0 0 100% 24u "Choose the initial admin password for Lince."
  ${NSD_CreatePassword} 0 32u 100% 12u ""
  Pop $LincePasswordField

  nsDialogs::Show
  ${NSD_GetText} $LincePasswordField $LincePassword
FunctionEnd

!macro NSIS_HOOK_POSTINSTALL
  StrCpy $LinceSetupArgs "--stage-desktop-install-setup"
  StrCpy $LinceAuthEnabled "0"
  StrCpy $LincePassword ""

  ${If} $LANGUAGE == ${LANG_PORTUGUESEBR}
    StrCpy $LinceSetupArgs "$LinceSetupArgs --language pt-BR"
  ${EndIf}

  MessageBox MB_YESNO "Start Lince when you sign in?" IDNO lince_skip_start_on_login
    StrCpy $LinceSetupArgs "$LinceSetupArgs --start-on-login"
    MessageBox MB_YESNO "Open Lince silently to the system tray when started automatically?" IDNO lince_skip_start_silent
      StrCpy $LinceSetupArgs "$LinceSetupArgs --start-silent"
  lince_skip_start_silent:
  lince_skip_start_on_login:

  MessageBox MB_YESNO "Enable authentication for Lince?" IDNO lince_stage_setup
    StrCpy $LinceAuthEnabled "1"
    Call LincePromptPassword
    ${If} $LincePassword == ""
      MessageBox MB_OK "A password is required when authentication is enabled."
      Call LincePromptPassword
    ${EndIf}
    ${If} $LincePassword == ""
      MessageBox MB_OK "Authentication setup was cancelled because no password was provided."
      StrCpy $LinceAuthEnabled "0"
    ${EndIf}

  lince_stage_setup:
  ${If} $LinceAuthEnabled == "1"
    ExecWait '"$INSTDIR\lince-desktop.exe" $LinceSetupArgs --auth-enabled --initial-admin-password "$LincePassword"'
  ${Else}
    ExecWait '"$INSTDIR\lince-desktop.exe" $LinceSetupArgs'
  ${EndIf}
!macroend
