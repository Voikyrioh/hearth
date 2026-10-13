---
id: BR-ARCH-005
domaine: ARCH
titre: Seuls un fichier ordinaire et un dossier sont acceptés ; droits, propriétaire et dates ne sont jamais repris
statut: active
invariant: true
source: ADR-0055 (contexts/hearth/conceptions/2026-10-13-adr-lecture-d-une-archive-brouillon.md), contexts/hearth/tickets/hrt/HRT-130.md
maj: 2026-10-13
---

# BR-ARCH-005 — Seuls un fichier ordinaire et un dossier sont acceptés ; droits, propriétaire et dates ne sont jamais repris

## Règle
Refusent TOUTE l'archive : un lien symbolique (attributs Unix d'un zip, point de reprise Windows, type `2` d'un tar), un lien physique (type `1`), un périphérique, un tube, une prise (types `3`, `4`, `6`, attributs Unix d'un zip), un en-tête étendu global, un fichier creux, un format que Hearth ne lit pas. Le port d'écriture ne reçoit ni droits, ni propriétaire, ni dates : les fichiers sont créés `0640` et les dossiers `0750` (moins le `umask`), sans bit d'exécution ni setuid/setgid, au propriétaire du processus. Une archive imbriquée est déposée telle quelle, jamais ouverte.

## Application (code)
- `crates/hearth-agent/src/domain/archive/zip.rs::inspect (attributs)`
- `crates/hearth-agent/src/domain/archive/tar.rs::TarWalker::next_item (types)`
- `crates/hearth-agent/src/domain/archive/sink.rs::ExtractSink (contrat)`
- `crates/hearth-agent/src/infrastructure/archive_fs.rs (modes)`

## Vérification
- `crates/hearth-agent/src/domain/archive/tests_zip.rs::a_symbolic_link_is_refused`
- `crates/hearth-agent/src/domain/archive/tests_zip.rs::a_windows_reparse_point_is_refused_as_a_link`
- `crates/hearth-agent/src/domain/archive/tests_zip.rs::devices_pipes_and_sockets_are_refused`
- `crates/hearth-agent/src/domain/archive/tests_tar.rs::links_devices_pipes_and_exotic_entries_are_refused`
- `crates/hearth-agent/src/domain/archive/tests_zip.rs::an_archive_inside_an_archive_is_stored_as_is_and_never_opened`
- `crates/hearth-agent/tests/archive_fs.rs::unix::modes_owners_and_special_bits_of_the_archive_are_never_restored (CI Linux)`

## Cas limites
- Le propriétaire « de la fiche » (service) est posé par l'appelant, pas par ce module.

## Règles liées
- BR-ARCH-001, BR-ARCH-002, BR-ARCH-003, BR-ARCH-004, BR-ARCH-006, BR-ARCH-007, BR-ARCH-008, BR-ARCH-009, BR-ARCH-010, BR-ARCH-011

## Historique
- 2026-10-13 — création (HRT-130, ADR-0055, lecture d'une archive hostile).
