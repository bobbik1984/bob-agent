!macro NSIS_HOOK_PREINSTALL
    ; Kill the sidecar process if it was left running after bob.exe was closed
    nsExec::ExecToStack 'taskkill /F /IM llm-engine.exe /T'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
    ; Also kill the sidecar process before uninstallation
    nsExec::ExecToStack 'taskkill /F /IM llm-engine.exe /T'
!macroend
