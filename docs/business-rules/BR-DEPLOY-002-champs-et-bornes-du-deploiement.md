---
id: BR-DEPLOY-002
domaine: DEPLOY
titre: Un déploiement a un nom, une image, une version, 0 à 100 variables, 0 à 50 correspondances de ports, 0 à 20 dossiers de données et une commande facultative
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-deployer-image.md (BR-DEPLOY-002), HRT-68
maj: 2026-10-17
---

# BR-DEPLOY-002 — Un déploiement a un nom, une image, une version, 0 à 100 variables, 0 à 50 correspondances de ports, 0 à 20 dossiers de données et une commande facultative

## Règle
Seuls le nom et l'image sont obligatoires (et, pour l'agent, les deux limites de ressources, BR-DEPLOY-038) : un déploiement minimal crée et démarre un service sans port ni dossier. Chaque borne est refusée avec son champ et une raison fixe (`VALIDATION_ERROR`, `details.field`, `details.reason`), jamais la valeur reçue.

## Application (code)
- `crates/hearth-agent/src/domain/services/deploy.rs::validate`
- `crates/hearth-proto/src/api/deploy.rs (`MAX_ENV_VARS`, `MAX_PORT_MAPPINGS`, `MAX_DEPLOY_MOUNTS`)`

## Vérification
- `crates/hearth-agent/src/domain/services/deploy.rs::tests::a_minimal_deployment_has_a_name_an_image_and_limits`
- `crates/hearth-agent/tests/service_deploy_https.rs::a_bad_request_is_refused_on_its_field_and_nothing_is_created`

## Cas limites
- Une borne dépassée est refusée entière : rien n'est tronqué ni corrigé en silence.

## Règles liées
BR-DEPLOY-003, BR-DEPLOY-005, BR-DEPLOY-006, BR-DEPLOY-007, BR-DEPLOY-014, BR-DEPLOY-038

## Historique
- 2026-10-17 — création (HRT-68, déployer une image, côté agent).
