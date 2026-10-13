---
id: BR-BACKUP-025
domaine: BACKUP
titre: Une sauvegarde interrompue est déclarée telle ; sa partie écrite est retirée au redémarrage, rien n'est relancé
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-modifier-un-service.md (BR-BACKUP-025), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 0, 0 bis et 6), HRT-83
maj: 2026-10-13
---

# BR-BACKUP-025 — Une sauvegarde interrompue est déclarée telle ; sa partie écrite est retirée au redémarrage, rien n'est relancé

## Règle
L'archive en cours d'écriture est un objet créé par l'opération (`ObjectKind::PartialArchive`), nommé au journal avec son numéro d'inode. Au démarrage, tant que son étape n'est pas `done`, elle est retirée APRÈS vérification de l'inode (un fichier de même nom mais d'un autre inode n'est pas touché ; sans plate-forme à inodes, rien n'est retiré). Rien d'autre n'est retiré ni relancé. Une restauration interrompue est « à vérifier » (BR-EDIT-031).

## Application (code)
- `crates/hearth-agent/src/domain/ops/recover.rs::recover (nettoyage)`
- `crates/hearth-agent/src/infrastructure/created_objects.rs::FsCreatedObjects`

## Vérification
- `crates/hearth-agent/tests/service_ops_journal.rs::only_what_the_operation_created_is_removed_and_only_if_it_is_still_that_object`
- `crates/hearth-agent/src/domain/ops/recover.rs::tests::a_partial_archive_goes_unless_its_step_is_done`
- `crates/hearth-agent/src/infrastructure/created_objects.rs::tests::a_replacement_with_another_inode_is_left_alone`

## Cas limites
- Inodes et liens : code Linux, prouvé par la CI seulement.
- Pas de résolution relative à un descripteur dans cette tranche (couche SafeFs, HRT-85).

## Règles liées
- BR-EDIT-031

## Historique
- 2026-10-13 — création (HRT-83, ADR-0044 et ADR-0045 : socle des services créés par Hearth, côté agent).
