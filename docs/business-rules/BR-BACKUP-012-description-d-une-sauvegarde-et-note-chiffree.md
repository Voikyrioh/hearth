---
id: BR-BACKUP-012
domaine: BACKUP
titre: Chaque sauvegarde est décrite (date, tailles, fichiers, mode, repères, compte, image, intégrité) ; la note est chiffrée dans le registre et absente du manifeste et du journal
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-sauvegardes-service.md (BR-BACKUP-012), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 6, 7 et 9.4), HRT-97
maj: 2026-11-14
---

# BR-BACKUP-012 : Chaque sauvegarde est décrite (date, tailles, fichiers, mode, repères, compte, image, intégrité) ; la note est chiffrée dans le registre et absente du manifeste et du journal

## Règle
La note : 80 caractères au plus, sans caractère de contrôle, scellée par ChaCha20-Poly1305 sous `services.key` (données associées : service, révision 0, champ `backup-note:<id>`), jamais en clair en base, jamais au manifeste (qui voyage avec l'archive téléchargée), jamais au journal d'activité. Une clé illisible refuse la note avant d'accepter l'opération ; la liste n'ouvre les notes que s'il y en a.

## Application (code)
- `crates/hearth-agent/src/application/backups.rs::clean_note, ::Backups::seal_note`
- `crates/hearth-agent/src/domain/sealed.rs::SecretField::BackupNote`
- `crates/hearth-agent/migrations/0017_backups.sql (note_nonce, note_sealed)`

## Vérification
- `crates/hearth-agent/tests/service_backups_https.rs::the_note_is_sealed_in_the_registry_and_never_in_the_journal_the_manifest_or_the_clear`
- `crates/hearth-agent/tests/service_backups_https.rs::the_archive_is_a_standard_tar_gz_with_a_manifest_beside_it_and_inside_it`

## Cas limites
- —

## Règles liées
- BR-BACKUP-014
- BR-BACKUP-032

## Historique
- 2026-11-14 : création (HRT-97, côté agent).
