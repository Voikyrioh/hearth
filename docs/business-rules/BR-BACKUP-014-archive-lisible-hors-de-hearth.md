---
id: BR-BACKUP-014
domaine: BACKUP
titre: Une sauvegarde est une archive `.tar.gz` standard avec un manifeste JSON lisible à côté et dans l'archive
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-sauvegardes-service.md (BR-BACKUP-014), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 6, 7 et 9.4), HRT-97
maj: 2026-11-14
---

# BR-BACKUP-014 : Une sauvegarde est une archive `.tar.gz` standard avec un manifeste JSON lisible à côté et dans l'archive

## Règle
Format `tar` GNU compressé en gzip niveau 1, noms longs compris, droits sans bit spécial, un dossier par racine montée (`data/…`). `<id>.manifest.json` à côté (avec l'empreinte et la taille de l'archive) et `.hearth-manifest.json` en DERNIÈRE entrée (sans l'empreinte de l'archive, qui ne peut pas se contenir elle-même). Écart assumé avec la spec (« en tête ») : écrire le manifeste en tête exigerait de lire deux fois des dizaines de Gio ; Hearth lit le manifeste d'à côté, jamais celui de l'archive.

## Application (code)
- `crates/hearth-agent/src/infrastructure/backup_archive.rs::ArchiveWriter`
- `crates/hearth-agent/src/domain/backups/manifest.rs`

## Vérification
- `crates/hearth-agent/tests/service_backups_https.rs::the_archive_is_a_standard_tar_gz_with_a_manifest_beside_it_and_inside_it`
- `crates/hearth-agent/src/domain/backups/manifest.rs::tests`

## Cas limites
- Liens physiques archivés comme fichiers indépendants ; attributs étendus et bits setuid non conservés.

## Règles liées
- BR-BACKUP-012

## Historique
- 2026-11-14 : création (HRT-97, côté agent).
