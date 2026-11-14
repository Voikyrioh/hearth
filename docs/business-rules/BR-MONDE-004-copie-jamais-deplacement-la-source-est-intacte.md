---
id: BR-MONDE-004
domaine: MONDE
titre: Reprendre est une copie : rien n'est écrit, renommé, déplacé ni effacé dans la source, qu'elle soit lue en lecture seule ou non
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-reprendre-un-monde-existant.md (BR-MONDE-004), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (section 9.9), HRT-105
maj: 2026-11-14
---

# BR-MONDE-004 — Reprendre est une copie : rien n'est écrit, renommé, déplacé ni effacé dans la source, qu'elle soit lue en lecture seule ou non

## Règle
La source est un `WorldSource`, un type qui n'a aucune méthode d'écriture. L'adaptateur ouvre le dossier ouvert en lecture seule, descend par `openat(O_NOFOLLOW)` et ne suit aucun lien. Après une reprise réussie, échouée ou annulée, le contenu, les noms, les droits et les dates de la source sont identiques.

## Application (code)
- `crates/hearth-agent/src/application/ports/world_fs.rs::WorldSource`
- `crates/hearth-agent/src/infrastructure/world_fs/unix.rs::UnixSource`
- `crates/hearth-agent/src/application/world_import/copy.rs::copy_world`

## Vérification
- `crates/hearth-agent/src/infrastructure/world_fs/unix.rs::tests::a_read_only_source_is_copied_verified_and_placed_without_a_change_to_the_source`
- `crates/hearth-agent/tests/world_import_https.rs::a_world_is_copied_verified_placed_and_loaded_by_name_and_the_source_is_untouched`
- `crates/hearth-agent/tests/world_import_https.rs::cancelling_during_the_copy_removes_the_import_and_creates_nothing`

## Cas limites
- Un dossier ouvert `rw` n'est jamais écrit non plus par une reprise.
- Les accès (`atime`) ne sont pas une écriture.

## Règles liées
- BR-MONDE-002, BR-MONDE-005

## Historique
- 2026-11-14 — création (HRT-105, côté agent).
