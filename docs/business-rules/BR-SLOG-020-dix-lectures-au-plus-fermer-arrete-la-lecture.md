---
id: BR-SLOG-020
domaine: SLOG
titre: Dix lectures simultanées au plus pour tout l'agent ; fermer le socket arrête la lecture côté serveur et rend la place
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-journaux-service.md (BR-SLOG-020), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §7, §9, §10, HRT-66
maj: 2026-10-14
---

# BR-SLOG-020 — Dix lectures simultanées au plus pour tout l'agent ; fermer le socket arrête la lecture côté serveur et rend la place

## Règle
Les dix places sont communes à tous les comptes (`ServiceLogs::acquire`, `Permit`). Une onzième ouverture répond `error { code: "LOGS_LIMIT" }`. Fermer le socket, proprement ou non (coupure du lien, client disparu), lâche la `LogSession` : la tâche qui lit le moteur est abandonnée, la connexion au moteur se ferme et la place revient. Une lecture suspendue par une coupure libère donc sa place ; à la reprise elle en retrouve une s'il en reste, sinon `LOGS_LIMIT`. La place d'attente d'authentification (quotas de `/stream`) est gardée jusqu'à l'`open`, puis rendue : une lecture est comptée par ses propres dix places. Un compte en lecture seule pouvait tenir les dix places et en priver les administrateurs : le propriétaire a décidé, le 2026-10-13, d'un plafond par compte (3 lectures, BR-SLOG-026) et d'un frein aux ouvertures (BR-SLOG-027), qui se vérifient avant celui-ci. Le texte « Trop de journaux sont ouverts… » est porté par le client.

**Côté client (HRT-67, ADR-0040) : seul l'onglet affiché lit.** Une lecture n'est jamais tenue par une fenêtre cachée : la croix de la fenêtre ferme les lectures côté Rust (`closed { window_hidden }`), la page efface ses lignes et dit qu'elle a été arrêtée, sans reprise automatique. Et une session de lecture ne reste jamais ouverte à l'insu de l'écran (page rechargée ou plantée, page muette, fenêtre détruite, application quittée, serveur changé) : génération de page annoncée au démarrage, battement toutes les 10 s, fermeture d'office après 45 s de silence (`services/logs.rs`).

## Application (code)
- `apps/desktop/src-tauri/src/services/logs.rs::{LogSessions::announce_page, LogSessions::keep_alive, LogSessions::reap, LogSessions::close_window, LogSessions::forget_window, LogSessions::close_all, IDLE_TIMEOUT, KEEP_ALIVE_PERIOD, REAP_PERIOD}` (HRT-67)
- `crates/hearth-agent/src/application/service_logs.rs::{ServiceLogs::acquire, LogSession (Drop)}`
- `crates/hearth-proto/src/api/service_logs.rs::MAX_READS`

## Vérification
- `apps/desktop/src-tauri/tests/service_logs_runtime.rs` : `a_reloaded_page_closes_what_the_previous_one_left_and_an_old_page_cannot_open_again`, `a_reload_in_one_window_leaves_the_reads_of_another_window_alone`, `a_session_whose_page_went_silent_is_closed_after_the_timeout_and_a_beat_keeps_it`, `a_destroyed_window_and_the_exit_close_everything_without_a_word`
- `apps/desktop/e2e/hrt67-shell.spec.ts` : « rechargement », « fenêtre cachée »
- `crates/hearth-agent/tests/service_logs_ws.rs::only_ten_readings_are_open_at_once_and_closing_one_gives_its_place_back`
- `crates/hearth-agent/tests/service_logs_ws.rs::the_place_of_a_reading_comes_back_whatever_ends_it`
- `crates/hearth-agent/tests/service_logs_ws.rs::a_revoked_session_closes_its_reading_and_gives_the_place_back`
- `crates/hearth-agent/tests/service_logs_ws.rs::a_silent_client_loses_its_reading_and_the_agent_shutting_down_closes_it_going_away`

## Règles liées
- BR-SLOG-007
- BR-SLOG-026
- BR-SLOG-027

## Historique
- 2026-10-14 : côté client, plus aucune session orpheline (revue de la PR 95).
- 2026-10-13 — création (HRT-66, tranche 4 des déploiements : journaux en direct, côté agent et protocole).
- 2026-10-13 — corrections de la review locale de HRT-66 : la place revient par tous les chemins de sortie (fermeture, coupure, session révoquée, silence, mauvaise demande, service disparu), testés.
