---
id: BR-MONDE-024
domaine: MONDE
titre: Un compte sans le droit de gérer les services ne voit ni la liste des mondes ni l'inspection, et le serveur refuse sa demande même d'un client modifié
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-reprendre-un-monde-existant.md (BR-MONDE-024), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (section 9.9), HRT-105
maj: 2026-11-14
---

# BR-MONDE-024 — Un compte sans le droit de gérer les services ne voit ni la liste des mondes ni l'inspection, et le serveur refuse sa demande même d'un client modifié

## Règle
`GET /catalog/{id}/worlds`, `POST /catalog/{id}/worlds/inspect` et l'installation avec un monde sont de niveau `Services` : le refus faute de droit est consigné au journal d'activité (action `service.world.import`).

## Application (code)
- `crates/hearth-agent/src/entrypoint/http/mod.rs::ENDPOINTS`
- `crates/hearth-agent/src/entrypoint/http/catalog.rs::{worlds, inspect_world}`

## Vérification
- `crates/hearth-agent/tests/world_import_https.rs::an_account_without_the_right_sees_neither_the_list_nor_the_inspection`
- `crates/hearth-agent/src/entrypoint/http/mod.rs::tests::every_route_is_classified_and_every_service_route_is_at_the_services_level`

## Cas limites
- Aucun.

## Règles liées
- BR-MONDE-003

## Historique
- 2026-11-14 — création (HRT-105, côté agent).
