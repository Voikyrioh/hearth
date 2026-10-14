---
id: BR-SLOG-026
domaine: SLOG
titre: Un compte lit en direct les journaux de trois services au plus en même temps, tous postes confondus
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-journaux-service.md (BR-SLOG-026, révisée le 2026-10-13 vers 18:30), HRT-107
maj: 2026-10-14
---

# BR-SLOG-026 : trois lectures en direct par compte

## Règle
Un compte tient au plus **3 lectures de journaux en même temps**, tous postes confondus (valeur de départ décidée par le propriétaire le 2026-10-13, à ajuster après mesure sur forge). La quatrième ouverture répond `error { code: "LOGS_ACCOUNT_LIMIT" }` puis la fermeture 1000, sans attente : fermer une lecture (proprement ou non) rend la place tout de suite. Le plafond du serveur (dix lectures, BR-SLOG-020) reste, et se vérifie APRÈS celui du compte : un refus du serveur ne garde aucune place au compte. Le texte « Tu lis déjà les journaux de 3 services. Ferme un onglet Journaux pour en ouvrir un autre. » est porté par le client. Un refus de ce plafond n'est pas inscrit au journal d'activité (BR-SLOG-022 : lire n'est pas journalisé).

**Côté client (HRT-67, ADR-0040)** : la coquille du PC tient elle aussi un plafond, PAR SERVEUR et égal à celui de l'agent (`MAX_SESSIONS_PER_SERVER`, repris de `MAX_READS_PER_ACCOUNT`), plus une garde globale de 12 (`MAX_SESSIONS_TOTAL`). Au-delà, elle refuse (`LogReadsFull`) sans rien envoyer au serveur, et l'écran le dit avec ce qu'il faut faire : « Trois lectures de journaux sont déjà ouvertes depuis ce PC pour ce serveur. Une lecture oubliée se ferme seule en moins d'une minute : attends un peu, puis réessaie. » Ce refus ne se confond jamais avec « Impossible de lire ».

## Application (code)
- `apps/desktop/src-tauri/src/services/logs.rs::{LogSessions::open, MAX_SESSIONS_PER_SERVER, MAX_SESSIONS_TOTAL}` (HRT-67)
- `crates/hearth-agent/src/domain/services/log_brake.rs::{AccountBrake::try_open, AccountBrake::release, BrakeSettings}`
- `crates/hearth-agent/src/application/service_logs.rs::{ServiceLogs::reserve_account, AccountSlot}`
- `crates/hearth-proto/src/api/service_logs.rs::{MAX_READS_PER_ACCOUNT, LogsErrorCode::LogsAccountLimit}`

## Vérification
- `apps/desktop/src-tauri/tests/service_logs_runtime.rs` : `a_fourth_session_on_a_server_is_refused_before_the_server_and_a_closed_one_gives_its_place_back`, `the_global_guard_refuses_beyond_its_number`, `the_shell_ceiling_per_server_is_the_one_of_the_agent_and_a_guard_sits_above_it`
- `apps/desktop/e2e/hrt67-shell.spec.ts` : « la coquille refuse faute de place »
- `crates/hearth-agent/src/domain/services/log_brake.rs` : `a_fourth_live_reading_is_refused_and_closing_one_gives_the_place_back`, `a_refused_open_does_not_count_as_a_reading`
- `crates/hearth-agent/tests/service_logs_ws.rs` : `a_fourth_live_reading_of_one_account_is_refused_and_a_close_gives_the_place_back`, `a_refusal_by_the_server_cap_does_not_keep_a_place_for_the_account`, `the_socket_answers_the_account_limit_with_its_code_and_no_wait`

## Règles liées
- BR-SLOG-020
- BR-SLOG-027

## Historique
- 2026-10-14 : plafond de la coquille par serveur et garde globale, refus dit à l'écran (revue de la PR 95).
- 2026-10-13 : création (HRT-107, réponse 13 du propriétaire : « oui on peut mettre un plafond avec un fichier plus complet téléchargeable »).
