---
id: BR-DEL-021
domaine: DEL
titre: Une seule action à la fois par service, tous comptes confondus
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-supprimer-service.md (BR-DEL-021), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md §5 et §9, HRT-85
maj: 2026-10-24
---

# BR-DEL-021 : Une seule action à la fois par service, tous comptes confondus

## Règle
Une suppression prend le verrou exclusif du service (BR-LIFE-013). Une demande qui arrive pendant une autre action reçoit `409 SERVICE_BUSY` avec le compte et le genre de l'action en cours. Une demande qui arrive après la fin d'une suppression reçoit `404` « Ce service n'existe plus. ».

## Application (code)
- `crates/hearth-agent/src/application/delete.rs::ServiceDeletion::{delete, busy_of}`

## Vérification
- `crates/hearth-agent/tests/service_delete_https.rs::{the_data_goes_only_after_the_container_while_the_name_is_reserved_and_the_service_busy, deleting_twice_in_a_row_the_second_finds_nothing}`

## Cas limites
- Aucun.

## Règles liées
- BR-LIFE-013
- BR-DEL-019

## Historique
- 2026-10-24 : création (HRT-85, côté agent).
