---
id: BR-BACKUP-007
domaine: BACKUP
titre: Une copie à chaud ne garantit pas la cohérence : les fichiers qui changent pendant la copie sont notés et la sauvegarde porte les repères
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-sauvegardes-service.md (BR-BACKUP-007), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 6, 7 et 9.4), HRT-97
maj: 2026-11-14
---

# BR-BACKUP-007 : Une copie à chaud ne garantit pas la cohérence : les fichiers qui changent pendant la copie sont notés et la sauvegarde porte les repères

## Règle
`fstat` avant et après la lecture de chaque fichier, sur le même descripteur : taille ou date de modification (à la nanoseconde) différentes, ou fichier rétréci, marquent `changed` dans le manifeste ; un fichier disparu en route est compté changé. Un fichier qui grossit est archivé à sa taille de départ, jamais au-delà. Repères : « À chaud » (`hot`) pour une copie à chaud ; « Peut être incohérente » (`maybe_inconsistent`) dès qu'au moins un fichier a changé, QUEL QUE SOIT le mode (un service arrêté que le moteur relance, ou un autre écrivain, change aussi des fichiers) et après un arrêt forcé. Les éléments que la copie n'archive pas (tubes, sockets, autre montage, nom illisible, arborescence trop profonde, lien illisible) sont COMPTÉS (`skipped`, au manifeste, au registre, dans la liste et au journal d'activité) : jamais omis en silence. Une sauvegarde incohérente reste utilisable. L'ordre d'enregistrement envoyé au programme avant la copie (jeu) viendra avec HRT-103/N11.

## Application (code)
- `crates/hearth-agent/src/application/backups.rs::Walk::file, crates/hearth-agent/src/domain/backups/rules.rs::flags_of`
- `crates/hearth-agent/src/infrastructure/backup_archive.rs::ArchiveWriter::file`

## Vérification
- `crates/hearth-agent/tests/service_backups_https.rs::a_hot_copy_says_which_files_changed_while_it_was_read`
- `crates/hearth-agent/src/infrastructure/backup_archive.rs::tests::a_file_that_shrinks_is_padded_to_its_announced_size_and_flagged`

## Cas limites
- Ce contrôle ne voit pas ce qu'un programme garde en mémoire : le texte du client le dit.

## Règles liées
- BR-BACKUP-006

## Historique
- 2026-11-14 : création (HRT-97, côté agent).
