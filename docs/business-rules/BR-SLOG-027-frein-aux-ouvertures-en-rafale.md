---
id: BR-SLOG-027
domaine: SLOG
titre: Un compte ouvre au plus douze lectures de journaux par minute ; un refus répété donne une seule entrée au journal d'activité
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-journaux-service.md (BR-SLOG-027, révisée le 2026-10-13 vers 18:30), HRT-107
maj: 2026-10-13
---

# BR-SLOG-027 : frein aux ouvertures en rafale

## Règle
Chaque compte a un seau de **12 ouvertures** de lectures de journaux, rendues **une toutes les 5 secondes** (12 par minute en régime établi ; valeurs de départ du propriétaire, à ajuster après mesure sur forge). Chaque tentative d'ouverture (reprise après une coupure comprise) coûte une ouverture, qu'elle réussisse ou soit refusée plus loin : sans cela, un compte refusé par le plafond (BR-SLOG-026) rouvrirait sans fin. Un seau vide répond `error { code: "LOGS_RATE_LIMITED", retry_after_s }` puis la fermeture 1000 ; `retry_after_s` (au moins 1) est le nombre de secondes avant qu'une ouverture soit rendue. Un compte au repos ne cumule pas plus de 12 ouvertures. Le frein passe AVANT le plafond de lectures du compte, lui-même avant le plafond du serveur. Texte porté par le client : « Trop d'ouvertures de journaux en peu de temps. Réessaie dans {n} secondes. ».

**Journal d'activité** : un refus par le frein n'est pas inscrit un par un. Quand un compte est refusé PLUS DE 30 fois en 10 minutes (compte glissant, mémoire bornée à 4 096 refus), UNE entrée `service.logs.throttled` (« Ouvertures de journaux freinées ») est écrite : compte, origine, résultat « Refusé : N ouvertures de journaux refusées en 10 minutes » (N = le nombre de refus de la fenêtre), cible la route `/services/{key}/logs` (jamais un service, une clé ni une ligne), puis plus rien pendant 10 minutes pour ce compte. Seuls les refus du frein comptent, pas ceux du plafond. L'état d'un compte est oublié quand il n'a plus de lecture, un seau plein et aucun refus récent.

## Application (code)
- `crates/hearth-agent/src/domain/services/log_brake.rs::{AccountBrake::try_open, BrakeSettings, OpenRefusal, REFUSALS_BEFORE_ENTRY, REFUSAL_WINDOW_MS}`
- `crates/hearth-agent/src/application/service_logs.rs::{ServiceLogs::reserve_account, OpenRefused}`
- `crates/hearth-agent/src/entrypoint/ws/service_logs.rs::journal_throttle`
- `crates/hearth-proto/src/api/service_logs.rs::{OPEN_BURST, OPEN_REFILL_S, LogsErrorCode::LogsRateLimited, LogsError::retry_after_s}`

## Vérification
- `crates/hearth-agent/src/domain/services/log_brake.rs` : `twelve_opens_in_a_burst_are_served_and_the_thirteenth_is_braked_with_its_wait`, `one_open_comes_back_every_five_seconds`, `the_wait_counts_down_to_the_next_token`, `an_idle_account_does_not_bank_more_than_its_burst`, `a_refusal_by_the_cap_still_costs_a_token`, `one_entry_after_thirty_refusals_then_silence_for_ten_minutes`, `thirty_refusals_or_fewer_write_nothing`, `a_new_entry_is_possible_after_the_ten_minutes_of_silence`, `the_remembered_refusals_are_bounded`
- `crates/hearth-agent/tests/service_logs_ws.rs` : `the_thirteenth_open_in_a_minute_is_braked_with_its_wait_and_other_accounts_are_not`, `an_open_comes_back_after_the_refill_period`, `a_reconnection_after_a_cut_counts_as_an_open`, `only_one_journal_entry_is_asked_for_after_thirty_refusals`, `a_braked_account_is_told_to_wait_and_the_journal_gets_one_entry`

## Règles liées
- BR-SLOG-020
- BR-SLOG-026
- BR-SLOG-022

## Historique
- 2026-10-13 : création (HRT-107, point « à fermer avant toute version publiée » : la review des journaux estimait, sans l'avoir mesuré, à 150 millions de messages par minute ce qu'un compte en lecture seule pouvait faire relire).
