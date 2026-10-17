---
id: BR-DEPLOY-037
domaine: DEPLOY
titre: Le déploiement écrit la description durable (révision 1) à l'acceptation, avant de créer le conteneur
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-modifier-un-service.md (BR-DEPLOY-037), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 0, 0 bis et 6), HRT-83
maj: 2026-10-13
---

# BR-DEPLOY-037 — Le déploiement écrit la description durable (révision 1) à l'acceptation, avant de créer le conteneur

## Règle
La description est écrite à l'acceptation du déploiement (révision 1, secrets scellés), puis le conteneur est créé À PARTIR D'ELLE (relue en base, plan fermé construit par `plan_from_spec`) ; un échec avant le lancement la retire (`forget`), une suppression l'efface. Le déploiement (HRT-68) est branché : écriture de la description en séquence avec l'acceptation de l'opération (deux écritures, pas une transaction ; un agent qui s'arrête entre les deux laisse une description que le démarrage retire, BR-DEPLOY-019).

## Application (code)
- `crates/hearth-agent/src/application/service_specs.rs::ServiceSpecs::{save, forget}`
- `crates/hearth-agent/src/domain/ops/kind.rs::OpKind::Deploy`
- `crates/hearth-agent/src/application/deployments.rs::Deployments::{deploy, plan_of}`

## Vérification
- `crates/hearth-agent/tests/service_spec_secrets.rs::forgetting_a_service_erases_its_sealed_values`
- `crates/hearth-agent/tests/service_spec_secrets.rs::a_description_is_numbered_and_the_previous_revision_stays_readable`
- `crates/hearth-agent/tests/service_deploy_https.rs::a_minimal_deployment_pulls_creates_starts_and_publishes_a_service_created_by_hearth`

## Cas limites
- Un échec avant le lancement ne laisse aucune description (HRT-68).
- Une clé des secrets illisible refuse le déploiement (rien n'est créé).

## Règles liées
- BR-EDIT-035, BR-EDIT-034, BR-DEL-004, BR-DEPLOY-019

## Historique
- 2026-10-17 — branché par HRT-68 (`Deployments::deploy`, relecture avant création).
- 2026-10-13 — création (HRT-83, ADR-0044 et ADR-0045 : socle des services créés par Hearth, côté agent).
