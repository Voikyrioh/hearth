---
id: BR-MONDE-039
domaine: MONDE
titre: La fiche déclare ce qu'est un monde : un fichier (suffixe, compagnons) ou un dossier (profondeur), le réglage du nom et les réglages portés
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-reprendre-un-monde-existant.md (BR-MONDE-039), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (section 9.9), HRT-105
maj: 2026-11-14
---

# BR-MONDE-039 — La fiche déclare ce qu'est un monde : un fichier (suffixe, compagnons) ou un dossier (profondeur), le réglage du nom et les réglages portés

## Règle
`WorldRules::from_fiche` lit la section `[worlds]` de la fiche ; la même définition sert la recherche, l'inspection et la copie. Une fiche qui ne la déclare pas n'a pas les choix de reprise (`not_supported`). Des chemins liés (`also`) ne sont pas repris par cette version.

## Application (code)
- `crates/hearth-agent/src/domain/worlds/mod.rs::WorldRules`

## Vérification
- `crates/hearth-agent/src/domain/worlds/mod.rs::tests::terraria_declares_a_file_world_in_its_world_folder`
- `crates/hearth-agent/src/domain/worlds/mod.rs::tests::a_file_world_is_a_file_with_the_suffix_and_a_non_empty_name`

## Cas limites
- Aucune fiche ne déclare encore un monde « dossier ».

## Règles liées
- BR-MONDE-003, BR-MONDE-014

## Historique
- 2026-11-14 — création (HRT-105, côté agent).
