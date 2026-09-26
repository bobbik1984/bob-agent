!macro NSIS_HOOK_PREINSTALL
    ; Kill bob.exe and sidecar if left running to prevent file locks during upgrade installation
    nsExec::ExecToStack 'taskkill /F /IM bob.exe /T'
    nsExec::ExecToStack 'taskkill /F /IM llm-engine.exe /T'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
    ; Also kill bob.exe and sidecar before uninstallation
    nsExec::ExecToStack 'taskkill /F /IM bob.exe /T'
    nsExec::ExecToStack 'taskkill /F /IM llm-engine.exe /T'
!macroend
