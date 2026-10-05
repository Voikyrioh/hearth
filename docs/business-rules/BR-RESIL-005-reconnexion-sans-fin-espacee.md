---
id: BR-RESIL-005
domaine: RESIL
titre: Le client se reconnecte sans fin, avec des délais de 0,5 s à 30 s
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-lien-resilient.md (BR-RESIL-005), ADR-0007
maj: 2026-10-05
---

# BR-RESIL-005 — Le client se reconnecte sans fin, avec des délais de 0,5 s à 30 s

## Règle
Après une coupure, la première tentative part tout de suite ; les suivantes sont espacées de 0,5 s, 1 s, 2 s, 4 s, 8 s, 15 s, 30 s, puis 30 s indéfiniment, avec ± 20 % d'aléa (source d'aléa injectée). Le plafond de 30 s est dur : l'aléa ne le dépasse jamais. Un succès remet la suite à 0,5 s. Il n'y a ni nombre maximal de tentatives ni disjoncteur. « Réessayer maintenant », un réveil ou un changement de réseau lancent une tentative immédiate sans remettre la suite à zéro.

## Application (code)
- `crates/hearth-link/src/domain/backoff.rs::Backoff::{next_delay, reset}` et `::jittered`.
- `crates/hearth-link/src/domain/state.rs::LinkMachine::on_transport_failed` (planifie la prochaine tentative), `::on_tick` (la lance).

## Vérification
- Tests : `domain::backoff::tests` (suite, remise à zéro, aléa borné à ± 20 %, plafond), `domain::state::tests::the_first_attempt_is_immediate_then_delays_follow_the_sequence`, `::a_success_restarts_the_delays_from_half_a_second`, `::row08_server_dead_for_ten_minutes_stays_offline_and_keeps_trying`, `::jitter_is_applied_and_bounded_by_the_cap`, `::a_trigger_while_an_attempt_is_running_restarts_it`.

## Cas limites
- Un échec rapporté alors qu'aucune tentative n'est en cours est ignoré : il n'avance pas la suite des délais.
- Le compteur d'échecs sature, il ne déborde pas.

## Règles liées
- BR-RESIL-004, BR-RESIL-006.

## Historique
- 2026-10-05 — création (HRT-07, session 2026-10-04-hearth-creation).
