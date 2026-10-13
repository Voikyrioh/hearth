---
id: BR-ARCH-005
domaine: ARCH
titre: Seuls un fichier ordinaire et un dossier sont acceptés ; droits, propriétaire et dates ne sont jamais repris
statut: active
invariant: true
source: ADR-0055 (contexts/hearth/conceptions/2026-10-13-adr-lecture-d-une-archive-brouillon.md), contexts/hearth/tickets/hrt/HRT-130.md
maj: 2026-10-14
---

# BR-ARCH-005 — Seuls un fichier ordinaire et un dossier sont acceptés ; droits, propriétaire et dates ne sont jamais repris

## Règle
Refusent TOUTE l'archive : un lien symbolique (attributs Unix d'un zip, point de reprise Windows, type `2` d'un tar), un lien physique (type `1`), un périphérique, un tube, une prise (types `3`, `4`, `6`, attributs Unix d'un zip), un zip dont les attributs externes codent un type autre que fichier ou dossier QUEL QUE SOIT l'hôte déclaré (`made_by`), un en-tête étendu global, un fichier creux, un format que Hearth ne lit pas. Le port d'écriture ne reçoit ni droits, ni propriétaire, ni dates. L'APPELANT demande une `WriteSpec` : propriétaire (identifiants numériques, jamais repris de l'archive) et modes fixes (fichiers : lecture/écriture seulement, aucun bit d'exécution ni spécial, lecture par le propriétaire ; dossiers : `rwx` pour le propriétaire, aucun bit spécial). Ils sont posés par descripteur à la création (`fchown`, `fchmod`), donc indépendants du masque ambiant (`umask`) ; le dossier d'attente reste `0700` jusqu'à la bascule. Une archive imbriquée est déposée telle quelle, jamais ouverte.

## Application (code)
- `crates/hearth-agent/src/domain/archive/zip.rs::inspect (attributs)`
- `crates/hearth-agent/src/domain/archive/tar.rs::TarWalker::next_item (types)`
- `crates/hearth-agent/src/domain/archive/sink.rs::ExtractSink (contrat)`
- `crates/hearth-agent/src/infrastructure/archive_fs.rs::WriteSpec`
- `crates/hearth-agent/src/infrastructure/archive_fs/unix.rs::apply`

## Vérification
- `crates/hearth-agent/src/domain/archive/tests_zip.rs::a_symbolic_link_is_refused`
- `crates/hearth-agent/src/domain/archive/tests_zip.rs::a_windows_reparse_point_is_refused_as_a_link`
- `crates/hearth-agent/src/domain/archive/tests_zip.rs::devices_pipes_and_sockets_are_refused`
- `crates/hearth-agent/src/domain/archive/tests_tar.rs::links_devices_pipes_and_exotic_entries_are_refused`
- `crates/hearth-agent/src/domain/archive/tests_zip.rs::an_archive_inside_an_archive_is_stored_as_is_and_never_opened`
- `crates/hearth-agent/src/domain/archive/tests_zip.rs::a_link_or_special_file_is_refused_whatever_host_the_archive_claims`
- `crates/hearth-agent/tests/archive_fs.rs::the_write_parameters_are_validated_where_they_are_built`
- `crates/hearth-agent/tests/archive_fs.rs::unix::modes_and_owners_come_from_the_caller_never_from_the_archive_nor_the_umask (CI Linux)`
- `crates/hearth-agent/tests/archive_fs.rs::unix::the_owner_asked_by_the_caller_is_set_at_creation_when_the_agent_is_root (CI Linux, root ; se saute sinon)`

## Cas limites
- Le propriétaire « de la fiche » (service) est choisi par l'appelant, qui le donne en identifiants numériques ; ce module le pose à la création et ne le déduit jamais de l'archive.
- Sans les droits de root, `fchown` ne réussit que vers l'utilisateur et un groupe du processus : l'agent de production est root.

## Règles liées
- BR-ARCH-001, BR-ARCH-002, BR-ARCH-003, BR-ARCH-004, BR-ARCH-006, BR-ARCH-007, BR-ARCH-008, BR-ARCH-009, BR-ARCH-010, BR-ARCH-011

## Historique
- 2026-10-13 — création (HRT-130, ADR-0055, lecture d'une archive hostile).
- 2026-10-14 — propriétaire et modes demandés par l'appelant, posés par descripteur ; type de zip lu quel que soit `made_by`.
