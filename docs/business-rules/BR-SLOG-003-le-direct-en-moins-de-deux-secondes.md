---
id: BR-SLOG-003
domaine: SLOG
titre: En direct, une ligne écrite par le service est visible en moins de 2 secondes
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-journaux-service.md (BR-SLOG-003), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §7, §9, §10, HRT-66
maj: 2026-10-13
---

# BR-SLOG-003 — En direct, une ligne écrite par le service est visible en moins de 2 secondes

## Règle
Une ligne écrite après l'ouverture part au client au plus 100 ms plus tard (fenêtre de regroupement, BR-SLOG-007), donc bien dans les 2 secondes. La vue calée sur la ligne la plus récente est portée par le client.

## Application (code)
- `crates/hearth-agent/src/application/service_logs.rs::LogSession::next_message`
- `crates/hearth-agent/src/domain/services/logs.rs::flush_due`

## Vérification
- `crates/hearth-agent/tests/service_logs_ws.rs::the_last_thousand_lines_then_the_live_arrive_in_under_two_seconds`
- `crates/hearth-agent/tests/service_logs_ws.rs::a_thousand_lines_a_second_reach_a_reader_that_keeps_up_without_loss (latence maximale mesurée)`
- `crates/hearth-agent/src/domain/services/logs.rs::tests::lines_flush_after_a_hundred_milliseconds_or_at_the_caps_and_events_at_once`

## Règles liées
- BR-SLOG-001, BR-SLOG-007

## Historique
- 2026-10-13 — création (HRT-66, tranche 4 des déploiements : journaux en direct, côté agent et protocole).
