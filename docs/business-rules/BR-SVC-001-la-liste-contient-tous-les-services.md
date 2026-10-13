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
La liste contient tous les services du serveur, créés par Hearth ou non, en marche ou arrêtés ; aucun n'est omis. Limite : un service géré par le système (unité systemd) arrêté n'existe pas pour le moteur, donc n'apparaît pas tant que Hearth ne l'a pas vu tourner (tranche 2). Afficher la liste est une observation : le port n'a aucune écriture. Un conteneur dont l'inspection échoue (délai, réponse illisible) ne gèle pas la liste : il garde sa dernière description, ou apparaît « état non reconnu » sous son nom s'il est nouveau, et la relecture complète est retentée en reculant (2 s, 4 s, 8 s… 60 s) ; un `warn` une fois par épisode. Un conteneur disparu entre la liste et l'inspection est simplement omis. Si le moteur lui-même est perdu pendant la relecture (injoignable, accès refusé), la relecture échoue : liste inchangée, `accès refusé` quand la liste est refusée (401, 403) alors que `/_ping` répond. Choix prudent : un 403 sur l'inspection d'UN seul conteneur (proxy de socket qui filtre) fait aussi basculer toute la liste en `accès refusé`. Le refus ne survit pas à la disparition du moteur : s'il revient, l'état suit la sonde. La liste ne se présente pas comme fraîche quand elle ne l'est pas (limite connue : BR-SVC-018) : le fil porte `engine.list_degraded` (relecture en échec, conteneurs illisibles, suivi en direct suspendu, OU liste pas encore relue depuis que le moteur a démarré ou est revenu : aucune publication sous « prêt » ne présente l'ancienne liste, ni une liste vide non lue, comme fraîche ; la relecture est jugée sur l'état OBSERVÉ du moteur, pas sur celui que l'anti-clignotement a publié : une liste lue pendant un retour retenu, puis perdue par une seconde disparition, n'est pas reprise sous « prêt », y compris quand elle a été relue sous un état inaccessible publié) et, pour l'âge, `engine.list_current_at` (dernier moment où la liste était à jour : l'heure de l'agent `engine.at` quand elle l'est, sinon l'instant où l'agent a vu qu'elle cessait de l'être ; une liste suivie en direct par les événements reste « à jour » sans relecture). `engine.list_read_at` (dernière lecture complète réussie) reste, information seulement. Le client écrit « Impossible de lire les services » et date la liste. Âge affiché = `at - list_current_at` (deux dates de l'agent, jamais l'horloge du client, bornée à zéro) plus le temps écoulé chez le client depuis la réception, mesuré par son horloge (même principe que « Vu il y a » : on part de l'instant de réception). `at` présent et `list_current_at` absent : l'agent n'a aucune liste (jamais lue, ou vidée dans un état sans liste) ; `at` absent : agent ancien, date inconnue. La relecture (jusqu'à 30 s) est menée pendant que la sonde et l'anti-clignotement continuent ; un moteur perdu pendant la lecture l'abandonne.

## Application (code)
- `crates/hearth-agent/src/application/ports/container_engine.rs::ContainerEngine::list (`all=true`, une inspection par conteneur)`
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::resync`
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::run`
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::while_probing`
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::{list_degraded, list_current_at, track_staleness, note_published}`
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
- `crates/hearth-agent/src/application/services.rs::tests::a_list_from_before_the_engine_vanished_is_never_shown_as_fresh_when_it_returns`
- `crates/hearth-agent/src/application/services.rs::tests::the_date_on_the_wire_is_the_last_moment_the_list_was_up_to_date`
- `crates/hearth-agent/src/application/services.rs::tests::a_list_that_stops_being_followed_is_dated_from_the_moment_it_was_noticed`
- `crates/hearth-agent/src/application/services.rs::tests::a_missing_engine_has_no_list_to_date`
- `crates/hearth-proto/src/api/services.rs::tests::the_list_age_fields_tell_a_never_read_list_from_an_old_agent`
- `crates/hearth-agent/src/application/services.rs::tests::a_refusal_does_not_outlive_the_engine_that_vanished`
- `crates/hearth-proto/src/api/services.rs::tests::the_list_freshness_fields_are_optional_both_ways`
- `crates/hearth-agent/tests/engine_fake_daemon.rs::a_container_the_daemon_cannot_describe_is_unreadable_not_a_failed_list`

## Historique
- 2026-10-13 — création (HRT-61, tranche 1 des déploiements : moteur détecté et liste en lecture seule).
- 2026-10-13 — HRT-72 : le marquage couvre aussi « liste pas relue depuis le retour du moteur » (FIX-01M4SFCQ7N5TN8VC21KBQR87PP) ; la date du fil est le dernier moment où la liste était à jour, avec l'heure de l'agent (FIX-01M4SFCRJWX8KMBC1ZK9ZWETTW).
- 2026-10-13 — HRT-72 (revue de la PR 78, R1) : « liste relue » suit l'état observé du moteur, plus l'état publié ; `list_current_at` n'est jamais posé à `at` pour une liste non relue depuis la dernière coupure observée, et reste absent dans les états sans liste (FIX-01M4SFCQ7N5TN8VC21KBQR87PP). Une coupure retenue par l'anti-clignotement ne publie aucune marque.
- 2026-10-13 — HRT-73 (revue de la PR 78, seconde passe) : plus d'exception sous un état inaccessible publié, la relecture est défaite par tout moteur observé non prêt (FIX-01M4SFCQ7N5TN8VC21KBQR87PP) ; textes alignés, la limite restante est écrite dans BR-SVC-018.
