---
id: BR-ENGINE-016
domaine: ENGINE
titre: Les deux rôles voient exactement le même état du moteur
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-moteur-de-conteneurs.md (BR-ENGINE-016), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §5, §7, §9, HRT-61
maj: 2026-10-13
---

# BR-ENGINE-016 — Les deux rôles voient exactement le même état du moteur

## Règle
Les deux rôles voient exactement le même état du moteur : `GET /services`, `POST /engine/recheck` et le sujet `services` sont ouverts à tout compte authentifié et rendent le même contenu. Seul l'affichage des blocs de commande à recopier est réservé aux administrateurs, côté client (HRT-62).

## Application (code)
- `crates/hearth-agent/src/entrypoint/http/mod.rs::ENDPOINTS (`Access::Authenticated`)`
- `crates/hearth-agent/src/entrypoint/http/services.rs::{list, recheck}`
- `crates/hearth-agent/src/entrypoint/ws/connection.rs (sujet `services` sans contrôle de rôle)`

## Vérification
- `crates/hearth-agent/tests/services_https.rs::both_roles_get_the_same_engine_and_services_sorted_by_name_stopped_ones_included`
- `crates/hearth-agent/tests/http_api.rs::authenticated_routes_let_a_read_only_account_through_the_guard (balayage des routes)`

## Règles liées
- BR-SVC-020

## Historique
- 2026-10-13 — création (HRT-61, tranche 1 des déploiements : moteur détecté et liste en lecture seule).
