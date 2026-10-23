---
id: BR-DEPLOY-038
domaine: DEPLOY
titre: Un service se crée avec une limite de mémoire et une limite de processeur, bornées par la machine
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-deployer-image.md (BR-DEPLOY-038), HRT-68
maj: 2026-10-24
---

# BR-DEPLOY-038 — Un service se crée avec une limite de mémoire et une limite de processeur, bornées par la machine

## Règle
Sans l'une des deux : `422 LIMITS_REQUIRED`, même venu d'un client modifié. Au moins 0,25 cœur (par pas de 0,25) et 128 Mo (par pas de 1 Mo), au plus les processeurs logiques et la mémoire de la machine lus sur le moteur (`GET /engine/capacity`, ouverte à tout compte, totaux sans aucun nom). Le conteneur reçoit `Memory`, `MemorySwap` égal et `NanoCpus`. Un moteur qui ne dit pas la machine : seul le plancher est jugé.

## Application (code)
- `crates/hearth-agent/src/domain/services/deploy.rs::check_limits`
- `crates/hearth-agent/src/infrastructure/engine/bollard.rs::create_body`
- `crates/hearth-agent/src/application/deployments.rs::Deployments::capacity`
- `apps/desktop/src/components/organisms/DeployForm.vue` avec `ResourceSliders` (client : aucune valeur « sans limite », bornes lues sur la machine)
- `crates/hearth-link/src/manager/service_deploy.rs::engine_capacity` et `apps/desktop/src-tauri/src/service_ops/deploy_dto.rs::EngineCapacityDto` (bornes lues, jamais un nom de service)

## Vérification
- `crates/hearth-agent/tests/service_deploy_https.rs::a_service_is_never_created_without_both_limits`
- `crates/hearth-agent/src/domain/services/deploy.rs::tests::limits_are_required_and_bounded_by_the_machine`
- `crates/hearth-agent/tests/service_deploy_https.rs::the_capacity_gives_the_bounds_of_the_machine_and_what_is_already_committed`
- `apps/desktop/src/pages/DeployFlow.test.ts (les ressources)`
- `apps/desktop/src/link/tauri-deploy.test.ts (les bornes de la machine ; hors bornes : aucune borne)`

## Cas limites
- Les services existants sans limite (adoptés) ne sont pas touchés : l'agent ne pose jamais de limite de lui-même.

## Règles liées
BR-DEPLOY-002, BR-DEPLOY-037

## Historique
- 2026-10-17 — création (HRT-68, déployer une image, côté agent).
- 2026-10-24 — côté client (HRT-69, déployer une image).
- 2026-10-24 — coquille Rust et liaison (HRT-69, déployer une image).
