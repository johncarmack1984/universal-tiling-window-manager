; Hooks Tauri's NSIS installer runs (bundle > windows > nsis > installerHooks
; in tauri.conf.json).
;
; A release installs two executables in one folder for every user. The
; command line, universal-tiling-window-manager.exe, is what Tauri calls the
; main executable: the Start menu, the installer's last step and an update's
; restart run it, and with no command it starts the tray app. The tray app is
; the release's window-manager executable, which these hooks name
; Window Manager.exe: it has UI Access, so Windows starts it only through the
; shell and never through CreateProcess, which is how the installer would
; start a main executable.

!define TRAYEXE "Window Manager.exe"
; What Tauri packs the tray app's executable as: its Cargo name.
!define TRAYEXEPACKED "window-manager.exe"
!define COMMANDLINEEXE "universal-tiling-window-manager.exe"
; Start at login, which only the app's menu turns on.
!define RUNKEY "Software\Microsoft\Windows\CurrentVersion\Run"
!define RUNAPPROVEDKEY "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run"
!define RUNVALUE "universal-tiling-window-manager"
; Where earlier releases installed the app, for one user: their entry in
; Settings > Apps, and the folder their installer remembered, under the
; publisher Tauri derives from the bundle identifier.
!define PERUSERUNINSTKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\Window Manager"
!define PERUSERDIRKEY "Software\johncarmack\Window Manager"

Var PerUserCopy

!macro NSIS_HOOK_PREINSTALL
  ; Tauri's own check covers the command line; the tray app is the copy that
  ; keeps running.
  !insertmacro CheckIfAppIsRunning "$INSTDIR\${TRAYEXE}" "${PRODUCTNAME}"
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; A tray app still running in another session, which the check above
  ; couldn't close, is replaced at the next restart.
  Delete /REBOOTOK "$INSTDIR\${TRAYEXE}"
  Rename /REBOOTOK "$INSTDIR\${TRAYEXEPACKED}" "$INSTDIR\${TRAYEXE}"
  !insertmacro MovePerUserCopy
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro CheckIfAppIsRunning "$INSTDIR\${TRAYEXE}" "${PRODUCTNAME}"
  Delete "$INSTDIR\${TRAYEXE}"
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $UpdateMode <> 1
    ; Start at login goes with the app, for the account uninstalling it.
    DeleteRegValue HKCU "${RUNKEY}" "${RUNVALUE}"
    DeleteRegValue HKCU "${RUNAPPROVEDKEY}" "${RUNVALUE}"
    ; So does what the installer remembered for the next install: the
    ; folder, and the language.
    DeleteRegKey HKLM "${MANUPRODUCTKEY}"
    DeleteRegKey /ifempty HKLM "${MANUKEY}"
    DeleteRegValue HKCU "${MANUPRODUCTKEY}" "Installer Language"
    DeleteRegKey /ifempty HKCU "${MANUPRODUCTKEY}"
    DeleteRegKey /ifempty HKCU "${MANUKEY}"
    ; With "Delete the application data" ticked, the log and the remembered
    ; choice of configuration go too; the configuration itself, in
    ; ~/.config/window-manager, is the user's and stays.
    ${If} $DeleteAppDataCheckboxState = 1
      SetShellVarContext current
      RmDir /r "$LOCALAPPDATA\window-manager"
      RmDir /r "$LOCALAPPDATA\universal-tiling-window-manager"
      SetShellVarContext all
    ${EndIf}
  ${EndIf}
!macroend

; Earlier releases installed the app for one user, in their own
; %LOCALAPPDATA%\Window Manager. The first release for every user moves the
; account that approves it over: that copy's files, its entry in Settings >
; Apps and its Start menu shortcut go, and Start at login, when it ran that
; copy, runs this one. Its uninstaller doesn't run: it sits where any program
; the user runs can change it, and this installer runs as an administrator.
; When an administrator approves the installer for a standard account, this
; is the administrator's account, and the app moves that user's Start at login
; over itself when it starts.
!macro MovePerUserCopy
  ReadRegStr $PerUserCopy HKCU "${PERUSERUNINSTKEY}" "InstallLocation"
  ; Tauri's installer records the folder in quotes.
  StrCpy $0 $PerUserCopy 1
  ${If} $0 == '"'
    StrCpy $PerUserCopy $PerUserCopy "" 1
    StrCpy $PerUserCopy $PerUserCopy -1
  ${EndIf}
  ${If} $PerUserCopy != ""
  ${AndIf} $PerUserCopy != $INSTDIR
    DetailPrint "Moving over from $PerUserCopy"
    ; A copy still running from there, as when this installer was run by
    ; hand rather than by that copy's update, closes first.
    !insertmacro CheckIfAppIsRunning "$PerUserCopy\${COMMANDLINEEXE}" "${PRODUCTNAME}"
    Delete "$PerUserCopy\${COMMANDLINEEXE}"
    Delete "$PerUserCopy\uninstall.exe"
    RMDir "$PerUserCopy"

    ; Its Start menu shortcut gives way to one for every user, which an
    ; update, as this install usually is, wouldn't otherwise make. A desktop
    ; shortcut leading to it leads here instead.
    SetShellVarContext current
    !insertmacro IsShortcutTarget "$SMPROGRAMS\${PRODUCTNAME}.lnk" "$PerUserCopy\${COMMANDLINEEXE}"
    Pop $0
    ${If} $0 = 1
      !insertmacro UnpinShortcut "$SMPROGRAMS\${PRODUCTNAME}.lnk"
      Delete "$SMPROGRAMS\${PRODUCTNAME}.lnk"
      SetShellVarContext all
      ${IfNot} ${FileExists} "$SMPROGRAMS\${PRODUCTNAME}.lnk"
        CreateShortcut "$SMPROGRAMS\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
        !insertmacro SetLnkAppUserModelId "$SMPROGRAMS\${PRODUCTNAME}.lnk"
      ${EndIf}
      SetShellVarContext current
    ${EndIf}
    !insertmacro IsShortcutTarget "$DESKTOP\${PRODUCTNAME}.lnk" "$PerUserCopy\${COMMANDLINEEXE}"
    Pop $0
    ${If} $0 = 1
      !insertmacro SetShortcutTarget "$DESKTOP\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    ${EndIf}
    SetShellVarContext all

    ; Its entry in Settings > Apps, and the folder its installer remembered.
    DeleteRegKey HKCU "${PERUSERUNINSTKEY}"
    DeleteRegValue HKCU "${PERUSERDIRKEY}" ""

    ReadRegStr $0 HKCU "${RUNKEY}" "${RUNVALUE}"
    ${If} $0 == '"$PerUserCopy\${COMMANDLINEEXE}"'
      WriteRegStr HKCU "${RUNKEY}" "${RUNVALUE}" '"$INSTDIR\${COMMANDLINEEXE}"'
    ${EndIf}
  ${EndIf}
!macroend
