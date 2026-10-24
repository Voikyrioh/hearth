---
id: BR-DEPLOY-031
domaine: DEPLOY
titre: Un port relié est joignable par tout appareil qui peut joindre le serveur ; le formulaire et le récapitulatif le disent
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-deployer-image.md (BR-DEPLOY-031), HRT-69
maj: 2026-10-24
---

# BR-DEPLOY-031 — Un port relié est joignable par tout appareil qui peut joindre le serveur ; le formulaire et le récapitulatif le disent

## Règle
Dès qu'une correspondance de ports existe, la section Ports du formulaire écrit en permanence « Ce port sera accessible à tout appareil qui peut joindre votre serveur. », et le récapitulatif la répète sous la liste des ports. Le message n'est pas fermable et ne dépend d'aucune réponse du serveur. Un port sous 1024 avertit en plus (`BR-DEPLOY-007`).

## Application (code)
- `apps/desktop/src/components/organisms/PortMappingRows.vue (data-ports-exposed)`
- `apps/desktop/src/components/organisms/DeploySummaryDialog.vue (data-sum-exposed)`

## Vérification
- `apps/desktop/src/pages/DeployService.test.ts (les ports : dit que tout appareil…)`
- `apps/desktop/src/pages/DeployFlow.test.ts (le récapitulatif)`

## Règles liées
BR-DEPLOY-007, BR-DEPLOY-015, BR-DEPLOY-036

## Historique
- 2026-10-24 — création (HRT-69, déployer une image, côté client).
