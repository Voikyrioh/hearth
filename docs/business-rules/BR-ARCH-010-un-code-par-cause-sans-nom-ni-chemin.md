---
id: BR-ARCH-010
domaine: ARCH
titre: Un refus porte un code par cause et ne cite jamais un nom d'entrée, un chemin du serveur ni un contenu
statut: active
invariant: true
source: ADR-0055 (contexts/hearth/conceptions/2026-10-13-adr-lecture-d-une-archive-brouillon.md), contexts/hearth/tickets/hrt/HRT-130.md
maj: 2026-10-13
---

# BR-ARCH-010 — Un refus porte un code par cause et ne cite jamais un nom d'entrée, un chemin du serveur ni un contenu

## Règle
`ArchiveRefusal` a une variante et un code stable (`code()`) par cause. Aucune variante ne porte un nom d'entrée ni un chemin ; le texte du système n'est jamais repris d'une erreur d'entrée-sortie (`from_io`). `is_about_the_archive()` distingue une archive refusée d'une panne de la machine ou d'une annulation.

## Application (code)
- `crates/hearth-agent/src/domain/archive/refusal.rs::ArchiveRefusal`

## Vérification
- `crates/hearth-agent/src/domain/archive/tests_paths.rs::a_refusal_never_quotes_the_entry_name_or_a_path`
- `crates/hearth-agent/tests/archive_fs.rs::an_error_never_quotes_a_server_path`
- `crates/hearth-agent/src/domain/archive/tests_extract.rs::a_failing_sink_turns_into_a_write_failure_without_naming_anything`

## Cas limites
- Aucun cas limite connu au-delà de ceux des tests.

## Règles liées
- BR-ARCH-001, BR-ARCH-002, BR-ARCH-003, BR-ARCH-004, BR-ARCH-005, BR-ARCH-006, BR-ARCH-007, BR-ARCH-008, BR-ARCH-009, BR-ARCH-011

## Historique
- 2026-10-13 — création (HRT-130, ADR-0055, lecture d'une archive hostile).
