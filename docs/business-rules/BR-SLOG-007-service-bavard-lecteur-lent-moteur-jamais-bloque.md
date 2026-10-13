---
id: BR-SLOG-007
domaine: SLOG
titre: Un service qui écrit 1 000 lignes par seconde ne bloque jamais le moteur ni le lien : un lecteur lent perd les plus anciennes lignes, comptées
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-journaux-service.md (BR-SLOG-007), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §7, §9, §10, HRT-66
maj: 2026-10-13
---

# BR-SLOG-007 — Un service qui écrit 1 000 lignes par seconde ne bloque jamais le moteur ni le lien : un lecteur lent perd les plus anciennes lignes, comptées

## Règle
Par lecture, une tâche lit le moteur sans jamais s'arrêter et pousse dans l'anneau borné (BR-SLOG-006) ; le socket vide l'anneau par lots, toutes les 100 ms, ou dès 500 lignes, ou dès 256 Kio. Si le socket n'avance pas, l'anneau perd ses lignes les plus anciennes, les compte, et un `gap { count, reason: slow }` précède le lot suivant. Le moteur n'est jamais ralenti par un client lent du DIRECT (la première page, lue dans un fichier déjà écrit, attend la place dans l'anneau, 5 s au plus : BR-SLOG-001 ; ses pertes se disent `first_page`, pas `slow`), et `/stream` n'est jamais concerné (un socket par lecture). Un client qui ne reçoit plus rien pendant `send_timeout` (10 s) perd SA lecture, pas le lien. Mémoire par lecture : anneau (2 Mio), ligne en cours d'assemblage (2 x 64 Kio), trame du moteur (256 Kio au plus).

## Application (code)
- `crates/hearth-agent/src/domain/services/logs.rs::{LogRing, flush_due}`
- `crates/hearth-agent/src/application/service_logs.rs::{Reader, LogSession}`
- `crates/hearth-agent/src/entrypoint/ws/service_logs.rs::serve`
- `crates/hearth-agent/src/infrastructure/engine/logs.rs::Reader`

## Vérification
- `crates/hearth-agent/tests/service_logs_ws.rs::a_slow_reader_loses_the_oldest_lines_counted_and_the_stream_goes_on`
- `crates/hearth-agent/tests/service_logs_ws.rs::the_engine_socket_is_drained_whatever_the_client_does_with_the_real_adapter`
- `crates/hearth-agent/tests/service_logs_ws.rs::a_thousand_lines_a_second_reach_a_reader_that_keeps_up_without_loss`
- `crates/hearth-agent/tests/service_logs_ws.rs::lines_leave_in_batches_of_a_hundred_milliseconds_and_never_more_than_five_hundred`
- `crates/hearth-agent/src/domain/services/logs.rs::tests::a_batch_stops_at_five_hundred_lines_or_the_byte_cap_and_always_carries_a_line`
- job `engine` de la CI : `tests/engine_real.rs::a_service_writing_a_thousand_lines_a_second_is_read_without_loss_for_the_whole_duration` (60 s ; 10 min en `workflow_dispatch`)

## Règles liées
- BR-SLOG-003, BR-SLOG-006, BR-SLOG-020

## Historique
- 2026-10-13 — création (HRT-66, tranche 4 des déploiements : journaux en direct, côté agent et protocole).
- 2026-10-13 — corrections de la review locale de HRT-66 : `slow` ne concerne que le direct ; états et repères en attente bornés (256 par anneau).
