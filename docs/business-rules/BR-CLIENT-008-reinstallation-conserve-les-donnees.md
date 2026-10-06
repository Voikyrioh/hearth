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
Relancer l'installateur remplace les fichiers de l'application mais ne touche ni au dossier de données (`%APPDATA%\fr.voikyrioh.hearth`) ni à l'entrée de démarrage (la page d'installation du démarrage de Windows, BR-CLIENT-006, n'existe pas en mise à jour automatique ni en silencieux : le choix déjà fait dans l'application n'est jamais écrasé). L'application n'est pas relancée automatiquement lors d'une mise à jour.

## Application (code)
- Comportement du modèle NSIS de Tauri (aucun fichier de données n'est écrit ni supprimé hors désinstallation avec case cochée) ; aucune fonction `domain/` : règle d'empaquetage.

## Vérification
- À la main : réglages modifiés, réinstaller, les retrouver.

## Cas limites
- Les données vivent hors du dossier d'installation, donc hors de portée d'un remplacement de fichiers.

## Règles liées
- BR-CLIENT-009

## Historique
- 2026-10-04 — création (HRT-08, session 2026-10-04-hearth-creation).
