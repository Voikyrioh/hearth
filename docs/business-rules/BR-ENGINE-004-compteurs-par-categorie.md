---
id: BR-ENGINE-004
domaine: ENGINE
titre: Compteurs de services par catégorie, calculés par l'interface
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-moteur-de-conteneurs.md (BR-ENGINE-004), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §5, §7, §9, HRT-61
maj: 2026-10-13
---

# BR-ENGINE-004 — Compteurs de services par catégorie, calculés par l'interface

## Règle
Les compteurs « en marche », « arrêtés », « en erreur » (et « en transition », « autres » s'ils ne sont pas vides) sont calculés par l'interface à partir de l'état de chaque service : l'agent ne compte rien et n'envoie aucun compteur. Il garantit seulement que l'état de chaque service est un des neuf états fermés de BR-SVC-004, ce qui rend le total égal au nombre de lignes. Côté client (affichage, textes, boutons) : porté par HRT-62, hors de cette tranche.

## Application (code)
- `crates/hearth-proto/src/api/services.rs::ServiceState`
- `crates/hearth-agent/src/domain/services/state.rs::derive_state`

## Vérification
- `crates/hearth-agent/src/domain/services/state.rs::tests::every_other_run_state_has_its_service_state`

## Règles liées
- BR-SVC-004

## Historique
- 2026-10-13 — création (HRT-61, tranche 1 des déploiements : moteur détecté et liste en lecture seule).
