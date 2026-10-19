---
id: BR-RES-031
domaine: RES
titre: Un cran sans chiffre fondé, ou caché par la fiche, n'est jamais servi ; aucun chiffre de cran n'est calculé à partir d'un autre
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-paliers-de-memoire.md (BR-RES-031), HRT-122
maj: 2026-10-17
---

# BR-RES-031 — Un cran sans chiffre fondé, ou caché par la fiche, n'est jamais servi ; aucun chiffre de cran n'est calculé à partir d'un autre

## Règle
Hearth ne sert que les crans de mémoire auxquels la fiche donne un chiffre fondé. « Instable » n'existe que si la mémoire minimale a été mesurée (mesurée ou mesurée au repos) ; « Lent » que si la fiche a une mémoire minimale ; « Recommandé » que si la mémoire recommandée est fondée (BR-RES-038) ; « Fluide », « Ultra » et « Communautaire » que si la fiche les déclare. Un cran listé dans `[resources.tiers] hidden` n'est jamais servi, même avec un chiffre. Aucun chiffre n'est obtenu en multipliant ou en divisant un autre : tout nombre servi est écrit dans la fiche.

## Application (code)
- `crates/hearth-agent/src/domain/catalog/loading/memory.rs::memory_tiers`
- `crates/hearth-agent/src/domain/catalog/loading/model.rs::Fiche::memory_tiers`

## Vérification
- `crates/hearth-agent/src/domain/catalog/tests_memory.rs::unstable_needs_a_measured_minimum_and_slow_needs_a_minimum`
- `crates/hearth-agent/src/domain/catalog/tests_memory.rs::hidden_tiers_are_never_served_even_with_a_number`
- `crates/hearth-agent/src/domain/catalog/tests_memory.rs::no_tier_figure_is_ever_derived_from_another`
- `crates/hearth-agent/src/domain/catalog/tests_memory.rs::only_the_recommended_tier_is_served_when_only_it_is_declared`

## Cas limites
- Une fiche sans bloc de crans se charge comme avant ; elle ne sert aucun cran tant que sa mémoire n'a pas de chiffre fondé.
- « Recommandé » avec une mesure au repos n'est pas servi (le chiffre est un plancher).

## Règles liées
- BR-RES-032, BR-RES-033, BR-RES-038

## Historique
- 2026-10-17 — création (HRT-122, domaine pur : fiche et chargeur ; la réponse du catalogue suit avec HRT-181, le curseur du client avec HRT-123).
