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

## Interface (coquille et vue)
- `apps/desktop/src/components/organisms/FingerprintAlert.vue` : alerte bloquante (deux empreintes côte à côte, « Ne pas se connecter » par défaut au clavier et sur Échap, « Accepter la nouvelle empreinte » en bouton contour) ; `stores/link.ts` (`pendingAlert`, `dismissAlert`, `reopenAlert`, `acceptAlert`) ; `components/organisms/OfflineBanner.vue` (suspension et « Voir l'alerte »).
- Coquille : l'alerte est un ÉTAT tenu par `link::PendingBook` (posée dès l'ouverture de la liaison, levée quand le blocage se lève), relu par `list_fingerprint_alerts` à l'abonnement ; `link://fingerprint` n'est qu'un signal. `accept_fingerprint` reçoit l'empreinte affichée et la compare à celle en attente ; `LinkManager::accept_fingerprint` aussi (empreinte présentée posée par la tâche avant l'annonce, refus si différente ou si rien n'attend ; `InputField::Fingerprint`). Tests : `pinning.rs::only_the_fingerprint_the_server_presented_can_be_accepted`, `link_runtime.rs`. Sur `Lagged` la coquille réannonce aussi les alertes.
- Tests : `apps/desktop/src-tauri/tests/link_runtime.rs::a_changed_fingerprint_blocks_the_link_until_it_is_accepted` (vrai agent réinstallé), `src/router/connect.test.ts`, `src/stores/connect-stores.test.ts`, `e2e/connect.spec.ts` (alerte refusée puis acceptée) ; `link_runtime.rs::a_server_reinstalled_while_the_pc_was_off_shows_its_alert_to_an_interface_that_arrives_later` (alerte relue sans événement), `::a_changed_fingerprint_is_a_state_that_a_late_listener_still_finds_and_only_that_one_is_accepted`.

## Vérification
- Tests : `domain::state::tests::a_changed_fingerprint_blocks_every_attempt`, `::only_an_explicit_retry_lifts_a_block`.
- Intégration : `tests/fault_proxy.rs::a_reinstalled_agent_is_refused_by_the_pinned_fingerprint`, `::accepting_the_new_fingerprint_unblocks_the_link`.

## Cas limites
- Les mêmes protections valent pour la reconnexion silencieuse : le mot de passe du coffre ne part pas vers un serveur dont l'empreinte a changé.

## Règles liées
- BR-CONN-002, BR-CONN-011.

## Historique
- 2026-10-05 — création (HRT-07, session 2026-10-04-hearth-creation).
- 2026-10-05 : section Interface (HRT-10).
