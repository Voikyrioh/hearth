---
id: BR-SLOG-001
domaine: SLOG
titre: À l'ouverture, les 1 000 dernières lignes du service (ou toutes s'il en a moins) arrivent en moins de 2 secondes, puis le direct
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-journaux-service.md (BR-SLOG-001), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §7, §9, §10, HRT-66
maj: 2026-10-13
---

# BR-SLOG-001 — À l'ouverture, les 1 000 dernières lignes du service (ou toutes s'il en a moins) arrivent en moins de 2 secondes, puis le direct

## Règle
À l'ouverture du socket (`open { tail: 1000 }`, plafonné à 1 000), l'agent demande au moteur les `tail` dernières lignes (jamais tout le fichier, même si le service a écrit des gigaoctets) puis suit le service en direct. Un service qui n'a rien écrit ne donne aucune ligne et aucune erreur (BR-SLOG-010). La première page est lue au rythme du client : tant que l'agent lit cet historique (un fichier déjà écrit), il attend la place dans l'anneau (5 s au plus sans que le client vide quoi que ce soit) au lieu de le déborder ; 1 000 lignes de 4 Ko arrivent donc entières. Passé ce délai, les lignes les plus anciennes de la page sont comptées dans `gap { reason: first_page }` : ce n'est pas un lecteur lent du direct, et le client ne doit pas le dire. `tail` compte des messages du moteur, pas des lignes : un message plein de retours chariot donne plusieurs lignes. L'affichage de l'heure par ligne, l'ordre et l'absence de doublon à l'écran (BR-SLOG-002) sont portés par le client : l'agent garantit l'ordre du moteur et l'absence de doublon dans le flux.

## Application (code)
- `crates/hearth-agent/src/application/service_logs.rs::ServiceLogs::open`
- `crates/hearth-agent/src/application/service_logs.rs::Reader::{plan, wait_for_room}`
- `crates/hearth-agent/src/infrastructure/engine/logs.rs::request`
- `crates/hearth-proto/src/api/service_logs.rs::TAIL_LINES`

## Vérification
- `crates/hearth-agent/tests/service_logs_ws.rs::the_last_thousand_lines_then_the_live_arrive_in_under_two_seconds`
- `crates/hearth-agent/tests/service_logs_ws.rs::a_service_that_has_written_less_than_a_thousand_lines_gives_them_all_and_a_silent_one_gives_none`
- `crates/hearth-agent/src/infrastructure/engine/logs.rs::tests::multiplexed_frames_become_stamped_lines_of_the_right_output`
- `crates/hearth-agent/tests/service_logs_ws.rs::a_first_page_of_long_lines_arrives_whole_for_a_reader_that_keeps_up`
- `crates/hearth-agent/tests/service_logs_ws.rs::a_first_page_a_client_does_not_read_is_cut_as_first_page_never_as_slow`
- `crates/hearth-agent/src/domain/services/logs.rs::tests::a_first_page_line_pushed_out_of_the_ring_is_not_called_slow`
- job `engine` de la CI : `tests/support/engine_contract.rs::logs` (tail=1, fin) contre un vrai moteur

## Règles liées
- BR-SLOG-003, BR-SLOG-010, BR-SLOG-021

## Historique
- 2026-10-13 — création (HRT-66, tranche 4 des déploiements : journaux en direct, côté agent et protocole).
- 2026-10-13 — corrections de la review locale de HRT-66 : la première page n'est plus amputée ni attribuée à un lecteur lent (attente de place pendant l'historique, raison `first_page`).
