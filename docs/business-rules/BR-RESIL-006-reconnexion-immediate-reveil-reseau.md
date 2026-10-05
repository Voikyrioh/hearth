---
id: BR-RESIL-006
domaine: RESIL
titre: Au réveil du PC ou au changement de réseau, le client retente tout de suite
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-lien-resilient.md (BR-RESIL-006), technique-socle §9
maj: 2026-10-05
---

# BR-RESIL-006 — Au réveil du PC ou au changement de réseau, le client retente tout de suite

## Règle
Un saut d'horloge de plus de 5 s (réveil de veille) ou un changement de la liste des adresses réseau locales (sondée toutes les 5 s) déclenche une tentative immédiate, quels que soient les délais en cours. Au réveil, la coupure est comptée **à partir du réveil**, quel que soit l'état de départ (« Connecté », « Reconnexion en cours », « Hors ligne ») : l'état est « Reconnexion en cours » (tentative immédiate) et ne devient « Hors ligne » que si 30 s s'écoulent après le réveil sans succès (pas de bandeau hors ligne chaque matin). Le report est borné : un seul par coupure tant qu'aucun contact n'a réussi (un poste qui se réveille toutes les 20 s devant un serveur éteint finit « Hors ligne » ; test : `domain::state::tests::a_pc_waking_every_twenty_seconds_in_front_of_a_dead_server_ends_offline`). Un changement de réseau ne coupe **pas** un flux sain (Docker, WSL, Tailscale changent la liste d'adresses sans que le réseau utile bouge) : depuis « Connecté » il avance la vérification (ping immédiat, échéance de silence raccourcie à un tiers) et les actions en vol ne deviennent pas « inconnues » ; hors « Connecté » il lance une tentative immédiate.

## Application (code)
- `crates/hearth-link/src/domain/triggers.rs::{detect_wake, network_changed}`.
- `crates/hearth-link/src/domain/state.rs::LinkMachine::on_trigger` (`Input::Woke`, `Input::NetworkChanged`).
- Sondes : `crates/hearth-link/src/adapters/net_watch.rs`, `crates/hearth-link/src/manager/watchers.rs`.

## Vérification
- Tests : `domain::triggers::tests`, `domain::state::tests::row14_offline_wake_or_network_change_shows_reconnecting_and_attempts_at_once`, `::waking_while_connected_counts_the_outage_from_the_wake_up_not_from_the_last_message`, `::waking_while_offline_restarts_the_outage_clock_at_the_wake_up`, `::waking_while_reconnecting_attempts_at_once_and_restarts_the_clock`, `::a_network_change_while_connected_does_not_cut_a_healthy_stream`, `::a_network_change_with_no_answer_cuts_the_link_after_the_shortened_deadline`.
- Intégration : `tests/fault_proxy.rs::a_network_change_reconnects_immediately`, `::a_wake_up_reconnects_immediately`, `::waking_up_after_a_long_outage_shows_reconnecting_again_not_offline`, `::a_network_change_does_not_cut_a_healthy_stream`, `::an_action_in_flight_is_not_made_unknown_by_a_network_change`.

## Cas limites
- Une horloge murale réglée en arrière n'est pas un réveil.
- Après un blocage (empreinte, versions), seul « Réessayer maintenant » relance : un réveil ou un changement de réseau ne contournent pas le blocage.

## Règles liées
- BR-RESIL-005.

## Historique
- 2026-10-05 — création (HRT-07, session 2026-10-04-hearth-creation).
- 2026-10-05 — précisé (HRT-07, review Stephen round 1).
- Un réveil déjà compté dans une coupure le reste après une reconnexion silencieuse (session expirée) : un second réveil ne repousse pas « Hors ligne » une deuxième fois. Code : `crates/hearth-link/src/domain/state.rs` (`on_session_expired`), test `a_silent_reauthentication_does_not_forget_that_the_pc_already_woke`.
- 2026-10-05 : réveil conservé après session expirée (HRT-10, suivi de review HRT-07).
