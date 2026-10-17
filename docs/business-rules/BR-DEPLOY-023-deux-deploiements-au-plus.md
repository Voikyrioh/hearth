---
id: BR-DEPLOY-023
domaine: DEPLOY
titre: Au plus deux déploiements s'exécutent en même temps sur un serveur, tous comptes confondus
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-deployer-image.md (BR-DEPLOY-023), HRT-68
maj: 2026-10-17
---

# BR-DEPLOY-023 — Au plus deux déploiements s'exécutent en même temps sur un serveur, tous comptes confondus

## Règle
Un troisième est refusé (`409 DEPLOY_LIMIT`) et la saisie reste intacte. La place est rendue quand l'exécution finit, quoi qu'il arrive (garde `Drop`).

## Application (code)
- `crates/hearth-agent/src/application/deployments.rs::Deployments::deploy (compteur `running`)`

## Vérification
- `crates/hearth-agent/tests/service_deploy_https.rs::a_third_deployment_is_refused_until_one_of_the_two_ends`

## Cas limites
- Une opération « à vérifier » (`needs_attention`) ne compte pas : elle n'exécute rien.

## Règles liées
- Aucune.

## Historique
- 2026-10-17 — création (HRT-68, déployer une image, côté agent).
