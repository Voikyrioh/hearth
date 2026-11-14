---
id: BR-BACKUP-004
domaine: BACKUP
titre: Hearth ne lit, ne liste, ne renomme, ne supprime et ne change les droits que des fichiers inscrits à son registre avec leur inode et l'empreinte de leur manifeste, quel que soit leur nom
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-sauvegardes-service.md (BR-BACKUP-004), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 6, 7 et 9.4), HRT-97
maj: 2026-11-14
---

# BR-BACKUP-004 : Hearth ne lit, ne liste, ne renomme, ne supprime et ne change les droits que des fichiers inscrits à son registre avec leur inode et l'empreinte de leur manifeste, quel que soit leur nom

## Règle
(a) Dans la racine des sauvegardes, la propriété est PROUVÉE : un fichier est à Hearth s'il est inscrit dans `backups` (périphérique, inode) ET que son manifeste voisin a l'empreinte de la ligne ; la racine n'est jamais listée ni parcourue (`BackupFiles` n'a ni `read_dir` ni recherche par nom) ; un fichier déposé à la main, même nommé comme une archive de Hearth, n'existe pas pour lui. (b) La racine ne chevauche aucun dossier de données : jugé sur les chemins canoniques ET les identités (périphérique, inode) à chaque démarrage et avant chaque opération ; sinon la fonction est refusée avec « La racine des sauvegardes chevauche un dossier de données. ». (c) Un élément ignoré n'est jamais lu. (e) Hearth ne supprime jamais un fichier dont l'inode n'est plus celui du registre (BR-BACKUP-013). Les points (d) et (f) (restauration, invariant de non-destruction) viennent avec la restauration.

## Application (code)
- `crates/hearth-agent/src/domain/backups/ownership.rs::judge_archive, ::manifest_is_ours`
- `crates/hearth-agent/src/domain/backups/overlap.rs::overlaps`
- `crates/hearth-agent/src/application/ports/backup_files.rs::BackupFiles`
- `crates/hearth-agent/src/infrastructure/backup_files.rs::StdBackupFiles`
- `crates/hearth-agent/src/application/backups.rs::Backups::check_overlap, ::check_root, ::delete`

## Vérification
- `crates/hearth-agent/tests/service_backups_https.rs::files_that_are_not_in_the_registry_are_never_listed_read_renamed_or_removed`
- `crates/hearth-agent/tests/service_backups_https.rs::a_backups_root_inside_a_data_folder_refuses_every_backup_function`
- `crates/hearth-agent/src/infrastructure/backup_files.rs::tests (Linux)`
- `crates/hearth-agent/src/domain/backups/ownership.rs::tests`
- `crates/hearth-agent/src/domain/backups/overlap.rs::tests`

## Cas limites
- Inodes et droits : code Linux, prouvé par la CI seulement.
- Entre la vérification de l'inode et le retrait, un processus root pourrait remplacer le fichier : la garde protège d'une erreur, pas de root (même limite que `FsCreatedObjects`).

## Règles liées
- BR-BACKUP-013
- BR-BACKUP-003

## Historique
- 2026-11-14 : création (HRT-97, côté agent).
