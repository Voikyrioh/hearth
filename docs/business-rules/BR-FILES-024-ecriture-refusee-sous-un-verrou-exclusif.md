---
id: BR-FILES-024
domaine: FILES
titre: Une écriture de fichier est refusée tant qu'une opération tient le verrou du service ; la lecture reste permise sauf pendant une restauration
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-modifier-un-service.md (BR-FILES-024), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 0, 0 bis et 6), HRT-83
maj: 2026-10-13
---

# BR-FILES-024 — Une écriture de fichier est refusée tant qu'une opération tient le verrou du service ; la lecture reste permise sauf pendant une restauration

## Règle
`LockTable::acquire_shared_write` refuse sous un verrou exclusif ; `check_read(.., ReadKind::Files)` refuse quand une restauration a gelé les fichiers (`set_files_frozen`), `ReadKind::Plain` jamais. Les fichiers eux-mêmes sont HRT-93.

## Application (code)
- `crates/hearth-agent/src/domain/ops/locks.rs::{LockTable::acquire_shared_write, LockTable::check_read, LockTable::set_files_frozen}`

## Vérification
- `crates/hearth-agent/src/domain/ops/locks.rs::tests::file_writes_share_the_service_but_keep_an_exclusive_operation_out`
- `crates/hearth-agent/src/domain/ops/locks.rs::tests::reads_take_no_lock_and_only_file_reads_stop_during_a_restore`

## Cas limites
- Deux comptes qui écrivent dans le même dossier ne se bloquent pas (le conflit sur un même fichier est détecté par HRT-93).

## Règles liées
- BR-LIFE-013

## Historique
- 2026-10-13 — création (HRT-83, ADR-0044 et ADR-0045 : socle des services créés par Hearth, côté agent).
