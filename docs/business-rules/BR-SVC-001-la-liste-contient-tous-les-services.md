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
La liste contient tous les services du serveur, créés par Hearth ou non, en marche ou arrêtés ; aucun n'est omis. Limite : un service géré par le système (unité systemd) arrêté n'existe pas pour le moteur, donc n'apparaît pas tant que Hearth ne l'a pas vu tourner (tranche 2). Afficher la liste est une observation : le port n'a aucune écriture. Un conteneur dont l'inspection échoue (délai, réponse illisible) ne gèle pas la liste : il garde sa dernière description, ou apparaît « état non reconnu » sous son nom s'il est nouveau, et la relecture complète est retentée en reculant (2 s, 4 s, 8 s… 60 s) ; un `warn` une fois par épisode. Un conteneur disparu entre la liste et l'inspection est simplement omis. Si le moteur lui-même est perdu pendant la relecture (injoignable, accès refusé), la relecture échoue : liste inchangée, `accès refusé` quand la liste est refusée (401, 403) alors que `/_ping` répond. Choix prudent : un 403 sur l'inspection d'UN seul conteneur (proxy de socket qui filtre) fait aussi basculer toute la liste en `accès refusé`. Le refus ne survit pas à la disparition du moteur : s'il revient, l'état suit la sonde. La liste ne se présente jamais comme fraîche quand elle ne l'est pas : le fil porte `engine.list_degraded` (relecture en échec, conteneurs illisibles, suivi en direct suspendu) et `engine.list_read_at` (dernière lecture complète réussie), pour que le client écrive « Impossible de lire les services » et date la liste. La relecture (jusqu'à 30 s) est menée pendant que la sonde et l'anti-clignotement continuent ; un moteur perdu pendant la lecture l'abandonne.

## Application (code)
- `crates/hearth-agent/src/application/ports/container_engine.rs::ContainerEngine::list (`all=true`, une inspection par conteneur)`
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::resync`
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::run`
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::while_probing`
- `crates/hearth-agent/src/entrypoint/services_wire.rs::engine_view`
- `crates/hearth-agent/src/infrastructure/engine/bollard.rs::BollardEngine::list_all`
- `crates/hearth-agent/src/domain/services/state.rs::Observation::unreadable`

## Vérification
- `crates/hearth-agent/src/application/services.rs::tests::a_resync_lists_every_container_stopped_ones_included_sorted_by_name`
- `crates/hearth-agent/tests/services_https.rs::subscribing_sends_a_snapshot_with_the_engine_and_every_service_then_deltas_in_time`
- `crates/hearth-agent/src/application/services.rs::tests::one_unreadable_container_does_not_freeze_the_list`
- `crates/hearth-agent/src/application/services.rs::tests::an_event_for_an_unreadable_container_keeps_it_and_says_so`
- `crates/hearth-agent/src/application/services.rs::tests::a_refused_list_while_the_probe_answers_is_access_denied_not_ready`
- `crates/hearth-agent/src/application/services.rs::tests::a_listing_that_keeps_failing_backs_off_instead_of_looping`
- `crates/hearth-agent/src/application/services.rs::tests::a_failed_or_partial_read_is_told_with_the_date_of_the_last_good_one`
- `crates/hearth-agent/src/application/services.rs::tests::a_change_of_list_freshness_is_published_to_subscribers`
- `crates/hearth-agent/src/application/services.rs::tests::a_slow_listing_does_not_blind_the_probe_nor_the_debounce`
- `crates/hearth-agent/src/application/services.rs::tests::a_refusal_does_not_outlive_the_engine_that_vanished`
- `crates/hearth-proto/src/api/services.rs::tests::the_list_freshness_fields_are_optional_both_ways`
- `crates/hearth-agent/tests/engine_fake_daemon.rs::a_container_the_daemon_cannot_describe_is_unreadable_not_a_failed_list`

## Historique
- 2026-10-13 — création (HRT-61, tranche 1 des déploiements : moteur détecté et liste en lecture seule).
