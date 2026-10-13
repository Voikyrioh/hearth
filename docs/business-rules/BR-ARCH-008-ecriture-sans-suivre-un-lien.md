---
id: BR-ARCH-008
domaine: ARCH
titre: L'extraction écrit sous un dossier neuf sans jamais suivre un lien, puis bascule
statut: active
invariant: true
source: ADR-0055 (contexts/hearth/conceptions/2026-10-13-adr-lecture-d-une-archive-brouillon.md), contexts/hearth/tickets/hrt/HRT-130.md
maj: 2026-10-13
---

# BR-ARCH-008 — L'extraction écrit sous un dossier neuf sans jamais suivre un lien, puis bascule

## Règle
L'extraction se fait dans un dossier d'attente neuf voisin de la destination (`0750`), puis `rename` vers la destination, qui ne doit pas exister. Sous Unix chaque dossier traversé est ouvert par `openat(parent, composant, O_NOFOLLOW | O_DIRECTORY)` depuis le descripteur du dossier d'attente et chaque fichier est créé par `O_CREAT | O_EXCL | O_NOFOLLOW` : un lien rencontré (créé par un tiers, ou déjà présent) est `Blocked`, un objet déjà présent `AlreadyExists`, rien n'est écrit au travers. Un refus, une panne ou une annulation retire le dossier d'attente (sans suivre de lien) et ne laisse rien. L'écriture ne remplace jamais un objet existant.

## Application (code)
- `crates/hearth-agent/src/infrastructure/archive_fs.rs::DirectorySink`
- `crates/hearth-agent/src/infrastructure/archive_fs.rs::extract_file`
- `crates/hearth-agent/src/domain/archive/sink.rs::ExtractSink`

## Vérification
- `crates/hearth-agent/tests/archive_fs.rs::an_archive_is_unpacked_into_a_new_folder_by_a_switch_and_nothing_else_is_left`
- `crates/hearth-agent/tests/archive_fs.rs::a_refused_archive_creates_nothing_not_even_a_staging_folder`
- `crates/hearth-agent/tests/archive_fs.rs::an_existing_destination_is_never_replaced`
- `crates/hearth-agent/tests/archive_fs.rs::unix::a_symlink_already_in_the_destination_is_never_followed (CI Linux)`
- `crates/hearth-agent/tests/archive_fs.rs::unix::an_existing_hard_link_is_never_written_through (CI Linux)`
- `crates/hearth-agent/tests/archive_fs.rs::unix::a_root_that_is_itself_a_symlink_is_not_opened (CI Linux)`

## Cas limites
- La voie hors Unix (poste Windows) vérifie chaque composant avec `symlink_metadata` : sans la garantie sans course de la voie Unix. Le journal des opérations (inode du dossier d'attente) est l'affaire de l'appelant.

## Règles liées
- BR-ARCH-001, BR-ARCH-002, BR-ARCH-003, BR-ARCH-004, BR-ARCH-005, BR-ARCH-006, BR-ARCH-007, BR-ARCH-009, BR-ARCH-010, BR-ARCH-011

## Historique
- 2026-10-13 — création (HRT-130, ADR-0055, lecture d'une archive hostile).
