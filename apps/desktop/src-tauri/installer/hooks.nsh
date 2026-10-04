; Crochets de l'installateur NSIS de Hearth (HRT-08).
; Messages : spec fonctionnelle « Installer le client », section 4 (français, tutoiement).

!include "WinVer.nsh"
!insertmacro GetRoot
!insertmacro DriveSpace

!define HEARTH_MIN_FREE_MB 50

; BR-CLIENT-014 : Windows 10 64 bits et 50 Mo libres, vérifiés AVANT toute copie.
; Un refus arrête l'installation sans que rien n'ait été écrit.
!macro NSIS_HOOK_PREINSTALL
  ${IfNot} ${AtLeastWin10}
  ${OrIfNot} ${RunningX64}
    MessageBox MB_OK|MB_ICONSTOP "Hearth a besoin de Windows 10 ou plus récent, en 64 bits. Ta version ne fonctionne pas." /SD IDOK
    Abort
  ${EndIf}

  ${GetRoot} "$INSTDIR" $R8
  ${DriveSpace} "$R8\" "/D=F /S=M" $R9
  ${If} $R9 < ${HEARTH_MIN_FREE_MB}
    MessageBox MB_OK|MB_ICONSTOP "Il manque de la place. Libère au moins ${HEARTH_MIN_FREE_MB} Mo. Espace disponible : $R9 Mo." /SD IDOK
    Abort
  ${EndIf}
!macroend

; BR-CLIENT-009 et BR-CLIENT-010 : l'arrêt propre de l'application en cours, le
; retrait de l'entrée de démarrage et le choix « garder / tout effacer » sont
; assurés par le modèle NSIS de Tauri (voir ADR-0010) ; rien à ajouter ici.
