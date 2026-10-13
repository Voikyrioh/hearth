---
id: BR-EDIT-035
domaine: EDIT
titre: Hearth garde pour chaque service qu'il crée une description durable numérotée : le voulu, jamais un état ni une mesure
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-modifier-un-service.md (BR-EDIT-035), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 0, 0 bis et 6), HRT-83
maj: 2026-10-13
---

# BR-EDIT-035 — Hearth garde pour chaque service qu'il crée une description durable numérotée : le voulu, jamais un état ni une mesure

## Règle
La description d'un service créé par Hearth (`h_…`) est écrite dans `service_specs` : nom, image et version, ports, montages, règle de redémarrage, délai d'arrêt, limites, réglages, fiche ; les variables et la commande sont scellées (BR-EDIT-034). Elle est numérotée : chaque modification validée est la révision suivante, la précédente reste lisible jusqu'à ce que l'ancien conteneur soit supprimé. Elle ne contient jamais un identifiant de conteneur, un état ni une mesure. La description dit ce qui est VOULU, le moteur ce qui est OBSERVÉ ; l'écart est calculé par le domaine (BR-EDIT-036) et affiché, jamais corrigé seul. Une écriture dit la révision qu'elle a lue : si une autre est passée entre-temps, elle est refusée (BR-EDIT-024). Deux services du même nom (à la casse près) ne coexistent pas. Un service adopté n'a pas de description.

Précision par rapport à la spec fonctionnelle (« noms en clair ») : les NOMS de variables sont scellés eux aussi, conformément à la contrainte du ticket (jamais de nom de variable en clair en base) ; un champ scellé est repéré par son rang (`env:3`), pas par un nom.

## Application (code)
- `crates/hearth-agent/src/domain/services/spec.rs::{SpecBody, SpecSecrets, fold_name}`
- `crates/hearth-agent/src/application/service_specs.rs::ServiceSpecs::{save, read, discard_before}`
- `crates/hearth-agent/src/infrastructure/sqlite/specs_repo.rs::SqliteSpecRepo`
- `crates/hearth-agent/migrations/0012_service_specs_and_ops.sql`

## Vérification
- `crates/hearth-agent/tests/service_spec_secrets.rs::a_description_is_numbered_and_the_previous_revision_stays_readable`
- `crates/hearth-agent/tests/service_spec_secrets.rs::a_concurrent_modification_and_a_taken_name_are_refused_not_overwritten`
- `crates/hearth-agent/tests/migration_0012.rs::one_current_revision_per_service_and_one_current_service_per_name`
- `crates/hearth-agent/src/domain/services/spec.rs::tests::the_body_has_no_room_for_a_container_id_a_state_or_a_measure`

## Cas limites
- La route `GET/PUT /services/{key}/spec` et le formulaire ne sont pas dans ce ticket (HRT-87, HRT-88).
- Un champ ajouté par une version plus récente est ignoré à la lecture ; un champ facultatif absent prend son défaut.

## Règles liées
- BR-EDIT-034, BR-EDIT-036, BR-EDIT-024, BR-DEPLOY-037, BR-DEL-004

## Historique
- 2026-10-13 — création (HRT-83, ADR-0044 et ADR-0045 : socle des services créés par Hearth, côté agent).
