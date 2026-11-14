# Architecture — domain/backups

Les sauvegardes d'un service (HRT-97, ADR-0047, `BR-BACKUP-*`). Règles pures, sans E/S : ni `std::fs`, ni SQLx, ni axum, ni tokio, ni `tar`. Aucun type d'ici n'a de champ pour un contenu de fichier ni pour la note d'un compte.

```
backups/
├── mod.rs       → `BackupId` (ULID en majuscules, jamais un nom pris tel quel d'une requête ; noms `<id>.tar.gz`, `.tar.gz.part`, `.manifest.json`), `archive_dir_name` (le nom d'un dossier monté dans l'archive : un composant sûr et unique)
├── ignore.rs    → `IgnoreRules` : les éléments ignorés (ajoutés, retirés des ignorés par défaut, pour cette fois) ; `name_looks_like_backups` n'est qu'une PROPOSITION — BR-BACKUP-003
├── manifest.rs  → `Manifest` : une ligne par entrée (chemin, type, taille, SHA-256, droits, propriétaire, date, `changed`), totaux, repères ; `check` (format, bornes, chemins sûrs et uniques, empreinte sur chaque fichier, totaux justes) — BR-BACKUP-012, 014
├── space.rs     → `reserve` (le plus grand de 5 Gio et de 5 %), `judge` (refus sous la réserve, sans case), `too_low_during_copy` (sous la moitié) — BR-BACKUP-009
├── ownership.rs → `judge_archive` (même inode : à Hearth ; autre inode : remplacée, jamais touchée ; absente : disparue), `manifest_is_ours` (empreinte du registre) — BR-BACKUP-004, 013
├── overlap.rs   → `overlaps` : la racine des sauvegardes ne chevauche aucun dossier de données (chemins canoniques ET identités, dans les deux sens) — BR-BACKUP-004 (b)
└── read.rs      → `ArchiveIndex` : une archive lue comme hostile (chaque entrée a sa ligne du manifeste, bon type, bonne taille, aucune de plus, total d'octets borné) — ADR-0047 point 7
```

Le plan de l'opération est dans `../ops/backup.rs`. La copie, la vérification et le registre sont dans `application/backups.rs`, `infrastructure/backup_archive.rs` et `infrastructure/sqlite/backups_repo.rs`.
