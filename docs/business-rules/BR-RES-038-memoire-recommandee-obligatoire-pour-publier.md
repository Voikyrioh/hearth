---
id: BR-RES-038
domaine: RES
titre: Une fiche publiée porte toujours une mémoire recommandée mesurée ou publiée par l'éditeur
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-paliers-de-memoire.md (BR-RES-038), HRT-122
maj: 2026-10-17
---

# BR-RES-038 — Une fiche publiée porte toujours une mémoire recommandée mesurée ou publiée par l'éditeur

## Règle
Une fiche `published` a `memory.recommended_bytes` avec une preuve `measured` ou `publisher`. Un nombre mesuré au repos seul ne suffit pas (un plancher n'est pas une recommandation et n'est jamais multiplié). Sans cela le chargeur refuse la fiche publiée (`PublieeSansMemoireRecommandee`) ; une fiche `preview` reste en préparation, et `cargo xtask catalog-check --release` dit ce qui lui manque (mémoire recommandée absente, ou mesurée au repos seulement).

## Application (code)
- `crates/hearth-agent/src/domain/catalog/loading/memory.rs::recommended_gap`
- `crates/hearth-agent/src/domain/catalog/loading/invariants.rs::check_resources`
- `xtask/src/catalog_check.rs::check_texts`

## Vérification
- `crates/hearth-agent/src/domain/catalog/tests_memory.rs::a_published_fiche_needs_a_founded_recommended_memory`
- `crates/hearth-agent/src/domain/catalog/tests_memory.rs::a_preview_fiche_stays_loadable_without_a_recommended_memory`
- `xtask/src/catalog_check.rs::tests::the_release_profile_names_the_missing_recommended_memory`

## Cas limites
- Terraria reste `preview` : sa mémoire est relevée sur forge par HRT-149.

## Règles liées
- BR-FICHE-008, BR-RES-031

## Historique
- 2026-10-17 — création (HRT-122, domaine pur : fiche et chargeur ; le curseur du client et la réponse du catalogue suivent).
