---
id: BR-SLOG-021
domaine: SLOG
titre: Hearth ne conserve aucune ligne : il n'envoie que ce que le moteur a gardé
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-journaux-service.md (BR-SLOG-021), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §7, §9, §10, HRT-66
maj: 2026-10-13
---

# BR-SLOG-021 — Hearth ne conserve aucune ligne : il n'envoie que ce que le moteur a gardé

## Règle
Les lignes anciennes que le moteur a fait tourner sont simplement absentes, sans erreur à l'ouverture. À la reprise, une ligne de curseur disparue est signalée (`resume_lost`, BR-SLOG-014). Aucun fichier, aucune table, aucun cache de lignes côté agent.

## Application (code)
- `crates/hearth-agent/src/domain/services/logs.rs::LogRing`
- `crates/hearth-agent/src/infrastructure/engine/logs.rs`

## Vérification
- `crates/hearth-agent/tests/service_logs_ws.rs::a_resume_whose_line_the_engine_no_longer_has_says_so_and_reloads_the_last_lines`
- `crates/hearth-agent/tests/service_logs_ws.rs::no_line_reaches_a_trace_the_database_the_journal_or_the_operations_queue`

## Règles liées
- BR-SLOG-014, BR-SLOG-019

## Historique
- 2026-10-13 — création (HRT-66, tranche 4 des déploiements : journaux en direct, côté agent et protocole).
