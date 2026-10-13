---
id: BR-ADOPT-017
domaine: ADOPT
titre: L'adoption suit l'identité stable du service, jamais son nom : par unité du système quand il y en a une, pour la vie du conteneur sinon
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-adopter-un-service.md (BR-ADOPT-017), docs/adr/ADR-0043-par-defaut-hearth-ne-gere-que-ses-services-un-conteneur-hors-hearth-s-adopte.md, HRT-79
maj: 2026-10-13
---

# BR-ADOPT-017 — L'adoption suit l'identité stable du service, jamais son nom : par unité du système quand il y en a une, pour la vie du conteneur sinon

## Règle
L'adoption est mémorisée par l'agent (table `adopted_services`, migration `0011`) sous l'identité stable du service, jamais son nom. Un service lancé par une unité du système (`u_<unité>`) reste adopté tant que l'unité existe, à travers chaque recréation de son conteneur, chaque redémarrage de l'agent et chaque redémarrage du serveur. Un conteneur sans unité (`c_<identifiant>`) n'est adopté que pour la vie de ce conteneur : s'il disparaît, l'adoption est oubliée (ligne effacée) ; un tiers qui recrée un conteneur de même nom crée un autre conteneur, non géré, compté, qui n'hérite de rien.

L'oubli n'a lieu que sur une liste lue du moteur : un moteur absent qui vide la liste ne fait tomber aucune adoption. `supervised_services` (mémoire d'un service dont le conteneur a disparu) ne garde que des services adoptés ; la migration `0011` efface toutes les lignes existantes (rien n'est adopté à ce moment) et la base refuse d'en retenir une autre (déclencheurs), relâcher une adoption efface ce qu'on en retenait.

## Application (code)
- `crates/hearth-agent/migrations/0011_adopted_services.sql`
- `crates/hearth-agent/src/domain/services/visibility.rs::lapsed`
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::{publish, reload_adopted, load_remembered}`
- `crates/hearth-agent/src/infrastructure/sqlite/services_repo.rs::SqliteServiceRepo::{load_adopted, forget_adopted}`

## Vérification
- `crates/hearth-agent/tests/migration_0011.rs::on_a_populated_base_every_remembered_service_is_erased_because_none_is_adopted_yet`
- `crates/hearth-agent/tests/migration_0011.rs::the_base_itself_keeps_only_adopted_services_in_supervised_services`
- `crates/hearth-agent/src/application/services.rs::tests::an_adoption_by_unit_survives_the_recreation_of_the_container_and_a_restart_of_the_agent`
- `crates/hearth-agent/src/application/services.rs::tests::an_adoption_without_a_unit_falls_with_its_container_and_a_namesake_does_not_inherit_it`
- `crates/hearth-agent/src/application/services.rs::tests::a_vanished_engine_does_not_end_the_adoption_of_a_container_it_still_holds`
- `crates/hearth-agent/tests/services_https.rs::an_adoption_by_unit_outlives_the_container_and_the_agent_and_one_without_a_unit_falls_with_its_container`

## Cas limites
- Le redémarrage du serveur est prouvé par celui de l'agent sur la même base, avec un conteneur absent ; le vrai système (unité NixOS, vrai moteur) ne l'est qu'en CI (`engine_supervised.rs`).
- Un conteneur d'une pile Compose perd son adoption à chaque recréation : gêne connue.
- L'acte qui écrit une adoption (adopter, relâcher) est HRT-80 : ce ticket la lit.

## Règles liées
- BR-ADOPT-018, BR-SVC-016, BR-ADOPT-001

## Historique
- 2026-10-13 — création (HRT-79, ADR-0043 : ce que l'agent publie, moitié agent, première partie).
