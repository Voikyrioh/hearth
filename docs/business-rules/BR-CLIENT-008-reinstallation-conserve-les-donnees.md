---
id: BR-CLIENT-008
domaine: CLIENT
titre: Réinstaller par-dessus une version existante conserve serveurs et réglages
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-installer-client.md (BR-CLIENT-008), HRT-08
maj: 2026-10-07
---

# BR-CLIENT-008 — Réinstaller par-dessus une version existante conserve serveurs et réglages

## Règle
Relancer l'installateur remplace les fichiers de l'application mais ne touche pas au dossier de données (`%APPDATA%\fr.voikyrioh.hearth`). L'entrée de démarrage n'est touchée que par la case de la page d'accueil (BR-CLIENT-006), et cette page n'existe ni en silencieux (`/S`) ni en mise à jour automatique (`/UPDATE /P`) : dans ces deux cas le choix déjà fait dans l'application n'est jamais écrasé. Seule exception, voulue : à la main, avec l'interface, la case arrive dans l'état actuel du démarrage et l'utilisateur peut le changer (décocher retire l'entrée). L'application n'est pas relancée automatiquement lors d'une mise à jour.

## Application (code)
- Comportement du modèle NSIS de Tauri (aucun fichier de données n'est écrit ni supprimé hors désinstallation avec case cochée) ; aucune fonction `domain/` : règle d'empaquetage.

## Vérification
- À la main : réglages modifiés, réinstaller, les retrouver.

## Cas limites
- Les données vivent hors du dossier d'installation, donc hors de portée d'un remplacement de fichiers.

## Règles liées
- BR-CLIENT-009

## Dossier d'installation proposé (HRT-47, S6, FIX-01M4EPVMJGC21SAKM7YBP4S7S9)
Sans /D, l'installateur propose le dossier mémorisé sous `HKCU\Software\Voikyrioh\Hearth` seulement s'il existe et contient l'exécutable (`hearth-desktop.exe`) ; sinon le dossier par défaut (`%LOCALAPPDATA%\Hearth`). Code : `apps/desktop/src-tauri/installer/hooks.nsh::HearthRememberedDir` (écran graphique et installation silencieuse). Preuve : `scripts/installer-ci.ps1`, cas (f), et `tests/installer.rs` (lecture du script).

## Historique
- 2026-10-04 — création (HRT-08, session 2026-10-04-hearth-creation).
