Unicode true
!ifndef APP_EXE
  !error "APP_EXE is required"
!endif
!ifndef TXT_WORKER_EXE
  !error "TXT_WORKER_EXE is required"
!endif
Name "Babel Tower Phase 3 TXT"
OutFile "babel-tower-phase3-txt-windows-x64.exe"
InstallDir "$LOCALAPPDATA\Programs\Babel Tower Phase 3 TXT"
RequestExecutionLevel user
SilentInstall silent
Section
  SetOutPath "$INSTDIR"
  File "${APP_EXE}"
  File "${TXT_WORKER_EXE}"
  FileOpen $0 "$INSTDIR\offline-install-marker.txt" w
  FileWrite $0 "No network access is required by this Phase 3 TXT vertical slice.$\r$\n"
  FileClose $0
  WriteUninstaller "$INSTDIR\uninstall.exe"
SectionEnd
Section "Uninstall"
  Delete "$INSTDIR\babel-phase3.exe"
  Delete "$INSTDIR\babel-txt-worker.exe"
  Delete "$INSTDIR\offline-install-marker.txt"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
SectionEnd
