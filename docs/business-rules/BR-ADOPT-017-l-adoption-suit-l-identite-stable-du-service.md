---
id: BR-ADOPT-017
domaine: ADOPT
titre: L'adoption suit l'identité stable du service, jamais son nom : par unité du système quand il y en a une, pour la vie du conteneur sinon
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-adopter-un-service.md (BR-ADOPT-017), docs/adr/ADR-0043-par-defaut-hearth-ne-gere-que-ses-services-un-conteneur-hors-hearth-s-adopte.md, HRT-79
maj: 2026-10-14
---

# BR-ADOPT-017 — L'adoption suit l'identité stable du service, jamais son nom : par unité du système quand il y en a une, pour la vie du conteneur sinon

## Règle
L'adoption est mémorisée par l'agent (table `adopted_services`, migration `0011`) sous l'identité stable du service, jamais son nom. Un service lancé par une unité du système (`u_<unité>`) reste adopté tant que l'unité existe POUR LE SYSTÈME, à travers chaque recréation de son conteneur, chaque redémarrage de l'agent et chaque redémarrage du serveur. L'unité existe quand le système la connaît : une unité écrite dans un fichier reste connue une fois arrêtée (`loaded/inactive`, établi sur un vrai systemd par la CI) et l'adoption ne tombe donc PAS pour un simple arrêt. Elle ne tombe QUE si le système répond clairement que l'unité n'existe plus (`LoadState=not-found`, état `NotLoaded` : unité supprimée, ou unité transitoire déchargée à l'arrêt) ET qu'aucun conteneur ne la porte, et seulement après `UNIT_GONE_READS` (3) lectures consécutives de cette réponse, espacées de deux périodes de sonde (8 s en production) ; une lecture qui dit autre chose remet le compte à zéro. La ligne est alors effacée de la base, sans entrée de journal (aucun compte n'agit), et la même unité recréée plus tard est un conteneur non géré qu'il faut adopter de nouveau. Tout le reste ne fait JAMAIS tomber une adoption (état `Unavailable`) : une absence de réponse (pas de `systemctl`, délai, sortie illisible), une unité en erreur (`LoadState=error`), au fichier invalide ou en cours d'édition (`bad-setting`), masquée (`masked`) ou fusionnée (`merged`). Ces unités existent, ou le système n'a pas su répondre : on ne sait rien, donc rien ne tombe. Un conteneur sans unité (`c_<identifiant>`) n'est adopté que pour la vie de ce conteneur : s'il disparaît, l'adoption est oubliée (ligne effacée) ; un tiers qui recrée un conteneur de même nom crée un autre conteneur, non géré, compté, qui n'hérite de rien.

**Une adoption dont l'unité n'est plus relue n'ouvre plus aucun droit** (BR-ADOPT-027) : les unités adoptées sont relues en priorité (jusqu'à 128 par passage, bien avant les unités d'un conteneur quelconque, 32 au plus), y compris une unité partagée par deux conteneurs vivants ou sans conteneur ni souvenir ; celle qu'on ne relit pas devient « à confirmer ». Le compte des lectures « introuvable » d'une unité n'est gardé que pour une unité adoptée et relue ; il est effacé dès qu'elle sort de la relecture, est relâchée ou confirmée.

L'oubli n'a lieu que sur une liste lue du moteur : un moteur absent qui vide la liste ne fait tomber aucune adoption. `supervised_services` (mémoire d'un service dont le conteneur a disparu) ne garde que des services adoptés ; la migration `0011` efface toutes les lignes existantes (rien n'est adopté à ce moment) et la base refuse d'en retenir une autre (déclencheurs), relâcher une adoption efface ce qu'on en retenait.

## Application (code)
- `crates/hearth-agent/migrations/0011_adopted_services.sql`
- `crates/hearth-agent/src/domain/services/visibility.rs::{lapsed, unit_is_gone, gone_long_enough, UNIT_GONE_READS}`
- `crates/hearth-agent/src/domain/services/unit.rs::parse_show` (seul `not-found` donne `NotLoaded`)
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::{publish, poll_units, reload_adopted, load_remembered, persist_supervised}`
- `crates/hearth-agent/src/infrastructure/sqlite/services_repo.rs::SqliteServiceRepo::{load_adopted, forget_adopted}`

## Vérification
- `crates/hearth-agent/src/application/services.rs::tests::the_adoption_of_a_unit_shared_by_two_live_containers_is_still_read_and_falls_with_the_unit`
- `crates/hearth-agent/src/application/services.rs::tests::the_count_of_missing_reads_is_erased_when_the_unit_leaves_the_rereading`
- `crates/hearth-agent/src/application/services.rs::tests::a_fallen_adoption_whose_erasure_fails_is_erased_at_the_next_success` (l'effacement qui échoue est refait, l'adoption ne revient pas)
- `crates/hearth-agent/tests/migration_0011.rs::on_a_populated_base_every_remembered_service_is_erased_because_none_is_adopted_yet`
- `crates/hearth-agent/tests/migration_0011.rs::the_base_itself_keeps_only_adopted_services_in_supervised_services`
- `crates/hearth-agent/src/application/services.rs::tests::an_adoption_by_unit_survives_the_recreation_of_the_container_and_a_restart_of_the_agent`
- `crates/hearth-agent/src/application/services.rs::tests::an_adoption_without_a_unit_falls_with_its_container_and_a_namesake_does_not_inherit_it`
- `crates/hearth-agent/src/application/services.rs::tests::a_vanished_engine_does_not_end_the_adoption_of_a_container_it_still_holds`
- `crates/hearth-agent/tests/services_https.rs::an_adoption_by_unit_outlives_the_container_and_the_agent_and_one_without_a_unit_falls_with_its_container`
- `crates/hearth-agent/src/domain/services/visibility.rs::tests::an_adoption_by_unit_falls_only_when_the_system_no_longer_knows_the_unit`
- `crates/hearth-agent/src/application/services.rs::tests::an_adoption_by_unit_survives_a_stop_of_a_unit_the_system_still_knows`
- `crates/hearth-agent/src/application/services.rs::tests::an_adoption_by_unit_falls_when_the_system_no_longer_knows_the_unit_and_not_on_a_silent_answer`
- `crates/hearth-agent/src/application/services.rs::tests::only_a_clear_not_found_repeated_ends_an_adoption_by_unit` (faux superviseur : arrêtée, muette ou invalide, introuvable)
- `crates/hearth-agent/src/application/services.rs::tests::a_read_that_says_anything_else_restarts_the_count_of_missing_reads`
- `crates/hearth-agent/src/domain/services/visibility.rs::tests::a_single_read_never_ends_an_adoption`
- `crates/hearth-agent/src/domain/services/unit.rs::tests::systemctl_show_is_read_for_every_state_a_unit_can_be_in`
- `crates/hearth-agent/tests/service_adoption_https.rs::a_container_without_a_unit_is_adopted_for_its_life_and_a_unit_for_good`

## Cas limites
- Le redémarrage du serveur est prouvé par celui de l'agent sur la même base, avec un conteneur absent ; le vrai système (unité NixOS, vrai moteur) ne l'est qu'en CI (`engine_supervised.rs`).
- Un conteneur d'une pile Compose perd son adoption à chaque recréation : gêne connue.
- L'acte qui écrit une adoption est `ServiceAdoption` (HRT-80, BR-ADOPT-008, BR-ADOPT-013). L'adoption par unité fait confiance à la détection du processus principal de l'unité (`proc_systemd.rs`) : toute modification de cette détection est une modification de la frontière de confidentialité.
- Une unité transitoire (`systemd-run`) est déchargée à l'arrêt (`not-found`, établi par la CI sur un vrai systemd) : son adoption tombe à l'arrêt, une fois l'état confirmé sur trois lectures et le conteneur disparu avec elle. C'est acceptable et voulu (un service lancé à la main n'a pas d'identité durable) ; l'agent n'ajoute aucun message à l'adoption, l'écran des candidats (HRT-81) devra avertir qu'une unité transitoire ne survit pas à son arrêt. Elle tient tant qu'un conteneur arrêté de l'unité existe encore.
- Un état de chargement « masquée » est `Unavailable` (`LoadState=masked`), jamais `NotLoaded` : une unité masquée ne fait rien tomber.
- Tant que la chute n'est pas confirmée, un service retenu dont le système répond « introuvable » n'est plus listé (son adoption reste en base) ; si l'unité revient entre deux lectures, il redevient listé.

## Règles liées
- BR-ADOPT-027, BR-ADOPT-018, BR-SVC-016, BR-ADOPT-001

## Historique
- 2026-10-14 — suivis 2, 3 et 11 de la revue finale de la PR 84 : les unités adoptées sont relues en priorité, le compte « introuvable » est effacé avec la relecture, l'adoption qu'on ne relit plus est « à confirmer » (BR-ADOPT-027), l'effacement raté d'une adoption tombée est refait.
- 2026-10-13 — revue locale de la PR 84 : durcissement. Seul `not-found` fait tomber une adoption, et sur trois lectures consécutives ; `error`, `bad-setting`, `masked`, délai ou silence ne la font jamais tomber (avant, tout état autre que « chargé » la faisait tomber sur une seule lecture, fichier d'unité invalide compris).
- 2026-10-13 — HRT-80 : la fiche dit ce que fait le code (l'adoption par unité tient tant que le système connaît l'unité, y compris arrêtée ; elle tombe quand l'unité n'existe plus), et le code le fait (`unit_is_gone`).
- 2026-10-13 — création (HRT-79, ADR-0043 : ce que l'agent publie, moitié agent, première partie).
