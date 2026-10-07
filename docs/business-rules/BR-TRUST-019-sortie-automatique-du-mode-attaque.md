---
id: BR-TRUST-019
domaine: TRUST
titre: Le mode attaque se désactive tout seul après 30 minutes sans tentative refusée, mesurées sur l'horloge monotone
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-06-fonctionnelle-poste-de-confiance.md (BR-TRUST-019) ; conception technique 2026-10-06 (0, 5.2 à 5.6, 5.8, 5.9, 6, 7, 8, 10, 13) ; Q7, Q9 à Q17 ; contexts/hearth/tickets/hrt/HRT-25.md ; ADR-0025
maj: 2026-10-07
---

# BR-TRUST-019 : Le mode attaque se désactive tout seul après 30 minutes sans tentative refusée, mesurées sur l'horloge monotone

## Règle
Une tentative refusée (connexion refusée `401` ou `429`, file pleine, session présentée seule refusée) repousse la sortie : le minuteur est en mémoire, sur l'horloge monotone de l'agent. Une tâche périodique (toutes les 10 s) arrête le mode quand `QUIET` = 30 minutes se sont écoulées : `attack_mode.auto_disable`, origine « système », `last_end: "auto"`. L'horloge murale (reculée d'un an, avancée, NTP perdu) n'a aucun effet.

- Le minuteur ne vit qu'en mémoire : un redémarrage du service le remet à zéro (le mode dure alors plus longtemps, jamais moins).
- Une fenêtre de redémarrage ne compte pas comme une période calme : à la reprise (BR-TRUST-020), le minuteur repart de zéro.
- Les fautes de frappe d'un poste légitime repoussent la sortie : conséquence dite. Le minuteur ne distingue pas l'attaquant d'un poste légitime que le mode bloque : un poste légitime bloqué qui réessaie repousse la sortie AUTANT que l'attaquant. Le client le dit à l'écran (carte « Mode attaque » quand le mode est actif : « Le mode s'arrête tout seul après 30 minutes sans tentative refusée. Un poste légitime que le mode bloque et qui réessaie repousse cette fin autant que l'attaquant. », suivi HRT-25 r2).
- Quand le mode s'arrête tout seul, le client l'annonce (message discret « L'attaque semble terminée. Le mode attaque s'est arrêté automatiquement. » et notification Windows, BR-TRUST-033), seulement s'il l'avait vu actif ou suspendu juste avant.

## Application (code)
- `crates/hearth-agent/src/domain/trust/attack_mode.rs::quiet_elapsed`.
- `crates/hearth-agent/src/application/attack_mode.rs::AttackModeService::{note_refusal, sweep, auto_disable}`.
- `crates/hearth-agent/src/entrypoint/tasks.rs::spawn_attack_sweep`.

## Vérification
- `attack_mode.rs` : `lockout_exit_1_the_mode_ends_by_itself_after_thirty_quiet_minutes`, `the_wall_clock_set_back_or_forward_never_moves_the_thirty_minutes`, `the_suspension_does_not_count_as_a_quiet_period_for_the_automatic_exit`, `the_refusals_of_sessions_are_journaled_in_a_bounded_way_and_push_the_automatic_exit_back`.
- Domaine : `domain::trust::attack_mode::tests::the_automatic_exit_is_due_after_exactly_thirty_quiet_minutes`.
- Client (HRT-26) : `apps/desktop/src-tauri/tests/security_alerts.rs::{the_automatic_end_is_announced_only_after_having_seen_the_mode_on, the_automatic_end_of_the_attack_mode_notifies}`, `src/pages/SecurityMode.test.ts` (« announces the automatic end… »).

## Cas limites
- Aucun cas limite propre à cette règle au-delà de ceux des règles liées.

## Règles liées
- BR-TRUST-020, 027, 030, ADR-0025.

## Historique
- 2026-10-07 : création (HRT-25, session 2026-10-04-hearth-creation, T34).
- 2026-10-07 : la phrase sur les postes légitimes bloqués, dite à l'écran ; l'annonce de la sortie automatique côté client (HRT-26, T38).
