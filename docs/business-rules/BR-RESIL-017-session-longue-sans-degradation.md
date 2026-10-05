---
id: BR-RESIL-017
domaine: RESIL
titre: Une session de plusieurs jours, serveur éteint puis rallumé, ne consomme pas de ressources en plus
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-lien-resilient.md (BR-RESIL-017)
maj: 2026-10-05
---

# BR-RESIL-017 — Une session de plusieurs jours, serveur éteint puis rallumé, ne consomme pas de ressources en plus

## Règle
Tout ce que la bibliothèque retient est borné : la dernière vue (300 échantillons, `HISTORY_CAP`), les opérations en suspens (256, `MAX_PENDING`, abandon après 24 h), le délai entre deux tentatives (30 s au plus), le fichier des opérations en suspens (même borne), le canal d'événements (1 024 : un abonné lent reçoit `Event::Lagged` et relit `states()`), un message de flux (4 Mio). Un serveur hors ligne depuis des jours reçoit une tentative toutes les 30 s environ, avec un seul événement par tentative ; chaque cycle éteint puis rallumé est une coupure comme une autre.

## Application (code)
- `crates/hearth-link/src/domain/server.rs::HISTORY_CAP`, `domain/pending_ops.rs::{MAX_PENDING, ABANDON_AFTER}`, `domain/backoff.rs::MAX_DELAY`.
- `crates/hearth-link/src/manager/events.rs` (canal borné), `manager/task.rs::display_changed` (un événement par tentative).

## Vérification
- Tests : `crates/hearth-link/tests/robustness.rs::a_server_that_always_fails_keeps_being_retried_with_bounded_state` (deux jours de coupure en temps virtuel), `::random_errors_and_malformed_answers_never_panic_nor_block_seed_1` (et `_2`, `_3` : dernière vue bornée, aucune tâche relancée) ; `domain::pending_ops::tests::the_number_of_tracked_operations_is_bounded`.

## Cas limites
- Un `snapshot` de plusieurs milliers d'échantillons est ramené à `HISTORY_CAP`.
- Mesure de consommation réelle sur plusieurs jours : à faire avec l'application (HRT-08 et suivants).

## Règles liées
- BR-RESIL-005.

## Historique
- 2026-10-05 — création (HRT-07, session 2026-10-04-hearth-creation).
- 2026-10-05 — précisé (HRT-07, review Stephen round 1).
