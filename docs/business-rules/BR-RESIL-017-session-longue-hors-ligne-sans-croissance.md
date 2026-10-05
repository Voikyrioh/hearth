---
id: BR-RESIL-017
domaine: RESIL
titre: Une longue session hors ligne ne fait pas grossir l'interface
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-lien-resilient.md (BR-RESIL-017), HRT-09
maj: 2026-10-05
---

# BR-RESIL-017 — Une longue session hors ligne ne fait pas grossir l'interface

## Règle
Pendant des jours hors ligne, l'interface ne plante pas et ne consomme pas de plus en plus de mémoire ni de processeur. **Part de l'interface** : un seul minuteur d'une seconde pour tout l'affichage de l'âge des données, arrêté quand plus rien ne l'utilise ; la file de notifications est plafonnée à 50 et une notification répétée devient un compteur ; les issues d'opération gardées sont plafonnées (100 entrées, 10 minutes) ; un état de lien par serveur (pas d'historique) ; les journaux d'erreurs de l'interface sont limités en débit. **Part hors interface** : mémoire et tâches de reconnexion dans `hearth-link`.

## Application (code)
- `apps/desktop/src/composables/useNow.ts` (minuteur partagé) ; `apps/desktop/src/stores/toasts.ts` (`MAX_QUEUED_TOASTS`, compteur) ; `apps/desktop/src/stores/link.ts` (`MAX_OPERATIONS`, `OPERATION_MAX_AGE_MS`, un événement par serveur) ; `apps/desktop/src/errors/report.ts` (débit) et `src-tauri/src/domain.rs::FrontendErrorLimiter`.

## Vérification
- Tests : `link-stores.test.ts::can be dismissed by hand and never grows without bound`, `::keeps the outcome by opId ... bounded in count and age`, `errors.test.ts::bounds the rate`, `molecules.test.ts::StaleStamp` (mise à jour en direct puis démontage), `src-tauri/tests/domain.rs::frontend_errors_are_rate_limited_per_window`.

## Cas limites
- Une page montée longtemps hors ligne continue de mettre à jour son « Vu il y a… » à la seconde, sans accumuler de minuteurs.

## Règles liées
- BR-RESIL-007, BR-RESIL-018.

## Historique
- 2026-10-05 — création (HRT-09, revue Stephen round 1 : références sans fiche). Portée par l'interface pour ce qui la concerne ; la reconnexion elle-même est dans `hearth-link` (ADR-0007).
