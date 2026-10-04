---
id: BR-CLIENT-009
domaine: CLIENT
titre: La désinstallation propose de garder ou d'effacer serveurs et mots de passe
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-installer-client.md (BR-CLIENT-009), HRT-08
maj: 2026-10-04
---

# BR-CLIENT-009 — La désinstallation propose de garder ou d'effacer serveurs et mots de passe

## Règle
La page de désinstallation porte une case « Tout effacer : supprimer aussi mes serveurs enregistrés et mes mots de passe mémorisés », décochée par défaut (= tout garder). Cochée, le dossier de données de l'application est supprimé.

## Application (code)
- `apps/desktop/src-tauri/installer/French.nsh` : texte `deleteAppData` ; la case et la suppression sont celles du modèle NSIS de Tauri.
- Écart assumé : le dialogue exact de la spec (deux boutons « Garder mes serveurs » / « Tout effacer ») demande une page NSIS sur mesure ; non réalisé, voir ADR-0010. Les mots de passe du Gestionnaire d'identification Windows seront retirés avec les serveurs quand ils existeront (HRT à venir).

## Vérification
- À la main : désinstaller case décochée puis cochée, vérifier `%APPDATA%\fr.voikyrioh.hearth`.

## Cas limites
- Mode silencieux ou mise à jour : jamais d'effacement.

## Règles liées
- BR-CLIENT-008
- BR-CLIENT-010

## Historique
- 2026-10-04 — création (HRT-08, session 2026-10-04-hearth-creation).
