---
id: BR-RES-032
domaine: RES
titre: Trois crans se déduisent de la mémoire minimale et recommandée de la fiche, trois se déclarent, strictement croissants au-dessus de la recommandée
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-paliers-de-memoire.md (BR-RES-032), HRT-122
maj: 2026-10-17
---

# BR-RES-032 — Trois crans se déduisent de la mémoire minimale et recommandée de la fiche, trois se déclarent, strictement croissants au-dessus de la recommandée

## Règle
« Recommandé » commence à `recommended_bytes`. « Lent » va de `min_bytes` à `recommended_bytes`. « Instable » est tout ce qui est sous `min_bytes`. Ces trois ne s'écrivent pas dans une fiche (`CranNonDeclarable`). « Fluide », « Ultra » et « Communautaire » se déclarent par `[[resources.memory_tier]]` (zéro à trois entrées) avec un `from_bytes` strictement croissant dans l'ordre des crans et au-dessus de la mémoire recommandée ; sans mémoire recommandée, aucun cran ne se place. Identifiant inconnu, en double, ou champ de texte dans un cran : refus typé. `[resources.tiers] hidden` ne cite que les six identifiants fermés.

## Application (code)
- `crates/hearth-agent/src/domain/catalog/loading/invariants.rs::check_memory_tiers`
- `crates/hearth-agent/src/domain/catalog/loading/schema/fiche.rs::MemoryTier`
- `crates/hearth-agent/src/domain/catalog/loading/memory.rs::TierId`

## Vérification
- `crates/hearth-agent/src/domain/catalog/tests_memory.rs::declared_tiers_with_a_bad_identifier_are_refused`
- `crates/hearth-agent/src/domain/catalog/tests_memory.rs::from_bytes_must_rise_strictly_above_the_recommended_memory`
- `crates/hearth-agent/src/domain/catalog/tests_memory.rs::a_tier_has_no_free_text_field`
- `crates/hearth-agent/src/domain/catalog/tests_memory.rs::hidden_lists_the_six_identifiers_only_once_each`

## Cas limites
- Les crans sont lus dans la fiche avec laquelle le service a été installé (BR-RES-036) : client et réponse du catalogue, pas ce ticket.

## Règles liées
- BR-RES-031, BR-RES-033

## Historique
- 2026-10-17 — création (HRT-122, domaine pur : fiche et chargeur ; le curseur du client et la réponse du catalogue suivent).
