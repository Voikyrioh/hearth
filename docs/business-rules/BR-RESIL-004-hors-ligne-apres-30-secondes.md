---
id: BR-RESIL-004
domaine: RESIL
titre: Au-delà de 30 secondes de coupure, l'état du lien est « Hors ligne »
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-lien-resilient.md (BR-RESIL-004), ADR-0007
maj: 2026-10-05
---

# BR-RESIL-004 — Au-delà de 30 secondes de coupure, l'état du lien est « Hors ligne »

## Règle
À partir de 30 s de coupure, l'état passe à `Offline` ; les tentatives continuent sans fin. L'événement d'état porte l'heure du dernier contact (`last_contact_at`) pour le bandeau « Dernier contact à {heure} » et la date de la prochaine tentative (`next_retry_at`). « Réessayer maintenant » depuis `Offline` affiche `Reconnecting` le temps de la tentative, puis revient à `Offline` si elle échoue.

L'affichage du bandeau et du bouton relève de l'interface (à venir, HRT-12) ; la bibliothèque fournit l'état et les dates.

## Application (code)
- `crates/hearth-link/src/domain/state.rs::LinkMachine::derive_down` (`LinkState::Offline`), `LinkMachine::status` (`Status::last_contact`, `Status::next_retry_at`), `LinkMachine::on_trigger` (`Input::RetryNow`).

## Vérification
- Tests : `domain::state::tests::row03_connected_cut_over_30s_shows_offline` (29 999 / 30 000 ms), `::row05_reconnecting_with_no_answer_for_30s_goes_offline`, `::row06_offline_retry_now_shows_reconnecting_and_starts_an_attempt`, `::row07_offline_link_back_automatically_is_connected`, `::a_late_tick_dates_the_state_at_the_threshold_not_at_the_late_instant`.
- Intégration : `tests/fault_proxy.rs::a_long_cut_goes_offline_then_comes_back`, `::retry_now_forces_an_attempt_without_waiting`.

## Cas limites
- Une tentative lancée par « Réessayer maintenant » qui échoue remet `Offline` avec la date d'origine (`since` inchangé : le serveur est resté hors ligne sans interruption).
- L'empreinte changée et les versions incompatibles s'affichent `Offline` avec `blocked` renseigné et aucune tentative planifiée (voir BR-CONN-003).

## Règles liées
- BR-RESIL-005.

## Historique
- 2026-10-05 — création (HRT-07, session 2026-10-04-hearth-creation).
