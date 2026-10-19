---
id: BR-LIFE-018
domaine: LIFE
titre: Chaque action est inscrite au journal d'activité avec le compte, le service, le geste, le résultat et une cause d'une liste fermée
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-cycle-de-vie-service.md (BR-LIFE-018), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md, HRT-64
maj: 2026-10-17
---

# BR-LIFE-018 — Chaque action est inscrite au journal d'activité avec le compte, le service, le geste, le résultat et une cause d'une liste fermée

## Règle
Les actions `service.start`, `service.stop`, `service.restart` portent le compte, le service (son nom, jamais plus), le résultat (réussi, refusé, échoué) et une cause TYPÉE, jamais du texte libre ni un extrait des journaux du service : « arrêt forcé » (réussi, note `Forced`), « port occupé », « arrêté juste après le démarrage », « moteur inaccessible », « service introuvable », « droits insuffisants », « action déjà en cours », « déjà dans l'état demandé », « état du service incompatible avec l'action », « repart tout seul », « délai dépassé », « interrompu par un redémarrage de l'agent », « refusé par le moteur ». Les refus synchrones et les échecs de route sont écrits par la couche d'accès (`failure_of`) ; l'issue d'une action acceptée l'est à sa fin (`ServiceLifecycle::record`), et à son démarrage pour une action interrompue (`record_interrupted`). Les codes d'opération équivalents sont `forced_stop`, `port_in_use`, `stopped_right_after_start`, `engine_unavailable`, `engine_refused`, `restarted_by_host`, `timeout`, `interrupted`, `service_not_found`.

## Application (code)
- `crates/hearth-agent/src/domain/audit/event.rs::{Reason, Target::ServiceLifecycle, LifecycleNote}`
- `crates/hearth-agent/src/domain/ops/kind.rs::Cause`
- `crates/hearth-agent/src/application/service_lifecycle.rs::{ServiceLifecycle::record, reason_of, record_interrupted}`
- `crates/hearth-agent/src/entrypoint/http/auth.rs::failure_of`

## Vérification
- `crates/hearth-agent/src/domain/audit/event.rs::tests::the_lifecycle_reasons_are_the_closed_list_of_the_rule_in_french`
- `crates/hearth-agent/tests/service_lifecycle_https.rs::stopping_answers_202_with_the_grace_of_the_service_and_writes_the_wanted_stop_first`
- `crates/hearth-agent/tests/service_lifecycle_https.rs::an_order_the_engine_refuses_is_a_failure_and_one_it_never_answers_is_an_unknown_result`
- `crates/hearth-agent/src/application/service_lifecycle.rs::tests::every_cause_of_the_lifecycle_has_its_fixed_reason_in_the_activity_log`

## Cas limites
- Annuler la confirmation n'écrit rien (BR-LIFE-017).

## Règles liées
- BR-AUDIT-003, BR-LIFE-001, BR-LIFE-014, BR-LIFE-019

## Historique
- 2026-10-17 — création (HRT-64 : démarrer, arrêter, redémarrer un service, côté agent).
