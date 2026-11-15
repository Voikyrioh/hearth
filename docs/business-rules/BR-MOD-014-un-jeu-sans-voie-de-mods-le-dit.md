---
id: BR-MOD-014
domaine: MOD
titre: Un jeu dont la fiche ne déclare aucune voie de mods ne fait pas semblant : il le dit
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-mods-et-packs.md (BR-MOD-014), HRT-124
maj: 2026-11-15
---

# BR-MOD-014 : Un jeu dont la fiche ne déclare aucune voie de mods ne fait pas semblant : il le dit

## Règle
Une fiche sans bloc `[mods]` n'offre aucune voie de mods : elle se charge comme avant, `GET /catalog/{id}` ne porte pas de membre `mods`, et un pack joint à son installation est refusé (`422 VALIDATION_ERROR`, `field = "source.pack"`, `reason = "not_offered"`) sans rien ranger. Le client dit alors « Hearth ne gère pas les mods de ce jeu. Tu peux déposer tes fichiers toi-même dans l'onglet Fichiers. » (HRT-125).

## Application (code)
- `crates/hearth-agent/src/domain/catalog/loading/schema/fiche.rs::FicheFile::mods` (facultatif)
- `crates/hearth-agent/src/domain/catalog/translate.rs::{translate, TranslateError::PackNotOffered}`
- `crates/hearth-agent/src/application/catalog.rs::mods_view`

## Vérification
- `crates/hearth-agent/src/domain/catalog/tests_mods.rs` : `a_fiche_without_a_mods_block_loads_as_before`
- `crates/hearth-agent/tests/pack_install_https.rs` : `an_install_with_a_pack_is_refused_whole_when_the_pack_is_refused_or_unknown_or_not_offered`

## Cas limites
- Un jeu qui n'a pas de fiche du tout s'installe comme un service ordinaire (déployer une image).

## Règles liées
- BR-MOD-001, BR-MOD-007

## Historique
- 2026-11-15 : création (HRT-124).
