---
id: BR-ADOPT-016
domaine: ADOPT
titre: Pour un service créé par Hearth rien ne change : commande et noms de variables restent lisibles de tous les comptes, les valeurs sont réservées au droit
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-adopter-un-service.md (BR-ADOPT-016), docs/adr/ADR-0043-par-defaut-hearth-ne-gere-que-ses-services-un-conteneur-hors-hearth-s-adopte.md, HRT-79
maj: 2026-10-13
---

# BR-ADOPT-016 — Pour un service créé par Hearth rien ne change : commande et noms de variables restent lisibles de tous les comptes, les valeurs sont réservées au droit

## Règle
Pour un service créé par Hearth (clé `h_`), le créateur du service en a choisi le contenu : la commande et les noms de variables restent lisibles de tous les comptes. Seules les valeurs restent réservées au droit. Un service `h_` est publié de droit, sans adoption (la base refuse d'ailleurs qu'on en adopte un).

Aujourd'hui l'agent ne produit pas encore de clé `h_` (le déploiement est un autre ticket) : la règle est tenue et prouvée au niveau du filtre (`visibility_of`) et de l'écriture du message (`service_item`), pas par un service créé de bout en bout.

## Application (code)
- `crates/hearth-agent/src/domain/services/visibility.rs::{visibility_of, may_read_details}`
- `crates/hearth-agent/src/entrypoint/services_wire.rs::service_item`

## Vérification
- `crates/hearth-agent/src/entrypoint/services_wire.rs::tests::a_service_made_by_hearth_keeps_its_details_for_every_account`
- `crates/hearth-agent/src/domain/services/visibility.rs::tests::a_service_made_by_hearth_is_published_without_being_adopted`
- `crates/hearth-agent/tests/migration_0011.rs::an_adoption_is_by_unit_or_by_container_never_by_name_and_never_for_a_service_made_by_hearth`

## Cas limites
- Le test de bout en bout d'un service `h_` (déployer, lire sa commande avec un compte sans le droit) viendra avec le déploiement.

## Règles liées
- BR-ADOPT-015, BR-SVC-007, BR-SVC-008

## Historique
- 2026-10-13 — création (HRT-79, ADR-0043 : ce que l'agent publie, moitié agent, première partie).
