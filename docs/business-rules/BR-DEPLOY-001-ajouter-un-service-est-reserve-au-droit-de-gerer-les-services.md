---
id: BR-DEPLOY-001
domaine: DEPLOY
titre: Ajouter un service est réservé aux comptes qui ont le droit de gérer les services, quel que soit leur rôle
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-deployer-image.md (BR-DEPLOY-001), HRT-68
maj: 2026-11-16
---

# BR-DEPLOY-001 — Ajouter un service est réservé aux comptes qui ont le droit de gérer les services

## Règle
`POST /deployments` et `POST /deployments/check` sont de niveau `Services` : le serveur refuse la demande d'un compte sans le droit (`403`, `details.missing`) même si le client est modifié, et inscrit le refus au journal d'activité. Un compte en lecture seule qui a le droit est servi. Côté client (HRT-243), la page Services a UN seul bouton de création, « Ajouter un serveur de jeu » : visible et désactivé avec sa raison sans le droit, il ouvre l'écran des trois chemins (BR-DEPLOY-039), gardé comme l'était le formulaire (sans le droit : retour à la liste des services). Le formulaire d'image reste derrière « Continuer sans fiche », gardé de même. Le déploiement est un acte d'administration confirmé (`ServiceDeploy` `0x0F`), jamais couvert par l'élévation : le mot de passe est demandé à chaque fois.

## Application (code)
- `crates/hearth-agent/src/entrypoint/http/mod.rs::ENDPOINTS (`/deployments`, `/deployments/check`)`
- `crates/hearth-proto/src/admin_act.rs::ROUTES (`POST /deployments`)`
- `apps/desktop/src/deploy/availability.ts::{deployDoor, routeOpen}` (client : bouton visible et désactivé sans le droit, route renvoyée vers la liste)
- `apps/desktop/src/router/deployGuard.ts::deployRouteGuard`
- `apps/desktop/src/components/molecules/DeployButton.vue`
- `apps/desktop/src/pages/AddGameServer.vue` (écran d'entrée, route `services-add`)

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
- 2026-11-16 — révisée et renommée (HRT-243) : bouton « Ajouter un serveur de jeu », écran des trois chemins gardé.
