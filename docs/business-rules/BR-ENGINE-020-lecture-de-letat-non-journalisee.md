---
id: BR-ENGINE-020
domaine: ENGINE
titre: Lire l'état du moteur n'est pas journalisé
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-moteur-de-conteneurs.md (BR-ENGINE-020), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §5, §7, §9, HRT-61
maj: 2026-10-13
---

# BR-ENGINE-020 — Lire l'état du moteur n'est pas journalisé

## Règle
Lire l'état du moteur est une consultation : elle n'est pas inscrite au journal d'activité. `GET /services`, `POST /engine/recheck` et l'abonnement au sujet `services` n'ont pas d'action de journal et n'écrivent rien en base.

## Application (code)
- `crates/hearth-agent/src/entrypoint/http/mod.rs::ENDPOINTS (`audit: None`, `READ_ONLY_POSTS`)`

## Vérification
- `crates/hearth-agent/tests/services_https.rs::reading_the_engine_and_the_services_writes_nothing_to_the_journal`
- `crates/hearth-agent/tests/http_api.rs::every_modifying_route_leaves_exactly_one_success_entry (balayage des lectures : aucune entrée)`

## Règles liées
- BR-SVC-024

## Historique
- 2026-10-13 — création (HRT-61, tranche 1 des déploiements : moteur détecté et liste en lecture seule).
