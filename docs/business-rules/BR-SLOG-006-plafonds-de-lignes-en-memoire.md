---
id: BR-SLOG-006
domaine: SLOG
titre: Aucune lecture ne garde plus de 5 000 lignes ni 2 Mio : au-delà, les plus anciennes sont retirées et comptées
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-journaux-service.md (BR-SLOG-006), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §7, §9, §10, HRT-66
maj: 2026-10-13
---

# BR-SLOG-006 — Aucune lecture ne garde plus de 5 000 lignes ni 2 Mio : au-delà, les plus anciennes sont retirées et comptées

## Règle
Côté agent, l'anneau d'une lecture est borné à 5 000 lignes ET 2 Mio de texte ; les plus anciennes lignes sont retirées, comptées, et un `gap { count, reason: slow }` est inséré à leur place (BR-SLOG-007). Le plafond de 5 000 lignes à l'écran, la mention « Les lignes plus anciennes ne sont plus affichées » et le trou de pause sont portés par le client (HRT-67).

## Application (code)
- `crates/hearth-agent/src/domain/services/logs.rs::LogRing`
- `crates/hearth-proto/src/api/service_logs.rs::{RING_LINES, RING_BYTES}`

## Vérification
- `crates/hearth-agent/src/domain/services/logs.rs::tests::a_slow_reader_loses_the_oldest_lines_and_a_counted_gap_comes_first`
- `crates/hearth-agent/src/domain/services/logs.rs::tests::the_ring_is_bounded_by_text_size_too`
- `crates/hearth-agent/src/domain/services/logs.rs::tests::a_producer_at_a_thousand_lines_a_second_against_a_reader_that_never_reads_stays_bounded`

## Règles liées
- BR-SLOG-007

## Historique
- 2026-10-13 — création (HRT-66, tranche 4 des déploiements : journaux en direct, côté agent et protocole).
