---
id: BR-TRUST-021
domaine: TRUST
titre: Un redémarrage du service de l'agent, ou une mise à jour de l'agent sans redémarrage de la machine, ne suspend pas le mode attaque
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-06-fonctionnelle-poste-de-confiance.md (BR-TRUST-021) ; conception technique 2026-10-06 (0, 5.2 à 5.6, 5.8, 5.9, 6, 7, 8, 10, 13) ; Q7, Q9 à Q17 ; contexts/hearth/tickets/hrt/HRT-25.md ; ADR-0025
maj: 2026-10-07
---

# BR-TRUST-021 : Un redémarrage du service de l'agent, ou une mise à jour de l'agent sans redémarrage de la machine, ne suspend pas le mode attaque

## Règle
Le mode attaque est persistant (SQLite) : compteurs, mode, identifiant d'activation, essais, postes, adresses. Le service qui redémarre (crash, boucle de redémarrage par un attaquant, mise à jour de l'agent) retrouve le mode actif **sans fenêtre** (le `boot_id` n'a pas changé), sans rendre un seul essai, avec le même `activation_id`. Seul le minuteur de sortie automatique repart de zéro (BR-TRUST-019).

## Application (code)
- `crates/hearth-agent/src/domain/trust/attack_mode.rs::on_start` (identifiant inchangé : aucune fenêtre).
- `crates/hearth-agent/src/application/attack_mode.rs::AttackModeService::on_start`.

## Vérification
- `attack_mode.rs` : `a_service_restarted_in_a_loop_gives_no_trial_back_and_opens_no_window`, `a_service_restart_during_the_window_finds_the_exact_remaining_time_and_never_extends_it`.
- Domaine : `domain::trust::attack_mode::tests::a_service_restart_on_the_same_boot_opens_no_window_and_keeps_the_mode`.

## Cas limites
- Aucun cas limite propre à cette règle au-delà de ceux des règles liées.

## Règles liées
- BR-TRUST-019, 020, ADR-0025.

## Historique
- 2026-10-07 : création (HRT-25, session 2026-10-04-hearth-creation, T34).
