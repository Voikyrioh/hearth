---
id: BR-ENGINE-008
domaine: ENGINE
titre: Accès refusé distinct de moteur arrêté, avec le compte de l'agent
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-moteur-de-conteneurs.md (BR-ENGINE-008), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §5, §7, §9, HRT-61
maj: 2026-10-13
---

# BR-ENGINE-008 — Accès refusé distinct de moteur arrêté, avec le compte de l'agent

## Règle
« Accès refusé à l'agent » est distinct de « Moteur arrêté » : le moteur tourne mais l'agent n'a pas le droit de lui parler (`EACCES` à l'ouverture du socket, ou refus 401/403). L'agent donne le nom du compte sous lequel il tourne (`agent_account`) et dit s'il est l'administrateur du serveur (`agent_is_admin`) ; l'interface en tire le texte (avec ou sans commande).

## Application (code)
- `crates/hearth-agent/src/domain/services/engine.rs::classify_probe`
- `crates/hearth-agent/src/infrastructure/engine/bollard.rs::connect_failure`
- `crates/hearth-agent/src/infrastructure/system/host_environment.rs::agent_account`
- `crates/hearth-proto/src/api/services.rs::EngineView (agent_account, agent_is_admin)`

## Vérification
- `crates/hearth-agent/tests/engine_fake_daemon.rs::a_refusing_socket_proxy_is_access_denied`
- `crates/hearth-agent/src/infrastructure/system/host_environment.rs::tests::the_account_comes_from_the_real_uid`
- `crates/hearth-agent/tests/services_https.rs::both_roles_get_the_same_engine_and_services_sorted_by_name_stopped_ones_included`

## Cas limites
- Le service de l'agent a `CAP_DAC_OVERRIDE` et `AF_UNIX` : root ouvre le socket du moteur sans appartenir à un groupe (ADR-0012). Sur une installation gérée où l'agent n'est pas root, cet état dit le compte.

## Historique
- 2026-10-13 — création (HRT-61, tranche 1 des déploiements : moteur détecté et liste en lecture seule).
