---
id: BR-ENGINE-021
domaine: ENGINE
titre: Le pilotage des services est désactivable dans la configuration de l'agent
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-moteur-de-conteneurs.md (BR-ENGINE-021), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §5, §7, §9, HRT-61
maj: 2026-10-13
---

# BR-ENGINE-021 — Le pilotage des services est désactivable dans la configuration de l'agent

## Règle
L'administrateur du serveur peut désactiver le pilotage des services dans la configuration de l'agent (`[engine] enabled = false` ou `HEARTH_ENGINE=off`) ; il est permis par défaut. Désactivé, l'état est « désactivé » : l'agent n'ouvre jamais le socket du moteur, n'envoie ni liste ni compteur. Hearth ne change jamais ce réglage lui-même. Le texte affiché est porté par l'interface.

## Application (code)
- `crates/hearth-agent/src/infrastructure/config/mod.rs::EngineConfig`
- `crates/hearth-agent/src/infrastructure/config/mod.rs::load`
- `crates/hearth-agent/src/domain/services/engine.rs::classify_probe (`enabled = false`)`
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::run`

## Vérification
- `crates/hearth-agent/src/infrastructure/config/mod.rs::tests::the_engine_section_and_the_environment_can_disable_it_or_move_the_socket`
- `crates/hearth-agent/tests/services_https.rs::a_disabled_engine_is_told_as_disabled_and_the_socket_is_never_opened`
- `crates/hearth-agent/src/application/services.rs::tests::a_disabled_engine_is_never_probed`

## Historique
- 2026-10-13 — création (HRT-61, tranche 1 des déploiements : moteur détecté et liste en lecture seule).
