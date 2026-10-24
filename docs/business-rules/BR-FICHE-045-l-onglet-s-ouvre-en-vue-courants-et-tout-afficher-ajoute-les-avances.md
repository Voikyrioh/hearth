---
id: BR-FICHE-045
domaine: FICHE
titre: L'onglet s'ouvre en vue « Courants » ; « Tout afficher » ajoute les réglages avancés et le choix est retenu sur le poste
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-045), HRT-109
maj: 2026-10-24
---

# BR-FICHE-045

## Règle
La vue « Courants » montre les réglages essentiels et courants ; « Tout afficher » ajoute les avancés. Le choix est retenu pour le poste (préférence d'affichage, pas un réglage du serveur) ; la recherche cherche toujours dans tous les niveaux. Un niveau que ce client ne connaît pas est traité comme avancé.

## Application (code)
- `apps/desktop/src/settings/filter.ts::{isCommon, shown}`, `apps/desktop/src/composables/useGameSettings.ts` (clé `hearth.gameSettings.scope`, lecture et écriture protégées)

## Vérification
- `apps/desktop/src/settings/settings.test.ts::chercher, filtrer, ranger (BR-FICHE-043, 045)`

## Cas limites
- Un stockage local indisponible (fenêtre privée, données bloquées) ne casse rien : la vue n'est simplement pas retenue.

## Règles liées
BR-FICHE-041, BR-FICHE-043

## Historique
- 2026-10-24 — création (HRT-109).
