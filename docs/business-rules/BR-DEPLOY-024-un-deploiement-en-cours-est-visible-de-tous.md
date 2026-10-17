---
id: BR-DEPLOY-024
domaine: DEPLOY
titre: Un déploiement en cours apparaît dans la liste des services pour tous les comptes (« Déploiement en cours »), avec son nom, son image et sa progression, sans ses variables ni sa commande
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-deployer-image.md (BR-DEPLOY-024), HRT-68
maj: 2026-10-17
---

# BR-DEPLOY-024 — Un déploiement en cours apparaît dans la liste des services pour tous les comptes (« Déploiement en cours »), avec son nom, son image et sa progression, sans ses variables ni sa commande

## Règle
Le registre tient une ligne `deploying` pour la clé `h_` du futur service tant que son conteneur n'existe pas ; la progression et l'annulation se lisent dans l'opération (`busy` du service). Fermer le formulaire ne l'arrête pas.

## Application (code)
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::{begin_deploying, end_deploying}`

## Vérification
- `crates/hearth-agent/tests/service_deploy_https.rs::a_deployment_in_progress_is_listed_for_everyone_without_its_variables_then_becomes_the_service`

## Cas limites
- Aucun cas particulier connu.

## Règles liées
BR-DEPLOY-026

## Historique
- 2026-10-17 — création (HRT-68, déployer une image, côté agent).
