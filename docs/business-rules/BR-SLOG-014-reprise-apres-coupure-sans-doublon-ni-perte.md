---
id: BR-SLOG-014
domaine: SLOG
titre: Après une coupure, la lecture reprend à la dernière ligne reçue sans doublon ni perte tant que l'heure du moteur n'a pas reculé ; sinon, ou si la ligne n'existe plus ou si le curseur n'est pas crédible, un trou et les dernières lignes
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-journaux-service.md (BR-SLOG-014), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §7, §9, §10, HRT-66
maj: 2026-10-14
---

# BR-SLOG-014 — Après une coupure, la lecture reprend à la dernière ligne reçue sans doublon ni perte tant que l'heure du moteur n'a pas reculé ; sinon, ou si la ligne n'existe plus ou si le curseur n'est pas crédible, un trou et les dernières lignes

## Règle
Le curseur est l'heure de la dernière ligne reçue (millisecondes) et son rang parmi les lignes de la même milliseconde, dans l'ordre du moteur ; chaque `lines` porte celui de sa dernière ligne. À la réouverture (`open { since }`), l'agent relit à partir de la seconde du curseur (le moteur ne filtre qu'à la seconde), saute jusqu'à la ligne du curseur comprise, et envoie la suite. Si le conteneur est un autre (créé après le curseur) : repère puis ses lignes depuis son début, sur 5 000 lignes au plus (`FROM_START_LINES`). Si la ligne du curseur n'existe plus (fichiers tournés) : `gap { reason: resume_lost }` puis les dernières lignes.

**Le curseur vient du client, il n'est jamais cru sur parole** (`check_resume`) : plus vieux que 5 minutes (`RESUME_MAX_AGE_MS`, de quoi couvrir la coupure attendue d'une mise à jour de l'agent), plus d'une seconde dans le futur (`CLOCK_SKEW_MS`), heure négative, ou rang supérieur à 20 000 (`RESUME_MAX_LINES`) : reprise perdue tout de suite, sans rien demander de large au moteur (`gap { resume_lost }` puis les dernières lignes). Toute requête au moteur qui porte `since` porte aussi un `tail` (`RESUME_WINDOW_LINES`, 25 000 messages) : le moteur ne décode jamais plus que cette fenêtre, et un curseur hors de la fenêtre est une reprise perdue. Si la fenêtre commence dans la milliseconde du curseur, l'agent relit la fenêtre sans suivre pour savoir si elle est pleine ; pleine, les rangs recalculés seraient sous-estimés et la reprise est perdue (limite : une fenêtre de 25 000 messages qui donne moins de lignes, messages longs fusionnés, peut passer pour non pleine). La fenêtre compte des MESSAGES du moteur, pas des lignes : un message pèse jusqu'à 16 Kio, donc 25 000 messages font environ 4 Mio de lignes ordinaires mais jusqu'à 400 Mio lus et jetés pour un service aux très longues lignes (le double avec la relecture de comptage) ; la borne ne borne pas les octets. Pire cas d'une ouverture : 25 000 messages relus, plus les 1 000 dernières lignes rechargées après une reprise perdue, soit 26 000 ; 50 000 messages dans ce dernier cas (relecture de comptage comprise), soit 51 000 avec les 1 000 dernières lignes. Conséquence : un lecteur en retard de plus de 25 000 messages reçoit `resume_lost` puis 1 000 lignes, même avant les 5 minutes ; la promesse de couvrir la coupure de 2 minutes d'une mise à jour ne vaut que sous 200 messages par seconde environ (80 pour 5 minutes), au-delà c'est la fenêtre qui décide. Le rythme des ouvertures n'est pas encore freiné (ticket du plafond par compte, M10). La relecture qui vise un curseur valide s'arrête, comme perdue, après 20 000 lignes sautées sans l'avoir atteint (rien n'a encore été envoyé à ce stade). Au-delà du curseur, les lignes lues sont celles écrites depuis : au plus 5 minutes d'écriture, dont l'anneau ne garde que les 5 000 dernières (`gap { slow }`).

**Limites exactes** : (1) si les 1 000 lignes de la première page tiennent dans la même milliseconde, le rang de la dernière est sous-estimé et une reprise peut répéter des lignes de ce groupe (des doublons, jamais une perte) ; (2) « sans perte » suppose que l'heure du moteur ne recule pas : si elle recule de plus d'une seconde, `RankTracker` ramène les lignes suivantes à l'heure haute déjà vue et le moteur écarte, à la relecture `since`, les lignes écrites avec une heure plus basse. L'agent le détecte (`RankTracker::rewound`) pour ses propres relectures et repart alors des dernières lignes avec `gap { resume_lost }` ; un curseur déjà remis au client dans cette période n'est repris qu'une fois l'heure rattrapée (plus d'une seconde dans le futur : perdu tout de suite) ; une ligne écrite dans la seconde de recul peut être sautée sans trou annoncé. L'affichage « Vu il y a » et le trou d'écran sont portés par le client.

**Reprises du client (HRT-67, ADR-0040) : bornées.** Le client rouvre un socket tombé avec le curseur de la dernière ligne reçue, mais chaque ouverture consomme une ouverture du frein de l'agent (BR-SLOG-027) : délai croissant 1, 2, 4, 8, 16 puis 30 s plus un aléa de 0 à 25 % (jamais retiré), au plus 6 réouvertures de suite, puis fin « injoignable » et un bouton « Réessayer ». Le compte ne repart de zéro qu'après une connexion qui a tenu 30 s : recevoir un message ne suffit pas. Un `503` ou un `429` est attendu AU MOINS le délai demandé par l'agent (jamais plafonné à 30 s), 3 essais au plus ; au-delà d'une heure demandée, la reprise automatique s'arrête. Constantes dans `crates/hearth-link/src/manager/service_logs_retry.rs`.

## Application (code)
- `crates/hearth-link/src/manager/service_logs_retry.rs::{Retry, growing_wait, MAX_FAILURES, MAX_WAIT, STABLE_AFTER, JITTER_PERCENT, MAX_BUSY, MAX_ASKED_WAIT}` et `crates/hearth-link/src/manager/service_logs.rs::run` (HRT-67)
- `crates/hearth-agent/src/domain/services/logs.rs::{RankTracker, ResumeFilter, check_resume, RESUME_MAX_AGE_MS, RESUME_MAX_LINES, CLOCK_SKEW_MS, FROM_START_LINES, RESUME_WINDOW_LINES}`
- `crates/hearth-agent/src/application/service_logs.rs::Reader::{plan, follow_container}`
- `crates/hearth-proto/src/api/service_logs.rs::{LogCursor, LogsClientMessage}`

## Vérification
- `crates/hearth-link/src/manager/service_logs_retry.rs` (tests) : `a_socket_that_falls_in_a_loop_gives_up_after_six_reopenings`, `a_connection_that_held_long_enough_clears_the_count`, `a_busy_answer_is_waited_at_least_as_asked_even_beyond_thirty_seconds`, `busy_answers_are_retried_fewer_times_than_cuts_and_never_in_a_burst`
- `crates/hearth-agent/tests/service_logs_ws.rs::a_resume_gives_exactly_what_follows_the_cursor_even_inside_one_millisecond`
- `crates/hearth-agent/tests/service_logs_ws.rs::a_resume_whose_line_the_engine_no_longer_has_says_so_and_reloads_the_last_lines`
- `crates/hearth-agent/tests/service_logs_ws.rs::a_resume_that_is_up_to_date_does_not_resend_anything_when_the_service_stops`
- `crates/hearth-agent/src/domain/services/logs.rs::tests::a_resume_skips_up_to_the_cursor_line_included_and_takes_what_follows`
- `crates/hearth-agent/src/domain/services/logs.rs::tests::a_resume_whose_line_is_gone_is_lost_not_silently_wrong`
- `crates/hearth-agent/src/domain/services/logs.rs::tests::a_resume_cursor_is_trusted_only_inside_the_window`
- `crates/hearth-agent/src/domain/services/logs.rs::tests::the_clock_going_back_by_more_than_a_second_is_seen`
- `crates/hearth-agent/tests/service_logs_ws.rs::a_cursor_that_cannot_be_trusted_is_a_lost_resume_and_never_makes_the_engine_reread_the_journal`
- `crates/hearth-agent/tests/service_logs_ws.rs::ten_forged_resumes_at_once_cost_the_engine_ten_first_pages_not_ten_journals`
- `crates/hearth-agent/tests/service_logs_ws.rs::a_cursor_the_journal_never_reaches_is_given_up_after_a_bounded_number_of_lines`
- `crates/hearth-agent/tests/service_logs_ws.rs::when_the_engine_clock_goes_back_the_next_read_says_so_instead_of_staying_silent`
- job `engine` de la CI : `tests/support/engine_contract.rs::logs` (reprise) contre un vrai moteur

## Règles liées
- BR-SLOG-009, BR-SLOG-021

## Historique
- 2026-10-14 : reprises du client bornées (recul exponentiel avec aléa, essais comptés sans être remis à zéro par un message), `503` et `429` attendus en entier (revue de la PR 95).
- 2026-10-13 — création (HRT-66, tranche 4 des déploiements : journaux en direct, côté agent et protocole).
- 2026-10-13 — corrections de la review locale de HRT-66 : reprise bornée (fenêtre de 5 minutes, curseur futur ou illisible, 20 000 lignes sautées au plus, conteneur recréé lu sur 5 000 lignes au plus) ; limite « horloge qui recule » écrite et détectée.
