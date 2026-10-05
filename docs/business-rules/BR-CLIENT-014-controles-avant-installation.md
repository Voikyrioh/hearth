---
id: BR-CLIENT-014
domaine: CLIENT
titre: Windows 10 64 bits et 50 Mo libres sont vérifiés avant toute écriture
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-installer-client.md (BR-CLIENT-014), HRT-08
maj: 2026-10-04
---

# BR-CLIENT-014 — Windows 10 64 bits et 50 Mo libres sont vérifiés avant toute écriture

## Règle
L'installateur vérifie Windows 10 ou plus récent en 64 bits (« Hearth a besoin de Windows 10 ou plus récent, en 64 bits. Ta version ne fonctionne pas. ») et au moins 50 Mo libres sur le disque d'installation (« Il manque de la place. Libère au moins 50 Mo. Espace disponible : X Mo. »). Un refus annule l'installation.

**Garanti** : un refus par l'un de ces deux contrôles ne laisse rien sur la machine : ni dossier d'installation, ni WebView2 installé, ni entrée de registre. Relancer l'installateur fonctionne normalement. Le mode silencieux refuse aussi (code de sortie 2).

**Non garanti** :
- une coupure ou une erreur d'écriture pendant la copie des fichiers : NSIS n'est pas transactionnel, les fichiers déjà copiés restent (un nouveau lancement les remplace) ;
- si l'utilisateur choisit « désinstaller d'abord » l'ancienne version sur la page de réinstallation puis change de dossier vers un disque presque plein, l'ancienne version est déjà retirée au moment du second contrôle.

## Application (code)
- `apps/desktop/src-tauri/installer/hooks.nsh` : `HearthPreflight` (`AtLeastWin10`, `RunningX64`, `DriveSpace`), appelée par `HearthGuiInit` (`.onGUIInit`, avant toute page) et par la section masquée `-HearthPreflight`, placée avant les sections du modèle Tauri (WebView2, copie). Le modèle n'offre pas de crochet plus tôt : voir l'en-tête du fichier.
- Messages : `apps/desktop/src-tauri/installer/French.nsh` (`hearthWindowsTooOld`, `hearthDiskTooSmall`).

## Vérification
- Testé en réel (voir HRT-08, PR #6) : installateur construit avec un seuil inatteignable, `/S /D=...` : code de sortie 2, aucun dossier, aucune clé `Uninstall`, aucun `MicrosoftEdgeWebview2Setup.exe` dans `%TEMP%`. Seuil rétabli à 50 Mo ensuite.
- Non automatisé en CI (aucun moyen de simuler un disque presque plein sans droits).

## Cas limites
- Mode silencieux : la boîte de message est ignorée (`/SD IDOK`), l'installation est annulée.

## Règles liées
- BR-CLIENT-001

## Historique
- 2026-10-04 — création (HRT-08, session 2026-10-04-hearth-creation).
- 2026-10-04 — revue Stephen : contrôles déplacés avant toute écriture, garanties et limites dites exactement.
