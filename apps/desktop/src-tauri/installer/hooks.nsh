; Crochets de l'installateur NSIS de Hearth (HRT-08).
; Textes dans French.nsh (spec fonctionnelle « Installer le client », section 4).
;
; BR-CLIENT-014 : Windows 10 64 bits et 50 Mo libres sont vérifiés au plus tôt.
; Le modèle de Tauri n'offre pas de crochet avant ses sections, donc :
;  1. au démarrage de l'interface (.onGUIInit, avant toute page) ;
;  2. dans une section masquée placée AVANT toutes celles du modèle (WebView2,
;     copie des fichiers), seule voie en mode silencieux.
; Un refus à l'une ou l'autre n'a encore rien écrit : ni dossier d'installation,
; ni WebView2, ni entrée de registre. Limite : si l'utilisateur choisit de
; désinstaller l'ancienne version sur la page de réinstallation, celle-ci est
; retirée avant le contrôle de la section ; le contrôle de .onGUIInit, fait avant
; cette page sur le dossier proposé, rend ce cas très improbable.

!include "WinVer.nsh"
!insertmacro GetRoot
!insertmacro DriveSpace

!define HEARTH_MIN_FREE_MB 50

; Résultat dans $R9 : "" (tout va bien), "os" ou "disk" ; Mo libres dans $R8.
Function HearthPreflight
  StrCpy $R9 ""
  ${IfNot} ${AtLeastWin10}
  ${OrIfNot} ${RunningX64}
    StrCpy $R9 "os"
    Return
  ${EndIf}
  ${GetRoot} "$INSTDIR" $R7
  ${DriveSpace} "$R7\" "/D=F /S=M" $R8
  ${If} $R8 < ${HEARTH_MIN_FREE_MB}
    StrCpy $R9 "disk"
  ${EndIf}
FunctionEnd

Function HearthRefuse
  ${If} $R9 == "os"
    MessageBox MB_OK|MB_ICONSTOP "$(hearthWindowsTooOld)" /SD IDOK
  ${Else}
    MessageBox MB_OK|MB_ICONSTOP "$(hearthDiskTooSmall)" /SD IDOK
  ${EndIf}
FunctionEnd

Function HearthGuiInit
  Call HearthPreflight
  ${If} $R9 != ""
    Call HearthRefuse
    Quit
  ${EndIf}
FunctionEnd
!define MUI_CUSTOMFUNCTION_GUIINIT HearthGuiInit

Section "-HearthPreflight"
  Call HearthPreflight
  ${If} $R9 != ""
    Call HearthRefuse
    Abort
  ${EndIf}
SectionEnd

; BR-CLIENT-009 et BR-CLIENT-010 : l'arrêt propre de l'application en cours, le
; retrait de l'entrée de démarrage et la case « Tout effacer » sont assurés par
; le modèle NSIS de Tauri (voir ADR-0010) ; rien à ajouter ici.
