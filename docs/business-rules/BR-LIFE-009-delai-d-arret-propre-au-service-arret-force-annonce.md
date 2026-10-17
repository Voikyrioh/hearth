---
id: BR-LIFE-009
domaine: LIFE
titre: Le délai d'arrêt est propre à chaque service, l'ordre d'arrêt le respecte et un arrêt forcé est annoncé
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-cycle-de-vie-service.md (BR-LIFE-009), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md, HRT-64
maj: 2026-10-17
---

# BR-LIFE-009 — Le délai d'arrêt est propre à chaque service, l'ordre d'arrêt le respecte et un arrêt forcé est annoncé

## Règle
Le délai de grâce d'un service est celui de son unité (`TimeoutStopUSec`) pour un service supervisé, sinon celui du conteneur (`StopTimeout`), sinon 10 secondes (`stop_grace_s`). L'ordre d'arrêt part avec ce délai (le moteur envoie l'ordre propre, puis `SIGKILL` à l'échéance ; le système applique le sien). `202` rend `grace_s`. Un arrêt est « forcé » quand le processus est mort sous `SIGKILL` (code 137) après le délai (`engine_stop_was_forced`) ; pour un service supervisé, quand l'arrêt a pris tout son délai (`supervised_stop_was_forced`, limite assumée : le système ne rend pas le code de sortie). L'opération est alors réussie avec `forced = true` et la cause `forced_stop`, et le journal d'activité écrit « (arrêt forcé) ». Un 137 AVANT l'échéance n'est pas un arrêt forcé par Hearth.

## Application (code)
- `crates/hearth-agent/src/domain/ops/lifecycle.rs::{engine_stop_was_forced, supervised_stop_was_forced}`
- `crates/hearth-agent/src/domain/services/state.rs::stop_grace_s`
- `crates/hearth-agent/src/application/service_lifecycle.rs::ServiceLifecycle::{run_stop, stop_was_forced}`
- `crates/hearth-agent/src/infrastructure/engine/bollard.rs::BollardEngine::docker_for_stop`

## Vérification
- `crates/hearth-agent/src/domain/ops/lifecycle.rs::tests::a_kill_after_the_grace_is_a_forced_stop_and_an_earlier_one_is_not`
- `crates/hearth-agent/src/domain/ops/lifecycle.rs::tests::a_supervised_stop_is_forced_only_when_it_used_its_whole_delay`
- `crates/hearth-agent/tests/service_lifecycle_https.rs::a_non_zero_exit_after_a_stop_asked_from_hearth_is_a_stop_not_an_error`
- `crates/hearth-agent/tests/service_lifecycle_https.rs::stopping_answers_202_with_the_grace_of_the_service_and_writes_the_wanted_stop_first`
- `crates/hearth-agent/tests/support/engine_contract.rs::writes (137 après le délai, trois exécutants)`

## Cas limites
- Le délai n'est pas réglable depuis Hearth dans ce lot.

## Règles liées
- BR-LIFE-021, BR-SVC-007

## Historique
- 2026-10-17 — création (HRT-64 : démarrer, arrêter, redémarrer un service, côté agent).
