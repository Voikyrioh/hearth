---
id: BR-LIFE-013
domaine: LIFE
titre: Une seule opération à la fois par service ; les écritures de fichiers se partagent, les lectures n'en prennent aucun
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-modifier-un-service.md (BR-LIFE-013), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 0, 0 bis et 6), HRT-83
maj: 2026-10-24
---

# BR-LIFE-013 — Une seule opération à la fois par service ; les écritures de fichiers se partagent, les lectures n'en prennent aucun

## Règle
Toute opération (démarrer, arrêter, redémarrer, déployer, modifier, supprimer, ressources, sauvegarder, restaurer, reprendre un monde) prend le verrou EXCLUSIF de son service à l'acceptation et le rend à sa fin. Une écriture de fichier prend un verrou PARTAGÉ avec les autres écritures et est refusée sous un exclusif ; une lecture n'en prend aucun (sauf la lecture de fichiers pendant une restauration, BR-FILES-024). Deux verrous du serveur : `heavy_io` (une place : sauvegarde, restauration, reprise de monde) et les noms réservés (BR-DEL-019). Le verrou est en mémoire, rebâti au démarrage pour les opérations « à vérifier » (BR-EDIT-031) : verrou du service, nom réservé (suppression, déploiement) et place lourde (sauvegarde, restauration, reprise de monde), déduits du genre. Ceinture en base : un index unique partiel interdit deux opérations non terminées sur un même service.

## Application (code)
- `crates/hearth-agent/src/domain/ops/locks.rs::LockTable`
- `crates/hearth-agent/src/domain/ops/kind.rs::OpKind::lock_mode`
- `crates/hearth-agent/src/application/service_ops.rs::ServiceOps::accept`
- `crates/hearth-agent/src/application/closing_gates.rs::ClosingGates` (une porte de clôture par service, HRT-101)
- `crates/hearth-agent/src/application/service_ops.rs::ServiceOps::closing`, `wait_closed`, `busy`, `accept_after_wait`, `settle`, `sweep`
- `crates/hearth-agent/src/application/service_op_access.rs::ServiceOpAccess::cancel` et `decide` (porte prise avant le commit)
- `crates/hearth-agent/migrations/0012_service_specs_and_ops.sql (index service_ops_open)`
- `crates/hearth-agent/src/application/service_lifecycle.rs::ServiceLifecycle::request` (HRT-64)
- `apps/desktop/src/services/lifecycle.ts::controlsOf` et `services/pilot.ts::pilotLock` (client, HRT-65 : boutons désactivés avec la raison et « Lancée par {compte} »)

## Vérification
- `crates/hearth-agent/src/domain/ops/locks.rs::tests`
- `crates/hearth-agent/src/domain/ops/kind.rs::tests::the_statuses_that_hold_the_lock_are_the_ones_the_unique_index_covers`
- `crates/hearth-agent/tests/service_ops_journal.rs::an_accepted_operation_takes_the_service_and_gives_it_back_when_it_ends`
- `crates/hearth-agent/tests/service_ops_journal.rs::the_unique_index_is_the_belt_when_the_memory_lock_has_a_defect`
- `crates/hearth-agent/tests/service_ops_journal.rs::heavy_io_has_one_seat_for_the_whole_server`
- `crates/hearth-agent/tests/service_ops_journal.rs::a_request_waits_for_the_closing_then_sees_the_service_free_through_close`, `accept_waits_for_the_closing_then_succeeds_through_move_to`, `a_concurrent_accept_never_makes_a_subscriber_receive_the_outcome_twice`, `a_slow_closing_on_one_service_never_makes_another_service_wait`, `a_closing_stuck_past_the_deadline_does_not_freeze_the_service_and_a_running_operation_is_still_refused` (échéance injectée), `the_expiry_of_a_decision_holds_the_gate_until_it_is_published`, `the_cancellation_of_the_access_holds_the_gate_until_it_is_published`, `the_decision_of_the_access_holds_the_gate_until_it_is_published`, `infrastructure/sqlite::tests::the_closing_gate_waits_at_least_as_long_as_the_database`
- `crates/hearth-agent/tests/service_lifecycle_https.rs::the_next_action_is_never_refused_once_the_previous_outcome_can_be_read`
- `crates/hearth-agent/tests/service_lifecycle_https.rs::a_second_request_on_a_busy_service_is_refused_with_who_holds_it_and_other_services_are_free`

## Cas limites
- Le refus est typé (`409 SERVICE_BUSY`, `details.by` le compte, `details.kind` le genre) : « Une action est déjà en cours sur ce service. » (HRT-64, `ServiceLifecycle::request` lit l'index du journal avant l'état du service ; deux demandes simultanées sont départagées par le verrou en mémoire puis la base).
- Le verrou est VISIBLE de tous : `ServiceItem.busy` (type, compte, depuis quand) est la projection de l'opération non terminée du service (BR-OPS-005, HRT-144).
- Une issue terminale lisible (le `GET` lit le journal) ne laisse jamais le service « occupé » (HRT-101). Il y a UNE porte de clôture PAR SERVICE (`ClosingGates`, table clé vers `RwLock`, entrée retirée quand plus personne ne la tient ni ne l'attend). Toute clôture (statut terminal) la prend en lecture AVANT son commit et la tient jusqu'à la fin de `publish` (verrous rendus, projection, annonce) : `close`/`finish_with`, `move_to` terminal, l'expiration par `sweep` (le nettoyage disque vient après, hors porte), l'annulation et la décision de `ServiceOpAccess` (porte TOUJOURS prise avant le `begin`, sans dépendre d'un statut lu hors transaction, passée à `settle`). Les transitions non terminales faites par `ServiceOps` (`move_to` non terminal) ne la prennent pas. `ServiceLifecycle::request` attend la porte du service UNE fois (`wait_closed`, au plus 5 s, `CLOSING_GATE_WAIT`, au moins égal au `busy_timeout` de la base), juge `busy`, puis `accept_after_wait` ; `accept` seul attend aussi une fois. À l'échéance on juge quand même et un `warn!` sans valeur est tracé : une opération réellement en cours donne toujours 409 ; « jamais refusé après lecture de l'issue » ne vaut que si la clôture tient dans l'échéance (au-delà, on juge sur l'état courant) ; une clôture bloquée ne gèle pas le service. Sans clôture en cours l'attente est nulle. Coût : un verrou court sur la table et une allocation par clôture ; une clôture lente sur A ne fait jamais attendre B. Pas d'interblocage : l'ordre est toujours porte puis transaction, une clôture n'attend jamais une autre porte. Limite : `settle` appelé sans garde (aucun appelant aujourd'hui hors accès) ne couvre que libération et annonce. La liste des services (`ServiceItem.busy`) lit la projection SANS attendre : pendant une clôture elle peut dire « occupé » un instant, la publication qui suit la rafraîchit.

## Règles liées
- BR-FILES-024, BR-DEL-019, BR-EDIT-031

## Historique
- 2026-10-24 — la porte de clôture : plus de 409 juste après la lecture de l'issue terminale (HRT-101, FIX-01M5P0Q3T8ZK6V2R9XH4DWB7NC).
- 2026-10-17 — le refus typé et le compte qui tient le verrou (HRT-64).
- 2026-10-14 — `busy` et `last_action` projetés du journal (HRT-144).
- 2026-10-13 — création (HRT-83, ADR-0044 et ADR-0045 : socle des services créés par Hearth, côté agent).
