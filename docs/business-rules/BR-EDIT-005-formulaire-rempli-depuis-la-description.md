---
id: BR-EDIT-005
domaine: EDIT
titre: `GET /services/{key}/spec` rend la description (noms, ports, dossiers, version), sa révision et l'écart avec l
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-modifier-un-service.md (BR-EDIT-005), HRT-87
maj: 2026-11-14
---

# BR-EDIT-005 — `GET /services/{key}/spec` rend la description (noms, ports, dossiers, version), sa révision et l'écart avec l

## Règle
`GET /services/{key}/spec` rend la description (noms, ports, dossiers, version), sa révision et l'écart avec le moteur, lus sans que le moteur soit prêt.

## Application (code)
- `crates/hearth-agent/src/application/service_update.rs::ServiceUpdates::view`

## Vérification
- `crates/hearth-agent/tests/service_update_https.rs::reading_the_description_never_shows_a_value_nor_the_command`

## Cas limites
- Les textes affichés et les boutons sont ceux du client (HRT-88).

## Règles liées
- BR-EDIT-035, BR-EDIT-024

## Historique
- 2026-11-14 — création (HRT-87, modifier un service par recréation, côté agent).
