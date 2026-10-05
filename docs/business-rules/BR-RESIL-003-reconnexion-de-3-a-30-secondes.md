---
id: BR-RESIL-003
domaine: RESIL
titre: De 3 à 30 secondes de coupure, l'état du lien est « Reconnexion en cours »
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-lien-resilient.md (BR-RESIL-003), ADR-0007
maj: 2026-10-05
---

# BR-RESIL-003 — De 3 à 30 secondes de coupure, l'état du lien est « Reconnexion en cours »

## Règle
Entre 3 s (inclus) et 30 s (exclu) de coupure, l'état passe à `Reconnecting`, sans fenêtre modale ni notification intrusive. Si le lien revient avant 30 s, l'état repasse à `Connected` et les données se remettent à jour.

## Application (code)
- `crates/hearth-link/src/domain/state.rs::LinkMachine::derive_down` (`LinkState::Reconnecting`), `LinkMachine::handle` (`Input::Connected`).

## Vérification
- Tests : `domain::state::tests::row02_connected_cut_between_3s_and_30s_shows_reconnecting`, `::row04_reconnecting_then_link_back_before_30s_is_connected`, `::row15_agent_alone_restarting_recovers_in_a_few_seconds`.
- Intégration : `tests/fault_proxy.rs::a_ten_second_cut_shows_reconnecting_then_connected`, `::an_agent_restart_is_a_short_reconnecting`.

## Cas limites
- Le texte « Reconnexion en cours » et son habillage relèvent de l'interface (à venir, HRT-12).

## Règles liées
- BR-RESIL-002, BR-RESIL-004.

## Historique
- 2026-10-05 — création (HRT-07, session 2026-10-04-hearth-creation).
