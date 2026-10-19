---
id: BR-DEPLOY-034
domaine: DEPLOY
titre: Un déploiement qui dure plus de 30 minutes est arrêté ; rien n'est conservé si le programme ne s'était pas encore lancé
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-deployer-image.md (BR-DEPLOY-034), HRT-68
maj: 2026-10-19
---

# BR-DEPLOY-034 — Un déploiement qui dure plus de 30 minutes est arrêté ; rien n'est conservé si le programme ne s'était pas encore lancé

## Règle
Au-delà de `DEPLOY_TIMEOUT_S`, l'exécution est coupée : cause `timeout`, la description et les dossiers sont retirés tant que l'ordre de création n'est pas parti ; une fois parti (le conteneur consigné ou non : le délai peut couper l'attente de la réponse du moteur), le résultat est inconnu (`interrupted`) et rien n'est retiré.

## Application (code)
- `crates/hearth-agent/src/application/deployments.rs::Deployments::execute`

## Vérification
- `crates/hearth-agent/tests/service_deploy_https.rs::a_deployment_that_lasts_past_its_deadline_is_stopped_and_leaves_nothing`
- `crates/hearth-agent/tests/service_deploy_https.rs::a_deadline_that_cuts_the_creation_order_leaves_an_unknown_result_and_removes_nothing`

## Cas limites
- Le moteur a créé le conteneur mais l'agent n'a pas lu sa réponse avant le délai : `interrupted`, la description reste, le compte relit l'état réel.

## Règles liées
BR-DEPLOY-019

## Historique
- 2026-10-17 — création (HRT-68, déployer une image, côté agent).
- 2026-10-19 — l'ordre de création parti suffit à rendre le résultat inconnu (review de la PR #107).
