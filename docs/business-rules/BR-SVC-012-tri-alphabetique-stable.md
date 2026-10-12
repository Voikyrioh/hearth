---
id: BR-SVC-012
domaine: SVC
titre: Liste triée par nom sans tenir compte de la casse
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-voir-les-services.md (BR-SVC-012), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §5, §7, §9, HRT-61
maj: 2026-10-13
---

# BR-SVC-012 — Liste triée par nom sans tenir compte de la casse

## Règle
La liste est triée par nom, ordre alphabétique sans tenir compte de la casse ; l'ordre ne dépend pas de l'état. L'agent trie l'instantané et chaque lot (à égalité de nom, le nom exact puis la clé départagent).

## Application (code)
- `crates/hearth-agent/src/domain/services/state.rs::compare_names`
- `crates/hearth-agent/src/application/services.rs::sort_views`

## Vérification
- `crates/hearth-agent/src/domain/services/state.rs::tests::names_sort_without_regard_to_case_or_state`
- `crates/hearth-agent/tests/services_https.rs::both_roles_get_the_same_engine_and_services_sorted_by_name_stopped_ones_included`

## Historique
- 2026-10-13 — création (HRT-61, tranche 1 des déploiements : moteur détecté et liste en lecture seule).
