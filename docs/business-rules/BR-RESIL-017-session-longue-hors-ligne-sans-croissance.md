---
id: BR-RESIL-017
domaine: RESIL
titre: Une session de plusieurs jours hors ligne, serveur éteint puis rallumé, ne fait pas grossir l'interface ni la bibliothèque
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-lien-resilient.md (BR-RESIL-017), HRT-09
maj: 2026-10-05
---

# BR-RESIL-017 — Une session de plusieurs jours hors ligne, serveur éteint puis rallumé, ne fait pas grossir l'interface ni la bibliothèque

## Règle
Pendant des jours hors ligne, l'interface ne plante pas et ne consomme pas de plus en plus de mémoire ni de processeur.

Part de la bibliothèque : tout ce qu'elle retient est borné : la dernière vue (300 échantillons, `HISTORY_CAP`), les opérations en suspens (256, `MAX_PENDING`, abandon après 24 h), le délai entre deux tentatives (30 s au plus), le fichier des opérations en suspens (même borne), le canal d'événements (1 024 : un abonné lent reçoit `Event::Lagged` et relit `states()`), un message de flux (4 Mio). Un serveur hors ligne depuis des jours reçoit une tentative toutes les 30 s environ, avec un seul événement par tentative ; chaque cycle éteint puis rallumé est une coupure comme une autre.

Part de l'interface : un seul minuteur d'une seconde pour tout l'affichage de l'âge des données, arrêté quand plus rien ne l'utilise ; la file de notifications est plafonnée à 50 et une notification répétée devient un compteur ; les issues d'opération gardées sont plafonnées (100 entrées, 10 minutes) ; un état de lien par serveur (pas d'historique) ; les journaux d'erreurs de l'interface sont limités en débit.

## Application (code)
- Bibliothèque :
  - `crates/hearth-link/src/domain/server.rs::HISTORY_CAP`, `domain/pending_ops.rs::{MAX_PENDING, ABANDON_AFTER}`, `domain/backoff.rs::MAX_DELAY`.
  - `crates/hearth-link/src/manager/events.rs` (canal borné), `manager/task.rs::display_changed` (un événement par tentative).
- Interface :
  - `apps/desktop/src/composables/useNow.ts` (minuteur partagé) ; `apps/desktop/src/stores/toasts.ts` (`MAX_QUEUED_TOASTS`, compteur) ; `apps/desktop/src/stores/link.ts` (`MAX_OPERATIONS`, `OPERATION_MAX_AGE_MS`, un événement par serveur) ; `apps/desktop/src/errors/report.ts` (débit) et `src-tauri/src/domain.rs::FrontendErrorLimiter`.

## Vérification
- Bibliothèque : `crates/hearth-link/tests/robustness.rs::a_server_that_always_fails_keeps_being_retried_with_bounded_state` (deux jours de coupure en temps virtuel), `::random_errors_and_malformed_answers_never_panic_nor_block_seed_1` (et `_2`, `_3` : dernière vue bornée, aucune tâche relancée) ; `domain::pending_ops::tests::the_number_of_tracked_operations_is_bounded`.
- Interface : `link-stores.test.ts::can be dismissed by hand and never grows without bound`, `::keeps the outcome by opId ... bounded in count and age`, `errors.test.ts::bounds the rate`, `molecules.test.ts::StaleStamp` (mise à jour en direct puis démontage), `src-tauri/tests/domain.rs::frontend_errors_are_rate_limited_per_window`.

## Cas limites
- Un `snapshot` de plusieurs milliers d'échantillons est ramené à `HISTORY_CAP`.
- Une page montée longtemps hors ligne continue de mettre à jour son « Vu il y a… » à la seconde, sans accumuler de minuteurs.
- Mesure de consommation réelle sur plusieurs jours : à faire avec l'application (HRT-08 et suivants).

## Règles liées
- BR-RESIL-005, BR-RESIL-007, BR-RESIL-018.

## Historique
- 2026-10-05 — création (HRT-07, session 2026-10-04-hearth-creation).
- 2026-10-05 — précisé (HRT-07, review Stephen round 1).
- 2026-10-05 — création de la partie interface (HRT-09, revue Stephen round 1 : références sans fiche). Portée par l'interface pour ce qui la concerne ; la reconnexion elle-même est dans `hearth-link` (ADR-0007).
- 2026-10-05 — fiches HRT-07 et HRT-09 réunies (fusion de main dans feat/HRT-07-link).
