---
id: BR-BACKUP-013
domaine: BACKUP
titre: L'intégrité est vérifiée par empreintes après la création, sur demande, et avant toute restauration ; une archive remplacée à la main n'est plus supprimée, seule sa ligne l'est
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-sauvegardes-service.md (BR-BACKUP-013), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 6, 7 et 9.4), HRT-97
maj: 2026-11-14
---

# BR-BACKUP-013 : L'intégrité est vérifiée par empreintes après la création, sur demande, et avant toute restauration ; une archive remplacée à la main n'est plus supprimée, seule sa ligne l'est

## Règle
`verify_archive` relit l'archive comme hostile : chaque entrée doit avoir sa ligne du manifeste (type, taille, SHA-256), aucune entrée de plus, le nombre d'entrées et le total d'octets bornés par le manifeste, le manifeste de l'archive égal à celui d'à côté, l'empreinte de l'archive entière égale à celle du registre. Résultats : « Vérifiée le {date} », « Corrompue », « Disparue ». Une sauvegarde dont la vérification de création échoue est supprimée par Hearth (c'est son fichier : archive et manifeste, puis la ligne) et l'échec est dit (cause `backup_corrupt`). Si l'archive n'a plus l'inode du registre : « Supprimer » retire seulement la ligne, laisse le fichier et son manifeste (`removed_files: false`).

## Application (code)
- `crates/hearth-agent/src/domain/backups/read.rs::ArchiveIndex`
- `crates/hearth-agent/src/infrastructure/backup_archive.rs::verify_archive`
- `crates/hearth-agent/src/application/backups.rs::Backups::verify, ::check_blocking, ::Backups::delete`

## Vérification
- `crates/hearth-agent/tests/service_backups_https.rs::an_archive_replaced_by_hand_is_corrupt_and_deleting_it_only_drops_the_registry_line`
- `crates/hearth-agent/tests/service_backups_https.rs::a_changed_byte_in_hearths_own_archive_is_found_by_the_check_on_demand`
- `crates/hearth-agent/tests/service_backups_https.rs::a_backup_whose_check_fails_at_creation_is_removed_by_hearth_and_the_failure_is_said`
- `crates/hearth-agent/tests/service_backups_https.rs::a_vanished_archive_is_listed_as_missing_and_its_line_can_be_removed`
- `crates/hearth-agent/src/domain/backups/read.rs::tests`
- `crates/hearth-agent/src/infrastructure/backup_archive.rs::tests`

## Cas limites
- La vérification à la demande est synchrone dans cette tranche (une archive de plusieurs dizaines de Gio peut dépasser le délai d'une requête) : le suivi comme opération légère est un suivi de HRT-98.

## Règles liées
- BR-BACKUP-004
- BR-BACKUP-014

## Historique
- 2026-11-14 : création (HRT-97, côté agent).
