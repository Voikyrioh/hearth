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
Un saut d'horloge de plus de 5 s (réveil de veille) ou un changement de la liste des adresses réseau locales (sondée toutes les 5 s) déclenche une tentative immédiate, quels que soient les délais en cours. Depuis « Connecté », un réveil compte la coupure depuis le dernier message reçu (le PC dormait) ; un changement de réseau rouvre le lien sans attendre que l'ancienne connexion meure.

## Application (code)
- `crates/hearth-link/src/domain/triggers.rs::{detect_wake, network_changed}`.
- `crates/hearth-link/src/domain/state.rs::LinkMachine::on_trigger` (`Input::Woke`, `Input::NetworkChanged`).
- Sondes : `crates/hearth-link/src/adapters/net_watch.rs`, `crates/hearth-link/src/manager/watchers.rs`.

## Vérification
- Tests : `domain::triggers::tests`, `domain::state::tests::row14_offline_wake_or_network_change_shows_reconnecting_and_attempts_at_once`, `::waking_while_connected_counts_the_outage_from_the_last_message`, `::a_network_change_while_connected_reopens_the_link_quietly`.
- Intégration : `tests/fault_proxy.rs::a_network_change_reconnects_immediately`, `::a_wake_up_reconnects_immediately`.

## Cas limites
- Une horloge murale réglée en arrière n'est pas un réveil.
- Après un blocage (empreinte, versions), seul « Réessayer maintenant » relance : un réveil ou un changement de réseau ne contournent pas le blocage.

## Règles liées
- BR-RESIL-005.

## Historique
- 2026-10-05 — création (HRT-07, session 2026-10-04-hearth-creation).
