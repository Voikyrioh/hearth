---
id: BR-DEPLOY-004
domaine: DEPLOY
titre: Le nom est unique parmi tous les conteneurs du serveur (gérés ou non) et les déploiements en cours, sans tenir compte de la casse ; le conflit ne nomme jamais un conteneur non adopté
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-deployer-image.md (BR-DEPLOY-004), HRT-68
maj: 2026-10-24
---

# BR-DEPLOY-004 — Le nom est unique parmi tous les conteneurs du serveur (gérés ou non) et les déploiements en cours, sans tenir compte de la casse ; le conflit ne nomme jamais un conteneur non adopté

## Règle
Un nom pris est refusé (`409 NAME_TAKEN`, « Ce nom est déjà pris. ») sans qu'un service existant soit touché, qu'il soit créé par Hearth, adopté ou non. Deux déploiements du même nom lancés ensemble : un seul est accepté (verrou du nom, puis index unique `service_specs_current_name`). Un nom dont les données s'effacent après une suppression répond `NAME_BEING_DELETED`.

## Application (code)
- `crates/hearth-agent/src/application/deployments.rs::Deployments::name_status`
- `crates/hearth-agent/migrations/0012_service_specs_and_ops.sql (index `service_specs_current_name`)`
- `crates/hearth-agent/src/domain/ops/locks.rs::LockTable::reserve_name`
- `apps/desktop/src/components/organisms/DeployForm.vue (nameError : « Ce nom est déjà pris », jamais par qui)`

## Vérification
- `crates/hearth-agent/tests/service_deploy_https.rs::a_name_taken_by_anyone_is_refused_without_saying_by_whom`
- `crates/hearth-agent/tests/service_deploy_https.rs::two_deployments_of_the_same_name_at_once_one_wins`
- `apps/desktop/src/pages/DeployService.test.ts (dit « Ce nom est déjà pris » sans dire par qui)`

## Cas limites
- Un conteneur créé entre la vérification et la création : le moteur refuse (`name_taken`), l'opération échoue sans rien laisser.

## Règles liées
BR-ADOPT-022, BR-DEPLOY-032

## Historique
- 2026-10-17 — création (HRT-68, déployer une image, côté agent).
- 2026-10-24 — côté client (HRT-69, déployer une image).
