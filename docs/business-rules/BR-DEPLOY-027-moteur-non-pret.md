---
id: BR-DEPLOY-027
domaine: DEPLOY
titre: Quand le moteur n'est pas « Prêt », le déploiement est refusé avant tout (`503 ENGINE_UNAVAILABLE`)
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-deployer-image.md (BR-DEPLOY-027), HRT-68
maj: 2026-10-24
---

# BR-DEPLOY-027 — Quand le moteur n'est pas « Prêt », le déploiement est refusé avant tout (`503 ENGINE_UNAVAILABLE`)

## Règle
Rien n'est créé ni écrit ; la saisie du formulaire est conservée côté client.

## Application (code)
- `crates/hearth-agent/src/application/deployments.rs::Deployments::ready`
- `apps/desktop/src/pages/DeployService.vue` et `deploy/availability.ts::deployDoor` (client : le formulaire s'ouvre et explique l'état du moteur, le lancement est désactivé)

## Vérification
- `crates/hearth-agent/tests/service_deploy_https.rs::an_engine_that_is_not_ready_refuses_before_anything`
- `apps/desktop/src/pages/DeployService.test.ts (la route)`

## Cas limites
- Aucun cas particulier connu.

## Règles liées
BR-DEPLOY-028

## Historique
- 2026-10-17 — création (HRT-68, déployer une image, côté agent).
- 2026-10-24 — côté client (HRT-69, déployer une image).
