---
id: BR-DEPLOY-036
domaine: DEPLOY
titre: Le formulaire et le récapitulatif ne disent jamais d'un service déployé qu'il est « isolé » ou « sûr »
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-deployer-image.md (BR-DEPLOY-036), HRT-69
maj: 2026-10-24
---

# BR-DEPLOY-036 — Le formulaire et le récapitulatif ne disent jamais d'un service déployé qu'il est « isolé » ou « sûr »

## Règle
Aucun texte de déploiement (formulaire, récapitulatif, progression, notifications) ne présente un service déployé comme isolé, sûr ou sécurisé : le droit de gérer les services n'est jamais présenté comme sans risque (`BR-GRANT-022`). Les textes disent ce que Hearth fait (limites de mémoire et de processeur, ports joignables par tout appareil du réseau), jamais une promesse de sécurité.

## Application (code)
- `apps/desktop/src/i18n/fr.ts (famille `deploy.*`)`

## Vérification
- `apps/desktop/src/deploy/copy.test.ts (aucun ne contient « isolé ou sûr »)`

## Règles liées
BR-DEPLOY-031, BR-GRANT-022

## Historique
- 2026-10-24 — création (HRT-69, déployer une image, côté client).
