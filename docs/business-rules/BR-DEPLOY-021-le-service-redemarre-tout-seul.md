---
id: BR-DEPLOY-021
domaine: DEPLOY
titre: Un service déployé redémarre tout seul quand le serveur ou l'agent redémarre, sauf si un compte l'a arrêté depuis Hearth
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-deployer-image.md (BR-DEPLOY-021), HRT-68
maj: 2026-10-17
---

# BR-DEPLOY-021 — Un service déployé redémarre tout seul quand le serveur ou l'agent redémarre, sauf si un compte l'a arrêté depuis Hearth

## Règle
Le conteneur est créé avec `RestartPolicy = unless-stopped` : le moteur relance le service au démarrage du serveur et retient lui-même un arrêt manuel. Aucune réconciliation par l'agent (ADR-0039). Cela suppose que le moteur démarre avec le serveur.

## Application (code)
- `crates/hearth-agent/src/infrastructure/engine/bollard.rs::create_body`
- `crates/hearth-agent/src/domain/services/deploy.rs::validate (`SpecRestart::UnlessStopped`)`

## Vérification
- `crates/hearth-agent/tests/service_deploy_https.rs::a_minimal_deployment_pulls_creates_starts_and_publishes_a_service_created_by_hearth`
- `crates/hearth-agent/tests/engine_real.rs::an_unless_stopped_service_comes_back_after_the_daemon_restarts_unless_it_was_stopped (CI)`

## Cas limites
- Un nettoyage automatique des conteneurs arrêtés supprimerait un service arrêté.

## Règles liées
BR-LIFE-010, BR-DEPLOY-020

## Historique
- 2026-10-17 — création (HRT-68, déployer une image, côté agent).
