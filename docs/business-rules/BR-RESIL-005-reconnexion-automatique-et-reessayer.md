---
id: BR-RESIL-005
domaine: RESIL
titre: Reconnexion automatique sans fin, avec des délais de 0,5 s à 30 s, et « Réessayer maintenant » relance tout de suite
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-lien-resilient.md (BR-RESIL-005), ADR-0007, HRT-09
maj: 2026-10-05
---

# BR-RESIL-005 — Reconnexion automatique sans fin, avec des délais de 0,5 s à 30 s, et « Réessayer maintenant » relance tout de suite

## Règle
Après une coupure, la première tentative part tout de suite ; les suivantes sont espacées de 0,5 s, 1 s, 2 s, 4 s, 8 s, 15 s, 30 s, puis 30 s indéfiniment, avec ± 20 % d'aléa (source d'aléa injectée). Le plafond de 30 s est dur : l'aléa ne le dépasse jamais. Un succès remet la suite à 0,5 s. Il n'y a ni nombre maximal de tentatives ni disjoncteur. « Réessayer maintenant », un réveil ou un changement de réseau lancent une tentative immédiate sans remettre la suite à zéro.

Part de l'interface : le bouton du bandeau hors ligne appelle `LinkBridge.retryNow(serverId)` ; l'état passe à « Reconnexion… » puis « Connecté » ou revient à « Hors ligne », sans modale. Part hors interface : le calcul des intervalles et la tentative elle-même vivent dans `hearth-link` (ADR-0007, machine à états du lien), pas dans le client web.

## Application (code)
- Bibliothèque :
  - `crates/hearth-link/src/domain/backoff.rs::Backoff::{next_delay, reset}` et `::jittered`.
  - `crates/hearth-link/src/domain/state.rs::LinkMachine::on_transport_failed` (planifie la prochaine tentative), `::on_tick` (la lance).
- Interface :
  - `apps/desktop/src/link/bridge.ts::LinkBridge.retryNow` (contrat) ; `apps/desktop/src/stores/link.ts::retryNow` ; `apps/desktop/src/components/organisms/OfflineBanner.vue` (bouton, événement `retry`) ; `apps/desktop/src/layouts/ServerLayout.vue` (branche `retry` au store).
  - Implémentation simulée : `apps/desktop/src/link/simulated.ts::SimulatedLinkBridge.retryNow`. Fait : le pont réel `TauriLinkBridge::retryNow` appelle la commande `retry_now` de la coquille, qui appelle `LinkManager::retry_now` (HRT-10) ; les intervalles sont ceux de la bibliothèque.

## Vérification
- Bibliothèque : `domain::backoff::tests` (suite, remise à zéro, aléa borné à ± 20 %, plafond), `domain::state::tests::the_first_attempt_is_immediate_then_delays_follow_the_sequence`, `::a_success_restarts_the_delays_from_half_a_second`, `::row08_server_dead_for_ten_minutes_stays_offline_and_keeps_trying`, `::jitter_is_applied_and_bounded_by_the_cap`, `::a_trigger_while_an_attempt_is_running_restarts_it`.
- Interface : `link-stores.test.ts::goes reconnecting on « Réessayer maintenant », then back to connected`, `::asks the bridge to retry now`, `shell.test.ts::« Réessayer maintenant » asks the bridge to retry that server`, `shell.test.ts::a rejecting action (retryNow) leaves the shell and the page intact`, `e2e/shell.spec.ts` (« connecté, reconnexion, hors ligne puis retour »).

## Cas limites
- Un échec rapporté alors qu'aucune tentative n'est en cours est ignoré : il n'avance pas la suite des délais.
- Le compteur d'échecs sature, il ne déborde pas.
- Si `retryNow` rejette (liaison en panne) : notification discrète et journal, la coquille et la page restent affichées (BR-RESIL-011).

## Règles liées
- BR-RESIL-003, BR-RESIL-004, BR-RESIL-006, BR-RESIL-011.

## Historique
- 2026-10-05 — création (HRT-07, session 2026-10-04-hearth-creation).
- 2026-10-05 — création de la partie interface (HRT-09, revue Stephen round 1 : références sans fiche). Portée par l'interface pour ce qui la concerne ; la reconnexion elle-même est dans `hearth-link` (ADR-0007).
- 2026-10-05 — fiches HRT-07 et HRT-09 réunies (fusion de main dans feat/HRT-07-link).
- 2026-10-05 : note « reste à faire » soldée (HRT-12).
