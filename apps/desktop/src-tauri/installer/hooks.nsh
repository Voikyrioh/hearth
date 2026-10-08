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
; vérité : l'entrée `Run` de l'utilisateur que l'application lit et écrit (startup.rs ;
; nom de valeur = nom du produit ; valeur `"chemin" --minimized`, chemin ENTRE GUILLEMETS :
; le chemin par défaut contient une espace dès que le profil Windows en contient une, HRT-29).
; L'installateur
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

; Dossier d'installation mémorisé (HRT-47, S6, FIX-01M4EPVMJGC21SAKM7YBP4S7S9). Le modèle de Tauri (mode utilisateur) propose,
; quand aucun /D n'est donné, le dossier lu sous `HKCU\Software\Voikyrioh\Hearth` sans vérifier qu'il existe encore ni
; qu'il contient Hearth (un dossier temporaire ou effacé était proposé). Ce dossier n'est gardé que s'il contient
; l'exécutable ; sinon le dossier par défaut du modèle (`$LOCALAPPDATA\Hearth`). Limite : un /D explicite qui désigne
; exactement le dossier mémorisé, vide, est aussi ramené au dossier par défaut (le script ne voit pas /D).
; Le nom de l'exécutable est répété ici (MAINBINARYNAME n'est défini qu'après ce fichier) : un garde de compilation
; (crochet POSTINSTALL) le compare au vrai.
!define HEARTH_PRODUCT_KEY "Software\Voikyrioh\Hearth"
!define HEARTH_MAIN_EXE "hearth-desktop.exe"
!define HEARTH_DEFAULT_DIR "$LOCALAPPDATA\Hearth"

Var HearthAutostartWas
Var HearthAutostartBox
Var HearthAutostartShown
Var HearthAutostartWanted

; Écarte le dossier mémorisé qui n'existe plus ou ne contient pas Hearth (S6). Appelée avant tout contrôle du dossier
; (écran graphique : avant la première page, jamais après : le choix de l'utilisateur gagne ; silencieux : section
; masquée ci-dessous, seule voie).
Function HearthRememberedDir
  ClearErrors
  ReadRegStr $R6 HKCU "${HEARTH_PRODUCT_KEY}" ""
  ${If} $R6 != ""
  ${AndIf} $R6 == $INSTDIR
    ${IfNot} ${FileExists} "$R6\${HEARTH_MAIN_EXE}"
      StrCpy $INSTDIR "${HEARTH_DEFAULT_DIR}"
    ${EndIf}
  ${EndIf}
FunctionEnd

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

; $R0 = 1 si le Gestionnaire des tâches n'a PAS désactivé l'entrée. Même lecture que
; auto-launch 0.6.0 (`is_task_manager_enabled`, ruche HKCU) : valeur absente, clé absente,
; ou moins de 8 octets = activée ; sinon activée seulement si les 8 derniers octets
; sont nuls (le premier octet 02 = activé, 03 = désactivé, suivi de la date).
Function HearthTaskManagerEnabled
  Push $0
  Push $1
  Push $2
  Push $3
  Push $4
  Push $5
  Push $6
  StrCpy $R0 1
  System::Call 'advapi32::RegOpenKeyExW(p 0x80000001, w "${HEARTH_STARTUP_APPROVED_KEY}", i 0, i 0x20019, *p .r1) i .r0'
  ${If} $0 = 0
    System::Call 'advapi32::RegQueryValueExW(p r1, w "${HEARTH_RUN_VALUE}", p 0, p 0, p 0, *i .r2) i .r0'
    ${If} $0 = 0
    ${AndIf} $2 >= 8
      System::Alloc $2
      Pop $3
      System::Call 'advapi32::RegQueryValueExW(p r1, w "${HEARTH_RUN_VALUE}", p 0, p 0, p r3, *i r2r2) i .r0'
      ${If} $0 = 0
        IntOp $4 $3 + $2
        IntOp $4 $4 - 8
        System::Call 'kernel32::RtlMoveMemory(*i .r5, p r4, i 4)'
        IntOp $4 $4 + 4
        System::Call 'kernel32::RtlMoveMemory(*i .r6, p r4, i 4)'
        ${If} $5 <> 0
        ${OrIf} $6 <> 0
          StrCpy $R0 0
        ${EndIf}
      ${EndIf}
      System::Free $3
    ${EndIf}
    System::Call 'advapi32::RegCloseKey(p r1)'
  ${EndIf}
  Pop $6
  Pop $5
  Pop $4
  Pop $3
  Pop $2
  Pop $1
  Pop $0
FunctionEnd

Function HearthGuiInit
  ; Démarrage déjà activé ? (entrée `Run` présente ET non désactivée dans le Gestionnaire
  ; des tâches, comme le lit le greffon.) Lu avant toute page : la page de réinstallation
  ; peut désinstaller l'ancienne version (et son entrée) ensuite.
  StrCpy $HearthAutostartWas 0
  ClearErrors
  ReadRegStr $R1 HKCU "${HEARTH_RUN_KEY}" "${HEARTH_RUN_VALUE}"
  ${IfNot} ${Errors}
    Call HearthTaskManagerEnabled
    ${If} $R0 = 1
      StrCpy $HearthAutostartWas 1
    ${EndIf}
  ${EndIf}
  Call HearthRememberedDir
  Call HearthPreflight
  ${If} $R9 != ""
    Call HearthRefuse
    Quit
  ${EndIf}
FunctionEnd
!define MUI_CUSTOMFUNCTION_GUIINIT HearthGuiInit

Section "-HearthPreflight"
  ; Le dossier mémorisé n'est qu'une valeur PROPOSÉE avant la page de choix : en écran graphique ce que
  ; l'utilisateur a choisi ou passé par /D gagne toujours (la page est déjà passée), donc seulement en silencieux.
  ${If} ${Silent}
    Call HearthRememberedDir
  ${EndIf}
  Call HearthPreflight
  ${If} $R9 != ""
    Call HearthRefuse
    Abort
  ${EndIf}
SectionEnd

; BR-CLIENT-009 et BR-CLIENT-010 : l'arrêt propre de l'application en cours, le
; retrait de l'entrée de démarrage et la case « Tout effacer » sont assurés par
; le modèle NSIS de Tauri (voir ADR-0010) ; rien à ajouter ici.

; Page d'accueil : la case, décochée sauf si le démarrage est déjà activé (réinstallation
; manuelle : on propose l'état actuel, on ne le change pas dans le dos de l'utilisateur).
; Retour en arrière puis avance : le choix déjà fait est conservé.
; Mise en page (unités de boîte de dialogue, donc la même à 100, 125 et 150 % d'échelle) :
; le contrôle de texte du modèle (120u 45u 195u 130u) est laissé VIDE (MUI_WELCOMEPAGE_TEXT
; ci-dessous) et le texte d'accueil est redessiné plus court, pour que rien ne se recouvre :
;   texte d'accueil   120u  55u 195u  55u  (jusqu'à 110u)
;   case              120u 118u 195u  12u  (130u)
;   aide              134u 134u 181u  50u  (jusqu'à 184u, la page fait 193u)
!searchparse /noerrors /file "${NSISDIR}\Contrib\Modern UI 2\Pages\Welcome.nsh" "Var mui.WelcomePage.Text" HEARTH_MUI_TEXT_VAR
!ifndef HEARTH_MUI_TEXT_VAR
  !error "MUI2 ne déclare plus mui.WelcomePage.Text : relire la page d'accueil de MUI2 (hooks.nsh, HearthWelcomeShow)"
!endif
!searchparse /noerrors /file "${NSISDIR}\Contrib\Modern UI 2\Pages\Welcome.nsh" "195u 130u" HEARTH_MUI_TEXT_BOX
!ifndef HEARTH_MUI_TEXT_BOX
  !error "MUI2 ne crée plus son texte d'accueil en 195u 130u : relire hooks.nsh (HearthWelcomeShow)"
!endif
Function HearthWelcomeShow
  ; nsDialogs insère chaque contrôle SOUS les précédents : le texte d'accueil du modèle (vidé,
  ; mais opaque et grand) recouvrait la case, que la capture de la CI montrait invisible. Il est
  ; donc masqué. MUI2 en garde la poignée dans $mui.WelcomePage.Text, mais cette variable n'est
  ; déclarée qu'à l'insertion de la page, APRÈS ce fichier (inutilisable ici) : on retrouve le
  ; contrôle à son texte (une seule espace, défini plus bas). Si MUI2 changeait, la compilation
  ; échoue (gardes ci-dessous) et la CI vérifie que la case n'est recouverte par rien.
  FindWindow $R1 "#32770" "" $HWNDPARENT
  FindWindow $R2 "Static" " " $R1
  ${If} $R2 <> 0
    ShowWindow $R2 0
  ${EndIf}
  ${NSD_CreateLabel} 120u 55u 195u 55u "$(hearthWelcomeText)"
  Pop $R0
  SetCtlColors $R0 "000000" "FFFFFF"
  ${NSD_CreateCheckbox} 120u 118u 195u 12u "$(hearthAutostartLabel)"
  Pop $HearthAutostartBox
  SetCtlColors $HearthAutostartBox "000000" "FFFFFF"
  ${If} $HearthAutostartShown = 1
    ${If} $HearthAutostartWanted = 1
      ${NSD_Check} $HearthAutostartBox
    ${EndIf}
  ${ElseIf} $HearthAutostartWas = 1
    ${NSD_Check} $HearthAutostartBox
  ${EndIf}
  ${NSD_CreateLabel} 134u 134u 181u 50u "$(hearthAutostartHelp)"
  Pop $R0
  SetCtlColors $R0 "595959" "FFFFFF"
FunctionEnd

Function HearthWelcomeLeave
  ${NSD_GetState} $HearthAutostartBox $HearthAutostartWanted
  StrCpy $HearthAutostartShown 1
FunctionEnd

!define MUI_WELCOMEPAGE_TEXT " "
; Page de fin en tutoiement (textes par défaut de MUI : vouvoiement). Consommé par la seule page de fin.
!define MUI_FINISHPAGE_TEXT "$(hearthFinishText)"
!define MUI_PAGE_CUSTOMFUNCTION_SHOW HearthWelcomeShow
!define MUI_PAGE_CUSTOMFUNCTION_LEAVE HearthWelcomeLeave

; Appliqué à la fin de la copie des fichiers, seulement si la case a été vue. Cochée : l'entrée
; est écrite comme le fait l'application (même valeur, chemin ENTRE GUILLEMETS puis --minimized, HRT-29,
; et « activé » dans le Gestionnaire des tâches). Décochée : retirée seulement si le démarrage était activé ; une entrée
; désactivée à la main dans le Gestionnaire des tâches n'est pas touchée.
!macro NSIS_HOOK_POSTINSTALL
  !if "${MAINBINARYNAME}.exe" != "${HEARTH_MAIN_EXE}"
    !error "HEARTH_MAIN_EXE ne correspond plus à l'exécutable du produit : le dossier mémorisé ne serait plus reconnu"
  !endif
  !if "${PRODUCTNAME}" != "${HEARTH_RUN_VALUE}"
    !error "HEARTH_RUN_VALUE ne correspond plus au nom du produit : l'entrée de démarrage ne serait plus celle de l'application"
  !endif
  ${If} $HearthAutostartShown = 1
    ${If} $HearthAutostartWanted = 1
      WriteRegStr HKCU "${HEARTH_RUN_KEY}" "${HEARTH_RUN_VALUE}" '"$INSTDIR\${MAINBINARYNAME}.exe" ${HEARTH_MINIMIZED_FLAG}'
      WriteRegBin HKCU "${HEARTH_STARTUP_APPROVED_KEY}" "${HEARTH_RUN_VALUE}" 020000000000000000000000
    ${ElseIf} $HearthAutostartWas = 1
      DeleteRegValue HKCU "${HEARTH_RUN_KEY}" "${HEARTH_RUN_VALUE}"
    ${EndIf}
  ${EndIf}
!macroend
