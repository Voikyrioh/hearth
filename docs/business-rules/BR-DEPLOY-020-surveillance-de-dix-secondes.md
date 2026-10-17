---
id: BR-DEPLOY-020
domaine: DEPLOY
titre: Après le lancement, l'agent surveille le service pendant 10 secondes ; un arrêt immédiat est annoncé sans faire échouer le déploiement
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-deployer-image.md (BR-DEPLOY-020), HRT-68
maj: 2026-10-17
---

# BR-DEPLOY-020 — Après le lancement, l'agent surveille le service pendant 10 secondes ; un arrêt immédiat est annoncé sans faire échouer le déploiement

## Règle
Le service qui s'arrête (ou redémarre en boucle) dans la fenêtre laisse l'opération réussie avec la cause `stopped_right_after_start`, le service conservé dans la liste, une entrée de journal « arrêté juste après le démarrage ».

## Application (code)
- `crates/hearth-agent/src/application/deployments.rs::Deployments::watch`

## Vérification
- `crates/hearth-agent/tests/service_deploy_https.rs::a_program_that_stops_right_after_its_start_is_announced_without_failing_the_deployment`

## Cas limites
- Aucun cas particulier connu.

## Règles liées
BR-DEPLOY-021, BR-DEPLOY-025

## Historique
- 2026-10-17 — création (HRT-68, déployer une image, côté agent).
