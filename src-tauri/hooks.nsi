!macro NSIS_HOOK_PREINSTALL
    ; Kill the sidecar process if it was left running after bob.exe was closed
    nsProcess::KillProcess "llm-engine.exe" $R0
!macroend

!macro NSIS_HOOK_PREUNINSTALL
    ; Also kill the sidecar process before uninstallation
    nsProcess::KillProcess "llm-engine.exe" $R0
!macroend
