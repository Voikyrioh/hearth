---
id: FIX-01M4CJEQS88NZWCQ129XDP91MT
titre: Une coupure rétablie entre 1,5 s et 3 s faisait clignoter « Reconnexion en cours »
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4CJEQS88NZWCQ129XDP91MT : « Reconnexion » montrée pour une coupure de moins de 3 s

## Symptôme
Les tentatives de reconnexion partent à 0, 0,5, 1,5 puis 3,5 s. Une coupure rétablie à 1,8 s n'était retrouvée qu'à 3,5 s ; entre-temps le seuil de 3 s était franchi et l'écran passait à « Reconnexion en cours » pour revenir à « Connecté » 0,5 s plus tard. La spec (BR-RESIL-002) dit : moins de 3 s de coupure, rien ne change à l'écran.

## Reproduction
`crates/hearth-link/tests/link_timing.rs::a_cut_healed_at_2_9_seconds_shows_nothing` (temps virtuel, seuils du produit) : rouge avant (Reconnecting à 3 s, Connected à 3,5 s).

## Cause root
L'état affiché dérivait de la seule durée écoulée depuis le début de la coupure (`derive_down`), pas de la preuve que la coupure durait encore à 3 s.

## Impacté
Tout client depuis HRT-07 : coupure rétablie entre la deuxième et la troisième tentative.

## Workaround
Aucun.

## Correction
`LinkMachine::check_threshold` (`domain/state.rs`) : au seuil de 3 s, si rien ne prouve la coupure, une vérification part une fois (hors de la suite des délais). Réussie : rien n'est montré. Échouée : la coupure est établie (`Outage::confirmed`), « Reconnexion » est montrée datée de 3 s, et la tentative planifiée reprend sa place (`resume_at`). Valent preuve sans vérification : une tentative sans réponse au seuil, un silence de plus de 3 s, une réponse « attends ». Les tentatives gardent leurs délais. **La vérification est bornée** (le sixième du seuil, 0,5 s ; review r1 de la PR #45) : sans elle, un serveur qu'on redémarre (refus, puis silence) laissait l'écran à « Connecté » jusqu'au délai de la tentative (8 s, environ 11 s de coupure). Sans réponse à la borne, la coupure est établie. Toute sortie de la vérification (borne, « attends », tentative relancée) remet `probe` et `resume_at` à zéro (`end_probe`) : l'échec suivant ne saute plus son délai. `// FIX:01M4CJEQS88NZWCQ129XDP91MT` dans `state.rs` et `state/tests.rs`.

## Règles
- BR-RESIL-002 (précisée).

## Non-régression
- `domain::state::tests::a_cut_healed_just_before_3s_shows_nothing_even_if_the_next_attempt_comes_later`, `::a_cut_still_there_at_3s_shows_reconnecting_then_connected_and_keeps_its_delays` ; `state/tests.rs::a_silent_probe_cannot_hold_the_screen_on_connected_past_its_bound`, `::a_probe_ended_by_a_wait_answer_does_not_make_the_next_failure_skip_its_delay`, `::a_trigger_during_the_probe_makes_it_an_ordinary_attempt` ; `link_timing.rs::a_host_that_refuses_then_goes_silent_shows_reconnecting_at_3_seconds`, `::a_cut_healed_at_2_9_seconds_shows_nothing`, `::a_cut_healed_at_3_1_seconds_shows_reconnecting_then_connected`, `::the_attempts_follow_0_5_1_2_4_8_15_30_30_seconds_and_never_stop` (la vérification n'en décale aucune).

## Références
- Ticket : HRT-18 (tranche 6)
- Code : `crates/hearth-link/src/domain/state.rs`
