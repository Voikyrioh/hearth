---
id: BR-MONDE-026
domaine: MONDE
titre: Une reprise de monde interrompue est déclarée telle ; ce qui a été copié est retiré, la source n'est pas touchée
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-modifier-un-service.md (BR-MONDE-026), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 0, 0 bis et 6), HRT-83
maj: 2026-10-13
---

# BR-MONDE-026 — Une reprise de monde interrompue est déclarée telle ; ce qui a été copié est retiré, la source n'est pas touchée

## Règle
Le dossier d'import est un objet créé par l'opération (`ObjectKind::ImportDir`), nommé au journal. Au démarrage il est retiré, après vérification de l'inode, tant qu'aucun geste destructif de l'opération n'a de ligne. La source n'est jamais un objet créé : elle n'est jamais nommée au journal ni retirée.

## Application (code)
- `crates/hearth-agent/src/domain/ops/recover.rs::recover (nettoyage)`
- `crates/hearth-agent/src/infrastructure/created_objects.rs::FsCreatedObjects`

## Vérification
- `crates/hearth-agent/src/domain/ops/recover.rs::tests::an_import_or_staging_directory_goes_only_while_nothing_destructive_has_begun`
- `crates/hearth-agent/src/infrastructure/created_objects.rs::tests::the_object_of_the_journal_is_removed_file_or_directory`

## Cas limites
- La copie du monde elle-même est HRT-105.

## Règles liées
- BR-EDIT-031

## Historique
- 2026-10-13 — création (HRT-83, ADR-0044 et ADR-0045 : socle des services créés par Hearth, côté agent).
