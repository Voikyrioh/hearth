---
id: FIX-01M4D6KNXEFG8DH4K9JXBJ98MA
titre: Un `.erasing` orphelin n'était fini qu'à la mise à jour suivante
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4D6KNXEFG8DH4K9JXBJ98MA : Un `.erasing` orphelin n'était fini qu'à la mise à jour suivante

## Symptôme
Une panne pendant l'effacement de la copie de la base de la mise à jour laissait `update/hearth.db.before.erasing` (anciennes empreintes en clair, ou à moitié écrasées) jusqu'au prochain nettoyage d'une mise à jour, qui peut ne jamais venir.

## Reproduction
`infrastructure::update::host::tests::an_orphan_erasing_file_is_finished_at_service_start_and_a_live_copy_is_left_alone`.

## Cause root
Seul `clear_staging` reprenait un `.erasing`.

## Impacté
Agent et client depuis la tranche concernée (jamais publié).

## Workaround
Aucun.

## Correction
`finish_interrupted_erasures(data_dir)` au démarrage du service (`app::start_full`) : écrase de zéros puis supprime les `.erasing` de la copie et de son journal ; une copie sous son nom d'origine est celle d'un travail en cours et n'est pas touchée. `FIX:` dans `update/host.rs` et `app.rs`.

## Règles
- BR-UPDATE-029 (copie d'avant l'échange) et ADR-0034 (copie écrasée avant suppression).

## Non-régression
- Le test ci-dessus.

## Références
- Ticket : HRT-18 (suites des reviews des PR #42 et #48)
