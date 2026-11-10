; Antes de desinstalar: fecha o programa e devolve o proxy automático do Windows.
!macro NSIS_HOOK_PREUNINSTALL
  nsExec::Exec 'taskkill /IM "live-ponte.exe" /F'
  nsExec::Exec '"$INSTDIR\live-ponte.exe" --limpar'
!macroend

; Antes de instalar por cima (atualização): fecha a versão em execução.
!macro NSIS_HOOK_PREINSTALL
  nsExec::Exec 'taskkill /IM "live-ponte.exe" /F'
!macroend
