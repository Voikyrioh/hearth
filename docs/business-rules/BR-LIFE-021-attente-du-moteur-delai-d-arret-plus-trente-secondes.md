---
id: BR-LIFE-021
domaine: LIFE
titre: Si le moteur ne répond pas dans le délai d'arrêt du service plus 30 secondes (60 au minimum), l'agent cesse d'attendre : résultat inconnu, jamais rejoué
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-cycle-de-vie-service.md (BR-LIFE-021), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md, HRT-64
maj: 2026-10-17
---

# BR-LIFE-021 — Si le moteur ne répond pas dans le délai d'arrêt du service plus 30 secondes (60 au minimum), l'agent cesse d'attendre : résultat inconnu, jamais rejoué

## Règle
L'attente d'un ordre est `max(délai d'arrêt + 30 s, 60 s)` (`wait_budget`), la même pour le moteur et le superviseur (la requête vers le moteur a ce délai, pas les 10 s des lectures). Au-delà, l'opération est `interrupted` avec la cause `timeout` (« délai dépassé ») et l'ordre n'est JAMAIS rejoué. L'état du service est relu quand le moteur répond.

## Application (code)
- `crates/hearth-agent/src/domain/ops/lifecycle.rs::{wait_budget, wait_budget_with}`
- `crates/hearth-agent/src/application/service_lifecycle.rs::{LifecycleTimings::wait_for, ServiceLifecycle::order}`
- `crates/hearth-agent/src/infrastructure/engine/bollard.rs::BollardEngine::docker_for_stop`

## Vérification
- `crates/hearth-agent/src/domain/ops/lifecycle.rs::tests::the_wait_is_the_grace_plus_thirty_seconds_and_at_least_sixty`
- `crates/hearth-agent/src/domain/ops/lifecycle.rs::tests::the_margin_and_the_floor_can_be_shortened_for_a_test_and_the_formula_stays_the_same`
- `crates/hearth-agent/tests/service_lifecycle_https.rs::an_order_the_engine_refuses_is_a_failure_and_one_it_never_answers_is_an_unknown_result`
- `crates/hearth-agent/tests/supervisor_proc.rs::a_failing_or_slow_systemctl_gives_a_typed_error_and_a_slow_one_is_cut_at_the_wait`

## Cas limites
- Aucun cas limite propre à cette règle au-delà de ceux de la règle.

## Règles liées
- BR-LIFE-009, BR-LIFE-019, BR-LIFE-020

## Historique
- 2026-10-17 — création (HRT-64 : démarrer, arrêter, redémarrer un service, côté agent).
