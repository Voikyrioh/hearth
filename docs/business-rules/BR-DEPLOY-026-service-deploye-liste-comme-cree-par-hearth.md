---
id: BR-DEPLOY-026
domaine: DEPLOY
titre: Le service déployé est listé comme un service créé par Hearth, avec ses ports, ses variables (noms) et sa commande
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-deployer-image.md (BR-DEPLOY-026), HRT-68
maj: 2026-10-17
---

# BR-DEPLOY-026 — Le service déployé est listé comme un service créé par Hearth, avec ses ports, ses variables (noms) et sa commande

## Règle
Son conteneur est consigné (`service_containers`) à la création : seul l'identifiant consigné vaut preuve d'origine, pas l'étiquette (ADR-0039). Au redémarrage de l'agent, les conteneurs consignés sont relus avant toute publication.

## Application (code)
- `crates/hearth-agent/src/application/services.rs (`hearth_ids`, `key_of`)`
- `crates/hearth-agent/src/infrastructure/sqlite/containers_repo.rs`

## Vérification
- `crates/hearth-agent/tests/service_deploy_https.rs::a_minimal_deployment_pulls_creates_starts_and_publishes_a_service_created_by_hearth`

## Cas limites
- Un conteneur étiqueté `io.hearth.service-id` mais non consigné reste un conteneur hors Hearth.

## Règles liées
BR-SVC-003, BR-DEPLOY-024

## Historique
- 2026-10-17 — création (HRT-68, déployer une image, côté agent).
