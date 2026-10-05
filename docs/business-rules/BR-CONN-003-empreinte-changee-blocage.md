---
id: BR-CONN-003
domaine: CONN
titre: Empreinte changée : blocage, aucune requête authentifiée n'est envoyée
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-connecter-serveur.md (BR-CONN-003), ADR-0005
maj: 2026-10-05
---

# BR-CONN-003 — Empreinte changée : blocage, aucune requête authentifiée n'est envoyée

## Règle
Si le certificat présenté a une autre empreinte que celle mémorisée (agent réinstallé, machine remplacée, interception), la connexion est refusée avant tout échange HTTP : ni jeton ni identifiant ne partent. L'état du lien est `Offline` avec `blocked = FingerprintChanged`, les tentatives sont arrêtées (aucune relance par réveil ou changement de réseau) et un événement d'alerte porte l'empreinte attendue et l'empreinte présentée. Seul l'utilisateur lève le blocage : `accept_fingerprint` (il accepte la nouvelle empreinte) ou la suppression du serveur ; « Réessayer maintenant » retente (et se rebloque si rien n'a changé). La modale d'alerte est de l'interface (à venir, HRT-10).

## Application (code)
- `crates/hearth-link/src/domain/state.rs::LinkMachine::handle` (`Input::FingerprintChanged` donne `Blocked::FingerprintChanged`).
- `crates/hearth-link/src/domain/pinning.rs::decide`, `crates/hearth-link/src/adapters/tls.rs` (refus dans la poignée de main).
- `crates/hearth-link/src/manager/mod.rs::LinkManager::accept_fingerprint`.

## Vérification
- Tests : `domain::state::tests::a_changed_fingerprint_blocks_every_attempt`, `::only_an_explicit_retry_lifts_a_block`.
- Intégration : `tests/fault_proxy.rs::a_reinstalled_agent_is_refused_by_the_pinned_fingerprint`, `::accepting_the_new_fingerprint_unblocks_the_link`.

## Cas limites
- Les mêmes protections valent pour la reconnexion silencieuse : le mot de passe du coffre ne part pas vers un serveur dont l'empreinte a changé.

## Règles liées
- BR-CONN-002, BR-CONN-011.

## Historique
- 2026-10-05 — création (HRT-07, session 2026-10-04-hearth-creation).
