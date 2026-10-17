---
id: BR-DEPLOY-034
domaine: DEPLOY
titre: Un déploiement qui dure plus de 30 minutes est arrêté ; rien n'est conservé si le programme ne s'était pas encore lancé
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-deployer-image.md (BR-DEPLOY-034), HRT-68
maj: 2026-10-17
---

# BR-DEPLOY-034 — Un déploiement qui dure plus de 30 minutes est arrêté ; rien n'est conservé si le programme ne s'était pas encore lancé

## Règle
Au-delà de `DEPLOY_TIMEOUT_S`, l'exécution est coupée : cause `timeout`, la description et les dossiers sont retirés tant qu'aucun conteneur n'est consigné ; sinon le résultat est inconnu (`interrupted`).

## Application (code)
- `crates/hearth-agent/src/application/deployments.rs::Deployments::execute`

## Vérification
- `crates/hearth-agent/tests/service_deploy_https.rs::a_deployment_that_lasts_past_its_deadline_is_stopped_and_leaves_nothing`

## Cas limites
- Aucun cas particulier connu.

## Règles liées
BR-DEPLOY-019

## Historique
- 2026-10-17 — création (HRT-68, déployer une image, côté agent).
