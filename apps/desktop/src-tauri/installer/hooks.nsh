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

; Lancement au démarrage de Windows (HRT-21, BR-CLIENT-006). Une seule source de
; vérité : l'entrée `Run` de l'utilisateur que le greffon autostart de l'application
; lit et écrit (nom de valeur = nom du produit, argument --minimized). L'installateur
; n'ajoute aucun réglage à lui : il écrit ou retire cette entrée.
; Voie retenue (ADR-0026) : le modèle de Tauri n'accepte pas de page sur mesure
; (les crochets sont inclus AVANT les pages) ; la case est donc ajoutée à la page
; d'accueil par ses rappels SHOW et LEAVE, comme le modèle le fait pour la case de
; la désinstallation. Cette page n'existe ni en silencieux (/S) ni en mode passif
; (mise à jour automatique, /UPDATE /P) : alors RIEN n'est écrit, l'entrée déjà
; présente (ou absente) reste telle que l'utilisateur l'a réglée dans l'application.
; PRODUCTNAME n'est défini qu'après ce fichier : le nom est répété ici et un garde
; de compilation (dans le crochet POSTINSTALL) le compare au vrai nom du produit.
!define HEARTH_RUN_KEY "Software\Microsoft\Windows\CurrentVersion\Run"
!define HEARTH_STARTUP_APPROVED_KEY "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run"
!define HEARTH_RUN_VALUE "Hearth"
; Doit rester celui de domain::MINIMIZED_FLAG (tests/installer.rs le vérifie).
!define HEARTH_MINIMIZED_FLAG "--minimized"

!include "LogicLib.nsh"
!include "nsDialogs.nsh"

Var HearthAutostartWas
Var HearthAutostartBox
Var HearthAutostartShown
Var HearthAutostartWanted

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
  ; Entrée de démarrage déjà présente ? Lue avant toute page : la page de
  ; réinstallation peut désinstaller l'ancienne version (et son entrée) ensuite.
  StrCpy $HearthAutostartWas 0
  ClearErrors
  ReadRegStr $R0 HKCU "${HEARTH_RUN_KEY}" "${HEARTH_RUN_VALUE}"
  ${IfNot} ${Errors}
    StrCpy $HearthAutostartWas 1
  ${EndIf}
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

; Page d'accueil : la case, décochée sauf si l'entrée existe déjà (réinstallation
; manuelle : on propose l'état actuel, on ne le change pas dans le dos de l'utilisateur).
; Retour en arrière puis avance : le choix déjà fait est conservé.
Function HearthWelcomeShow
  ${NSD_CreateCheckbox} 120u 122u 195u 12u "$(hearthAutostartLabel)"
  Pop $HearthAutostartBox
  SetCtlColors $HearthAutostartBox "000000" "FFFFFF"
  ${If} $HearthAutostartShown = 1
    ${If} $HearthAutostartWanted = 1
      ${NSD_Check} $HearthAutostartBox
    ${EndIf}
  ${ElseIf} $HearthAutostartWas = 1
    ${NSD_Check} $HearthAutostartBox
  ${EndIf}
  ${NSD_CreateLabel} 134u 136u 181u 40u "$(hearthAutostartHelp)"
  Pop $R0
  SetCtlColors $R0 "595959" "FFFFFF"
FunctionEnd

Function HearthWelcomeLeave
  ${NSD_GetState} $HearthAutostartBox $HearthAutostartWanted
  StrCpy $HearthAutostartShown 1
FunctionEnd

; Texte d'accueil court (tutoiement) : il laisse la place de la case sous lui.
!define MUI_WELCOMEPAGE_TEXT "$(hearthWelcomeText)"
!define MUI_PAGE_CUSTOMFUNCTION_SHOW HearthWelcomeShow
!define MUI_PAGE_CUSTOMFUNCTION_LEAVE HearthWelcomeLeave

; Appliqué à la fin de la copie des fichiers, seulement si la case a été vue.
!macro NSIS_HOOK_POSTINSTALL
  !if "${PRODUCTNAME}" != "${HEARTH_RUN_VALUE}"
    !error "HEARTH_RUN_VALUE ne correspond plus au nom du produit : l'entrée de démarrage ne serait plus celle de l'application"
  !endif
  ${If} $HearthAutostartShown = 1
    ${If} $HearthAutostartWanted = 1
      WriteRegStr HKCU "${HEARTH_RUN_KEY}" "${HEARTH_RUN_VALUE}" '"$INSTDIR\${MAINBINARYNAME}.exe" ${HEARTH_MINIMIZED_FLAG}'
      ; Même geste que le greffon : active dans le Gestionnaire des tâches.
      WriteRegBin HKCU "${HEARTH_STARTUP_APPROVED_KEY}" "${HEARTH_RUN_VALUE}" 020000000000000000000000
    ${Else}
      DeleteRegValue HKCU "${HEARTH_RUN_KEY}" "${HEARTH_RUN_VALUE}"
    ${EndIf}
  ${EndIf}
!macroend
