---
id: BR-DEPLOY-001
domaine: DEPLOY
titre: Déployer un service est réservé aux comptes qui ont le droit de gérer les services, quel que soit leur rôle
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-deployer-image.md (BR-DEPLOY-001), HRT-68
maj: 2026-10-24
---

# BR-DEPLOY-001 — Déployer un service est réservé aux comptes qui ont le droit de gérer les services, quel que soit leur rôle

## Règle
`POST /deployments` et `POST /deployments/check` sont de niveau `Services` : le serveur refuse la demande d'un compte sans le droit (`403`, `details.missing`) même si le client est modifié, et inscrit le refus au journal d'activité. Un compte en lecture seule qui a le droit est servi. Le déploiement est un acte d'administration confirmé (`ServiceDeploy` `0x0F`), jamais couvert par l'élévation : le mot de passe est demandé à chaque fois.

## Application (code)
- `crates/hearth-agent/src/entrypoint/http/mod.rs::ENDPOINTS (`/deployments`, `/deployments/check`)`
- `crates/hearth-proto/src/admin_act.rs::ROUTES (`POST /deployments`)`
- `apps/desktop/src/deploy/availability.ts::{deployDoor, routeOpen}` (client : bouton visible et désactivé sans le droit, route renvoyée vers la liste)
- `apps/desktop/src/router/deployGuard.ts::deployRouteGuard`
- `apps/desktop/src/components/molecules/DeployButton.vue`

## Vérification
- `crates/hearth-agent/tests/service_deploy_https.rs::only_an_account_with_the_right_may_deploy_whatever_its_role`
- `crates/hearth-agent/tests/admin_reauth.rs (matrice des actes)`
- `apps/desktop/src/pages/DeployService.test.ts (la route ; le bouton de la section Services)`

## Cas limites
- Le droit retiré en cours de déploiement n'arrête pas le déploiement déjà accepté (BR-DEPLOY-030).

## Règles liées
BR-GRANT-003, BR-GRANT-004, BR-DEPLOY-030

## Historique
- 2026-10-17 — création (HRT-68, déployer une image, côté agent).
- 2026-10-24 — côté client (HRT-69, déployer une image).
