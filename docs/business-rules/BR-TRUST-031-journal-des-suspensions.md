---
id: BR-TRUST-031
domaine: TRUST
titre: Le journal d'activité enregistre tout redémarrage de la machine qui suspend le mode attaque, et la reprise
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-06-fonctionnelle-poste-de-confiance.md (BR-TRUST-031) ; conception technique 2026-10-06 (0, 5.2 à 5.6, 5.8, 5.9, 6, 7, 8, 10, 13) ; Q7, Q9 à Q17 ; contexts/hearth/tickets/hrt/HRT-25.md ; ADR-0025
maj: 2026-10-07
---

# BR-TRUST-031 : Le journal d'activité enregistre tout redémarrage de la machine qui suspend le mode attaque, et la reprise

## Règle
`attack_mode.suspend` (origine système) quand la fenêtre s'ouvre au lancement du service ; `attack_mode.resume` (origine système) quand elle finit. Une entrée par événement, jamais plus : la reprise est constatée une seule fois (la marque de fenêtre est effacée dans la même transaction). Le tout est dit au flux (`security`).

## Application (code)
- `crates/hearth-agent/src/application/attack_mode.rs::AttackModeService::{on_start, resume}`.

## Vérification
- `attack_mode.rs` : `the_window_lasts_thirty_minutes_of_uptime_then_the_mode_resumes_with_the_same_activation_and_no_trial_back`, `lockout_exit_3_a_physical_reboot_suspends_the_mode_and_the_right_password_passes`, `a_machine_reboot_with_the_mode_off_opens_nothing`.

## Cas limites
- Aucun cas limite propre à cette règle au-delà de ceux des règles liées.

## Règles liées
- BR-TRUST-020, 030, ADR-0025.

## Historique
- 2026-10-07 : création (HRT-25, session 2026-10-04-hearth-creation, T34).
