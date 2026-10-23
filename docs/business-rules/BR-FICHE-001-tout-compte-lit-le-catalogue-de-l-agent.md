---
id: BR-FICHE-001
domaine: FICHE
titre: Tout compte authentifié lit le catalogue des fiches de l'agent ; lire n'exige pas le droit de gérer les services et n'est pas journalisé
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-001), HRT-101
maj: 2026-10-24
---

# BR-FICHE-001 — Lire le catalogue

## Règle
`GET /catalog` et `GET /catalog/{id}` sont ouverts à tout compte authentifié, quel que soit son rôle, sans le droit de gérer les services, et ne sont pas inscrits au journal d'activité. Installer reste un acte du droit « gérer les services » (`POST /deployments`).

## Application (code)
- `crates/hearth-agent/src/entrypoint/http/mod.rs::ENDPOINTS (niveau Authenticated, audit None)`
- `crates/hearth-agent/src/entrypoint/http/catalog.rs::{list, detail}`

## Vérification
- `crates/hearth-agent/tests/catalog_https.rs::the_catalog_needs_a_session_and_says_what_the_agent_can_do`
- `crates/hearth-agent/src/entrypoint/http/mod.rs::every_route_is_classified_and_every_service_route_is_at_the_services_level`

## Règles liées
- BR-FICHE-002, BR-FICHE-012

## Historique
- 2026-10-24 — création (HRT-101, étape S8).
