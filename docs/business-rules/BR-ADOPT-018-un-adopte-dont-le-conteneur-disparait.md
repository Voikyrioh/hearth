---
id: BR-ADOPT-018
domaine: ADOPT
titre: Un service adopté dont le conteneur disparaît reste listé s'il est lancé par une unité du système, et disparaît de la liste sans notification sinon
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-adopter-un-service.md (BR-ADOPT-018), docs/adr/ADR-0043-par-defaut-hearth-ne-gere-que-ses-services-un-conteneur-hors-hearth-s-adopte.md, HRT-79
maj: 2026-10-13
---

# BR-ADOPT-018 — Un service adopté dont le conteneur disparaît reste listé s'il est lancé par une unité du système, et disparaît de la liste sans notification sinon

## Règle
Un service adopté dont le conteneur disparaît reste listé (« Arrêté » ou « En erreur » selon l'unité, avec ses derniers réglages connus, sans variable ni commande) s'il est lancé par une unité du système ; il disparaît de la liste, sans notification, s'il n'avait pas d'unité (`BR-ADOPT-017`). Le service retenu d'une unité adoptée est celui de `supervised_services` ; l'unité que le système ne connaît plus est oubliée avec son souvenir (`BR-SVC-016`).

## Application (code)
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::{build_views, poll_units}`

## Vérification
- `crates/hearth-agent/tests/services_https.rs::a_service_run_by_a_unit_is_served_under_its_unit_and_stays_listed_when_its_container_goes`
- `crates/hearth-agent/src/application/services.rs::tests::an_adoption_by_unit_survives_the_recreation_of_the_container_and_a_restart_of_the_agent`

## Cas limites
- Le détail grisé « Ce service n'existe plus. », l'échec de démarrage « Le système du serveur ne connaît plus l'unité » et « Ne plus gérer ce service » appartiennent au client et aux actes (HRT-80 et suivants).

## Règles liées
- BR-ADOPT-017, BR-SVC-016, BR-SVC-001

## Historique
- 2026-10-13 — création (HRT-79, ADR-0043 : ce que l'agent publie, moitié agent, première partie).
