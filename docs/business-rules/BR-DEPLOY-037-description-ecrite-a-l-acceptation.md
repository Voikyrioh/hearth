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
La description est écrite à l'acceptation du déploiement, puis le conteneur est créé à partir d'elle ; un échec avant le lancement la retire (`forget`), une suppression l'efface. Cette tranche fournit les opérations (`ServiceSpecs::save` pour la révision 1, `forget`) et le genre d'opération `deploy` ; leur branchement sur le déploiement est HRT-68 (partiel ici, par conception : HRT-83 passe AVANT HRT-68).

## Application (code)
- `crates/hearth-agent/src/application/service_specs.rs::ServiceSpecs::{save, forget}`
- `crates/hearth-agent/src/domain/ops/kind.rs::OpKind::Deploy`

## Vérification
- `crates/hearth-agent/tests/service_spec_secrets.rs::forgetting_a_service_erases_its_sealed_values`
- `crates/hearth-agent/tests/service_spec_secrets.rs::a_description_is_numbered_and_the_previous_revision_stays_readable`

## Cas limites
- Un échec avant le lancement ne laisse aucune description (HRT-68).

## Règles liées
- BR-EDIT-035, BR-EDIT-034, BR-DEL-004

## Historique
- 2026-10-13 — création (HRT-83, ADR-0044 et ADR-0045 : socle des services créés par Hearth, côté agent).
