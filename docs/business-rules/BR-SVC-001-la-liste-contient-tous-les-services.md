---
id: BR-SVC-001
domaine: SVC
titre: La liste contient tous les services, arrêtés compris, par observation seule
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-voir-les-services.md (BR-SVC-001), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §5, §7, §9, HRT-61
maj: 2026-10-13
---

# BR-SVC-001 — La liste contient tous les services, arrêtés compris, par observation seule

## Règle
La liste contient tous les services du serveur, créés par Hearth ou non, en marche ou arrêtés ; aucun n'est omis. Limite : un service géré par le système (unité systemd) arrêté n'existe pas pour le moteur, donc n'apparaît pas tant que Hearth ne l'a pas vu tourner (tranche 2). Afficher la liste est une observation : le port n'a aucune écriture. Un conteneur dont l'inspection échoue (délai, réponse illisible) ne gèle pas la liste : il garde sa dernière description, ou apparaît « état non reconnu » sous son nom s'il est nouveau, et la relecture complète est retentée en reculant (2 s, 4 s… 60 s) ; un `warn` une fois par épisode. Un conteneur disparu entre la liste et l'inspection est simplement omis. Si le moteur lui-même est perdu pendant la relecture (injoignable, accès refusé), la relecture échoue : liste inchangée, `accès refusé` quand la liste est refusée (401, 403) alors que `/_ping` répond.

## Application (code)
- `crates/hearth-agent/src/application/ports/container_engine.rs::ContainerEngine::list (`all=true`, une inspection par conteneur)`
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::resync`
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::run`
- `crates/hearth-agent/src/infrastructure/engine/bollard.rs::BollardEngine::list_all`
- `crates/hearth-agent/src/domain/services/state.rs::Observation::unreadable`

## Vérification
- `crates/hearth-agent/src/application/services.rs::tests::a_resync_lists_every_container_stopped_ones_included_sorted_by_name`
- `crates/hearth-agent/tests/services_https.rs::subscribing_sends_a_snapshot_with_the_engine_and_every_service_then_deltas_in_time`
- `crates/hearth-agent/src/application/services.rs::tests::one_unreadable_container_does_not_freeze_the_list`
- `crates/hearth-agent/src/application/services.rs::tests::an_event_for_an_unreadable_container_keeps_it_and_says_so`
- `crates/hearth-agent/src/application/services.rs::tests::a_refused_list_while_the_probe_answers_is_access_denied_not_ready`
- `crates/hearth-agent/src/application/services.rs::tests::a_listing_that_keeps_failing_backs_off_instead_of_looping`
- `crates/hearth-agent/tests/engine_fake_daemon.rs::a_container_the_daemon_cannot_describe_is_unreadable_not_a_failed_list`

## Historique
- 2026-10-13 — création (HRT-61, tranche 1 des déploiements : moteur détecté et liste en lecture seule).
