---
id: BR-ADOPT-026
domaine: ADOPT
titre: Quand le moteur n'est pas prêt, la liste des candidats dit l'état du moteur et rien ne peut être adopté ; « ne plus gérer » reste possible
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-adopter-un-service.md (BR-ADOPT-026), docs/adr/ADR-0043-par-defaut-hearth-ne-gere-que-ses-services-un-conteneur-hors-hearth-s-adopte.md, HRT-80
maj: 2026-10-13
---

# BR-ADOPT-026 — Quand le moteur n'est pas prêt, la liste des candidats dit l'état du moteur et rien ne peut être adopté ; « ne plus gérer » reste possible

## Règle
`GET /services/external` répond `200` avec `engine` (état du moteur) et aucun candidat tant que l'état n'est pas `ready` ; `POST /services/adopt` répond `409 CONFLICT` avec `details.reason = engine_not_ready` et `details.engine` (un conteneur ne s'adopte que s'il est vu par le registre à cet instant). `POST /services/{key}/release` ne demande pas le moteur : c'est une décision de Hearth.

## Application (code)
- `crates/hearth-agent/src/application/service_adoption.rs::ServiceAdoption::{candidates, adopt}`
- `crates/hearth-agent/src/entrypoint/http/services.rs::adoption_error`

## Vérification
- `crates/hearth-agent/tests/service_adoption_https.rs::the_candidates_say_the_engine_state_when_it_is_not_ready_and_nothing_is_offered`

## Règles liées
- BR-ADOPT-013, BR-ADOPT-020

## Historique
- 2026-10-13 — création (HRT-80, ADR-0043 : l'acte d'adopter et de relâcher, moitié agent et protocole).
