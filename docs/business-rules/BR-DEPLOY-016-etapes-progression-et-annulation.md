---
id: BR-DEPLOY-016
domaine: DEPLOY
titre: Un déploiement passe par la vérification, la récupération de l'image (progression), la création, le démarrage et la surveillance ; il s'annule pendant la récupération, plus après la création
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-deployer-image.md (BR-DEPLOY-016), HRT-68
maj: 2026-10-24
---

# BR-DEPLOY-016 — Un déploiement passe par la vérification, la récupération de l'image (progression), la création, le démarrage et la surveillance ; il s'annule pendant la récupération, plus après la création

## Règle
Le plan figé de l'opération (`check`, `pull`, `create`, `start`, `watch`) a `create` pour point de non-retour : `create` et `start` écrivent leur `intent` avant le geste. La progression de la récupération (pourcentage jamais décroissant, octets) est publiée au plus toutes les 100 ms. Annuler (`DELETE /operations/services/{id}`, acte `0x10`) abandonne le flux de téléchargement (la connexion au moteur se ferme) et ne laisse rien ; après la création : `409 TOO_LATE_TO_CANCEL`.

## Application (code)
- `crates/hearth-agent/src/domain/services/deploy.rs::op_plan`
- `crates/hearth-agent/src/application/deployments.rs::Deployments::pull`
- `crates/hearth-agent/src/domain/services/pull.rs::PullTracker`
- `apps/desktop/src/components/organisms/DeployProgressDialog.vue` et `composables/useDeployFlow.ts::performCancel` (client : étapes dans l'ordre, annulation pendant la récupération seulement)

## Vérification
- `crates/hearth-agent/tests/service_deploy_https.rs::cancelling_during_the_pull_stops_it_and_creates_nothing`
- `crates/hearth-agent/tests/service_deploy_https.rs::once_the_container_is_being_created_it_is_too_late_to_cancel`
- `crates/hearth-agent/tests/service_deploy_https.rs::the_pull_reports_its_progress_and_an_image_already_there_is_used_as_is`
- `apps/desktop/src/pages/DeployFlow.test.ts (le lancement et la progression)`
- `apps/desktop/e2e/hrt69-deploy.spec.ts`

## Cas limites
- Une annulation qui arrive pendant que l'étape échoue gagne : « annulé », jamais « échoué ».

## Règles liées
BR-OPS-002, BR-DEPLOY-019

## Historique
- 2026-10-17 — création (HRT-68, déployer une image, côté agent).
- 2026-10-24 — côté client (HRT-69, déployer une image).
