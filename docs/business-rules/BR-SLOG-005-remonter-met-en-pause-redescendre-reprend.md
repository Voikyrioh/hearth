---
id: BR-SLOG-005
domaine: SLOG
titre: Remonter dans les lignes met le direct en pause ; redescendre tout en bas le reprend
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-journaux-service.md (BR-SLOG-005), HRT-67
maj: 2026-10-14
---

# BR-SLOG-005 : pause automatique

## Règle
Remonter dans les lignes (molette, barre de défilement, clavier) met le direct en pause automatiquement, avec le même compteur ; le bouton « Revenir au direct » ramène en bas et est le seul bouton principal de la vue, visible seulement pendant une pause. Redescendre tout en bas (à 24 px près) reprend le direct sans clic quand la pause est automatique ; une pause demandée par le bouton ne se lève que par lui. Descendre ne pose jamais de pause.

## Application (code)
- HORS DOMAIN : `apps/desktop/src/components/organisms/LogViewer.vue::onScroll`, `apps/desktop/src/logs/windowing.ts::{atBottom, BOTTOM_PX}`, `apps/desktop/src/stores/serviceLogs.ts` (mode `live`, `auto`, `manual`).

## Vérification
- `apps/desktop/e2e/hrt67-logs.spec.ts` (parcours : remonter, figé, compteur, retour au direct)

## Règles liées
- BR-SLOG-004

## Historique
- 2026-10-14 : fiche créée avec l'onglet Journaux du client (HRT-67).
