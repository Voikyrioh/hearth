---
id: BR-SLOG-012
domaine: SLOG
titre: Les lignes de la sortie d'erreur sont marquées ; un service lancé avec un terminal mélange ses sorties et l'agent le dit
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-journaux-service.md (BR-SLOG-012), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §7, §9, §10, HRT-66
maj: 2026-10-13
---

# BR-SLOG-012 — Les lignes de la sortie d'erreur sont marquées ; un service lancé avec un terminal mélange ses sorties et l'agent le dit

## Règle
Chaque ligne porte sa sortie (`s: "o"` normale, `s: "e"` erreur). Pour un conteneur à terminal le moteur n'envoie qu'un flux brut : toutes les lignes sont `o` et `status.mixed` est vrai. Le repère « Erreur » (texte et icône) et le bandeau d'explication sont portés par le client.

## Application (code)
- `crates/hearth-proto/src/api/service_logs.rs::{LogSource, LogsStatus}`
- `crates/hearth-agent/src/infrastructure/engine/logs.rs::Reader::{demux, demux_tty}`
- `crates/hearth-agent/src/application/service_logs.rs::status_of`

## Vérification
- `crates/hearth-agent/tests/service_logs_ws.rs::error_lines_are_marked_and_a_terminal_service_says_its_outputs_are_mixed`
- `crates/hearth-agent/src/infrastructure/engine/logs.rs::tests::a_terminal_container_gives_one_raw_stream_even_when_a_line_starts_with_a_control_byte`
- job `engine` de la CI : `tests/support/engine_contract.rs::logs` (sorties, terminal) contre un vrai moteur

## Règles liées
- BR-SLOG-013

## Historique
- 2026-10-13 — création (HRT-66, tranche 4 des déploiements : journaux en direct, côté agent et protocole).
