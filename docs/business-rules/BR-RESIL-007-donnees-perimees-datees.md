---
id: BR-RESIL-007
domaine: RESIL
titre: Hors « Connecté », les dernières données restent affichées, désaturées et datées
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-lien-resilient.md (BR-RESIL-007), HRT-09
maj: 2026-10-05
---

# BR-RESIL-007 — Hors « Connecté », les dernières données restent affichées, désaturées et datées

## Règle
Tant que le lien n'est pas « Connecté » (reconnexion, hors ligne, session expirée, accès révoqué), les vues gardent les dernières données connues, désaturées (`grayscale(.85)`, opacité .62), avec la mention « Vu il y a 12 s » / « Vu il y a 2 min » / « Vu il y a 3 h » / « Vu il y a 2 j » mise à jour en direct. Un seul mécanisme : `StaleSurface`. Les données ne sont jamais retirées.

## Application (code)
- `apps/desktop/src/components/molecules/StaleSurface.vue` (désaturation, `data-stale`) et `StaleStamp.vue` (âge) ; `apps/desktop/src/composables/format.ts::formatSeen` ; horloge partagée `composables/useNow.ts`.
- Utilisé par `ComingSoonPanel` (Dashboard, Accounts, Audit), puis par les vraies cartes des tickets suivants.

## Vérification
- Tests : `molecules.test.ts::StaleStamp`, `::StaleSurface`, `format.test.ts::formatSeen`, `shell.test.ts::goes connected -> reconnecting -> offline -> back`, `e2e/shell.spec.ts`.

## Cas limites
- Horloge reculée : l'âge ne devient jamais négatif (« Vu il y a 0 s »).
- Aucun contact connu : « Jamais vu ».
- Un seul minuteur d'une seconde pour toute l'interface, arrêté quand plus rien ne l'utilise (BR-RESIL-017).

## Règles liées
- BR-RESIL-001, BR-RESIL-008, BR-DASH-009.

## Historique
- 2026-10-05 — création (HRT-09, session 2026-10-04-hearth-creation). Portée par l'interface ; le calcul des états est dans `hearth-link` (ADR-0007).
