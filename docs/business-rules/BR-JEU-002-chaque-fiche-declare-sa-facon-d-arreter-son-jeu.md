---
id: BR-JEU-002
domaine: JEU
titre: Chaque fiche déclare la façon d'arrêter son jeu : une suite d'étapes bornée, des littéraux de la fiche, jamais saisie
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-serveur-de-jeu-au-quotidien.md (BR-JEU-002), contexts/hearth/conceptions/2026-10-13-technique-fiches-de-jeu.md, HRT-113
maj: 2026-10-13
---

# BR-JEU-002 — Chaque fiche déclare la façon d'arrêter son jeu : une suite d'étapes bornée, des littéraux de la fiche, jamais saisie

## Règle
La fiche dit si le jeu enregistre son monde à l'arrêt (`by_image_on_signal`), ne l'enregistre pas (`not_saved`) ou s'arrête par une SUITE d'étapes (`by_sequence`) : `stdin_line` (un ordre littéral de 64 caractères ASCII imprimables au plus), `wait_log` (sous-chaîne littérale, délai de 1 à 300 secondes, obligatoire ou non), `wait` (de 1 à 120 secondes). Six étapes au plus, vingt minutes au plus, délai d'arrêt de 10 à 600 secondes, signal parmi `image_default`, `SIGTERM`, `SIGINT`. L'ordre de sortie (`quits`) termine la suite et l'arrêt du moteur le suit AUSSITÔT avec le délai de la fiche. Une séquence d'enregistrement à chaud ne quitte jamais le jeu. Une fiche qui ne dit pas comment son jeu s'arrête est refusée (`ChampManquant`).

## Application (code)
- `crates/hearth-agent/src/domain/catalog/stop_plan.rs::{stop_plan, save_plan, StopPlan}`
- `crates/hearth-agent/src/domain/catalog/loading/invariants.rs::{check_stop, check_steps}`

## Vérification
- `crates/hearth-agent/src/domain/catalog/tests_state.rs::a_tested_sequence_ends_with_the_exit_order_then_the_engine_stop`
- `crates/hearth-agent/src/domain/catalog/tests_state.rs::every_plan_is_bounded_and_the_exit_order_always_comes_last`
- `crates/hearth-agent/src/domain/catalog/tests_fiches.rs::i4_nothing_is_executed_no_step_kind_runs_a_command`
- `crates/hearth-agent/src/domain/catalog/tests_fiches.rs::i9_a_fiche_cannot_exhaust_the_agent_or_the_screen_bounds`

## Cas limites
- Le déroulé de la séquence (envoi par l'entrée standard, suivi des journaux, décision, point de non-retour) est à HRT-103 ; ce ticket produit le PLAN.

## Règles liées
- BR-JEU-038, BR-JEU-040

## Historique
- 2026-10-13 — création (HRT-113, format de fiche version 2, domaine pur).
