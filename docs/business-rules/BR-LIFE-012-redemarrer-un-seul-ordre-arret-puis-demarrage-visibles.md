---
id: BR-LIFE-012
domaine: LIFE
titre: Redémarrer enchaîne « Arrêt en cours », « Démarrage », « En marche » en un seul ordre ; si le service s'arrête mais ne repart pas, il reste arrêté avec la cause
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-cycle-de-vie-service.md (BR-LIFE-012), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md, HRT-64
maj: 2026-10-17
---

# BR-LIFE-012 — Redémarrer enchaîne « Arrêt en cours », « Démarrage », « En marche » en un seul ordre ; si le service s'arrête mais ne repart pas, il reste arrêté avec la cause

## Règle
Redémarrer est UN ordre du pilote (`restart` du moteur, `systemctl restart` du système), avec le délai de grâce du service. Le registre montre « Arrêt en cours » tant que l'ancien conteneur tourne, « Démarrage » quand il est arrêté, « En marche » dès qu'une exécution postérieure à l'ordre tourne (`TransitionKind::Restarting`) ; pour un service géré par le système, une étape trop brève peut ne pas se voir. Un arrêt forcé est annoncé comme en BR-LIFE-009. Si l'arrêt a réussi mais le démarrage échoue (port pris entre-temps), l'opération est `failed` avec la cause du démarrage, le service reste « Arrêté » (la sortie est celle de l'arrêt de Hearth, `keep_stop`) et n'est jamais « En erreur ». Les démarrages demandés par Hearth ne comptent pas pour « Redémarre en boucle » (`StartWindow::exempt`).

## Application (code)
- `crates/hearth-agent/src/application/service_lifecycle.rs::ServiceLifecycle::run_restart`
- `crates/hearth-agent/src/domain/services/state.rs::{TransitionKind::Restarting, StartWindow::exempt}`
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::exempt_starts`

## Vérification
- `crates/hearth-agent/tests/service_lifecycle_https.rs::restarting_is_one_order_and_the_restarts_hearth_asked_never_make_a_boucle`
- `crates/hearth-agent/tests/service_lifecycle_https.rs::a_restart_whose_start_meets_a_taken_port_leaves_the_service_stopped_not_in_error`
- `crates/hearth-agent/src/domain/services/state.rs::tests::a_restart_goes_through_stopping_then_starting_then_the_new_run_is_running`
- `crates/hearth-agent/src/domain/services/state.rs::tests::a_start_asked_by_hearth_is_not_counted_toward_the_restart_loop`
- `crates/hearth-agent/tests/support/engine_contract.rs::writes (redémarrer, trois exécutants)`

## Cas limites
- Aucun cas limite propre à cette règle au-delà de ceux de la règle.

## Règles liées
- BR-LIFE-006, BR-LIFE-009, BR-SVC-006, BR-SVC-015

## Historique
- 2026-10-17 — création (HRT-64 : démarrer, arrêter, redémarrer un service, côté agent).
