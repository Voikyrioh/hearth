---
id: BR-ADOPT-021
domaine: ADOPT
titre: Il n'existe aucune activation globale : on ne peut pas décider que « tout ce qui existe » ou « tout ce qui apparaîtra » est géré
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-adopter-un-service.md (BR-ADOPT-021), docs/adr/ADR-0043-par-defaut-hearth-ne-gere-que-ses-services-un-conteneur-hors-hearth-s-adopte.md, HRT-80
maj: 2026-10-13
---

# BR-ADOPT-021 — Il n'existe aucune activation globale : on ne peut pas décider que « tout ce qui existe » ou « tout ce qui apparaîtra » est géré

## Règle
Aucune route, aucune option de configuration et aucun réglage n'adopte « tout » : l'acte d'adoption ne prend que des clés explicites (50 au plus) dont le nombre et l'empreinte sont signés. Un conteneur qui apparaît après une adoption reste non géré (compté) tant qu'on ne l'a pas coché lui-même. Le test de classement des routes (`CLASSES`) fait de toute route nouvelle sous un préfixe de service un point de revue.

## Application (code)
- `crates/hearth-agent/src/entrypoint/http/mod.rs::ENDPOINTS`
- `crates/hearth-agent/src/application/service_adoption.rs::ServiceAdoption::adopt`

## Vérification
- `crates/hearth-agent/tests/service_adoption_https.rs::adopting_changes_nothing_on_the_service_publishes_it_and_journals_one_entry_per_service`
- `crates/hearth-agent/src/entrypoint/http/mod.rs::tests::every_route_is_classified_and_every_service_route_is_at_the_services_level`

## Règles liées
- BR-ADOPT-007, BR-ADOPT-008

## Historique
- 2026-10-13 — création (HRT-80, ADR-0043 : l'acte d'adopter et de relâcher, moitié agent et protocole).
