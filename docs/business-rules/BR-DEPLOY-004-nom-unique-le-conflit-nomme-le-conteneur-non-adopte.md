---
id: BR-DEPLOY-004
domaine: DEPLOY
titre: Le nom est unique parmi tous les conteneurs du serveur (gérés ou non) et les déploiements en cours, sans tenir compte de la casse ; le conflit nomme le conteneur non adopté, ou son image
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-deployer-image.md (BR-DEPLOY-004), HRT-68
maj: 2026-11-14
---

# BR-DEPLOY-004 — Le nom est unique parmi tous les conteneurs du serveur (gérés ou non) et les déploiements en cours, sans tenir compte de la casse ; le conflit nomme le conteneur non adopté, ou son image

## Règle
Un nom pris est refusé (`409 NAME_TAKEN`, « Ce nom est déjà pris. ») sans qu'un service existant soit touché, qu'il soit créé par Hearth, adopté ou non. Deux déploiements du même nom lancés ensemble : un seul est accepté (verrou du nom, puis index unique `service_specs_current_name`). Quand un conteneur non adopté porte le nom, le refus et le contrôle du formulaire le nomment, à défaut son image, pour un compte qui gère les services (`details.holder`, `name_holder`, BR-ADOPT-022, HRT-214) ; un service publié n'est pas nommé (son nom est dans la liste), ni un nom pris par un déploiement ou une suppression. Un nom dont les données s'effacent après une suppression répond `NAME_BEING_DELETED`.

## Application (code)
- `crates/hearth-agent/src/application/deployments.rs::Deployments::name_status`
- `crates/hearth-agent/migrations/0012_service_specs_and_ops.sql (index `service_specs_current_name`)`
- `crates/hearth-agent/src/domain/ops/locks.rs::LockTable::reserve_name`
- `apps/desktop/src/components/organisms/DeployForm.vue (nameError : « Ce nom est déjà pris », ou « Ce nom est tenu par « {nom} », un conteneur créé hors de Hearth. Choisissez un autre nom. »)`

## Vérification
- `crates/hearth-agent/tests/service_deploy_https.rs::a_name_taken_by_an_unmanaged_container_names_it_and_never_what_it_holds`
- `crates/hearth-agent/tests/service_deploy_https.rs::two_deployments_of_the_same_name_at_once_one_wins`
- `apps/desktop/src/pages/DeployService.test.ts (dit « Ce nom est déjà pris », ou le conteneur non adopté qui le tient)`

## Cas limites
- Un conteneur créé entre la vérification et la création : le moteur refuse (`name_taken`), l'opération échoue sans rien laisser.

## Règles liées
BR-ADOPT-022, BR-DEPLOY-032

## Historique
- 2026-10-17 — création (HRT-68, déployer une image, côté agent).
- 2026-10-24 — côté client (HRT-69, déployer une image).
- 2026-11-14 — le conflit nomme le conteneur non adopté ou son image (HRT-214, amende BR-ADOPT-022).
