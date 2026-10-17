---
id: BR-LIFE-019
domaine: LIFE
titre: Si le moteur n'est pas prêt, l'action est refusée avec l'état du moteur et le service n'a pas été modifié ; un résultat que le moteur ne donne pas est inconnu, pas échoué
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-cycle-de-vie-service.md (BR-LIFE-019), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md, HRT-64
maj: 2026-10-17
---

# BR-LIFE-019 — Si le moteur n'est pas prêt, l'action est refusée avec l'état du moteur et le service n'a pas été modifié ; un résultat que le moteur ne donne pas est inconnu, pas échoué

## Règle
Le moteur est jugé en premier, sur l'état PUBLIÉ (après l'anti-clignotement) : tout état autre que « prêt » est refusé tout de suite, `503 ENGINE_UNAVAILABLE` avec `details.engine`, « Le service n'a pas été modifié. ». Un refus net du moteur pendant l'action (autre raison) est un échec (`engine_refused`) : le service n'a pas été modifié, l'arrêt voulu est oublié. Si le moteur devient injoignable pendant l'action, le résultat est INCONNU : l'opération est `interrupted` (cause `engine_unavailable`), jamais `failed` ; l'état réel est relu quand le moteur répond.

## Application (code)
- `crates/hearth-agent/src/domain/ops/lifecycle.rs::engine_ready`
- `crates/hearth-agent/src/application/service_lifecycle.rs::{ServiceLifecycle::request, OrderFailure::finish}`
- `crates/hearth-agent/src/entrypoint/http/services.rs::lifecycle_error`

## Vérification
- `crates/hearth-agent/tests/service_lifecycle_https.rs::an_engine_that_is_not_ready_refuses_before_anything_else_and_says_which_state`
- `crates/hearth-agent/tests/service_lifecycle_https.rs::an_engine_that_becomes_unreachable_during_the_action_leaves_the_result_unknown_not_failed`
- `crates/hearth-agent/src/application/service_lifecycle.rs::tests::a_refusal_of_the_pilot_is_a_failure_and_an_unknown_result_is_never_one`
- `crates/hearth-agent/src/domain/ops/lifecycle.rs::tests::only_a_ready_engine_receives_an_order`

## Cas limites
- Aucun cas limite propre à cette règle au-delà de ceux de la règle.

## Règles liées
- BR-LIFE-020, BR-LIFE-021, BR-ENGINE-014

## Historique
- 2026-10-17 — création (HRT-64 : démarrer, arrêter, redémarrer un service, côté agent).
