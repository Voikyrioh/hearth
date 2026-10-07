---
id: BR-TRUST-020
domaine: TRUST
titre: Un démarrage de la machine suspend le mode attaque 30 minutes (régime d'alerte), puis il reprend, sans horloge murale
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-06-fonctionnelle-poste-de-confiance.md (BR-TRUST-020) ; conception technique 2026-10-06 (0, 5.2 à 5.6, 5.8, 5.9, 6, 7, 8, 10, 13) ; Q7, Q9 à Q17 ; contexts/hearth/tickets/hrt/HRT-25.md ; ADR-0025
maj: 2026-10-07
---

# BR-TRUST-020 : Un démarrage de la machine suspend le mode attaque 30 minutes (régime d'alerte), puis il reprend, sans horloge murale

## Règle
**Détection fiable** : le noyau tire un identifiant à chaque démarrage (`/proc/sys/kernel/random/boot_id`). Au lancement du service, `on_start` le compare à celui noté en base (`attack_mode.last_boot_id`). Différent, mode actif, redémarrage non demandé par Hearth, et la machine vient de démarrer (temps écoulé < 30 minutes) : la fenêtre s'ouvre (`window_boot_id`), `attack_mode.suspend` est consigné (origine système, BR-TRUST-031).

- **État effectif** (calculé, jamais stocké) : suspendu tant que `window_boot_id` est le démarrage courant et que le temps écoulé depuis le démarrage du noyau (`/proc/uptime`) est inférieur à 30 minutes ; `resumes_in_s` est la durée exacte restante. Aucune horloge murale : la reculer d'un an ne change rien. Un service qui redémarre dans la fenêtre la retrouve avec la durée restante exacte ; un service qui démarre tard ne la rallonge pas.
- **Pendant la fenêtre**, le mode se comporte comme une ALERTE : personne n'est bloqué, le ralentissement par identifiant s'applique, une session seule fonctionne. L'inscription de nouveaux postes reste gelée.
- **Reprise** : à 30 minutes, même `activation_id`, essais non rendus, `attack_mode.resume` (constatée par la tâche périodique, au plus 10 s après), message de flux.
- **Désactivé pendant la fenêtre** : le mode n'est pas rouvert après.
- **Identifiant illisible** : jamais de fenêtre, un avertissement au démarrage (l'inverse l'ouvrirait à chaque mise à jour de l'agent, qu'une session administrateur déclenche à distance). Il reste `hearth-agent attack-mode off`.
- **Redémarrage demandé par Hearth** : si `remote_reboot_boot_id` (noté AVANT l'appel au système par la future commande de redémarrage, dans une écriture validée) est le démarrage qui vient de finir, aucune fenêtre. Aucune commande de redémarrage n'existe aujourd'hui ; la colonne existe, rien ne l'écrit encore (ADR-0025).
- **Pas de plafond** du nombre de fenêtres (risque « prise connectée » assumé, Q14 point 4) : qui peut couper le courant gagne 30 minutes de régime d'alerte à chaque fois ; chaque suspension est journalisée.

## Application (code)
- `crates/hearth-agent/src/domain/trust/attack_mode.rs::{effective, on_start, window_over}`.
- `crates/hearth-agent/src/application/attack_mode.rs::AttackModeService::{on_start, sweep, resume}`.
- `crates/hearth-agent/src/infrastructure/system/boot.rs::ProcBootInfo` (port `BootInfo`).
- `crates/hearth-agent/src/app.rs::start_full` (appelle `on_start` avant d'accepter la moindre requête).

## Vérification
- Domaine : `domain::trust::attack_mode::tests::{the_effective_state_over_every_combination_of_window_boot_and_uptime, a_new_boot_of_the_machine_opens_the_window_for_that_boot_only, an_unreadable_boot_identifier_never_opens_a_window, a_reboot_asked_by_hearth_opens_no_window_and_the_mark_is_cleared, a_backup_restored_long_after_boot_keeps_the_mode_active_without_a_window, the_end_of_the_window_is_detected_once_and_not_while_it_applies}`.
- `attack_mode.rs` : `the_window_lasts_thirty_minutes_of_uptime_then_the_mode_resumes_with_the_same_activation_and_no_trial_back`, `the_window_is_measured_on_the_uptime_not_on_the_wall_clock`, `a_service_restart_during_the_window_finds_the_exact_remaining_time_and_never_extends_it`, `a_disable_during_the_window_is_not_reopened_after_it`, `an_unreadable_boot_identifier_never_opens_a_window_and_never_closes_the_mode`, `a_reboot_asked_by_hearth_opens_no_window`, `a_backup_restored_while_the_mode_was_active_neither_locks_nor_silently_ends_the_mode`, `lockout_exit_3_a_physical_reboot_suspends_the_mode_and_the_right_password_passes`.
- Adaptateur : `infrastructure::system::boot::tests::*`.
- Non testable en conteneur (le `boot_id` y est celui de l'hôte) : fumée manuelle sur la forge (ADR-0025).

## Cas limites
- Un redémarrage lancé par un autre moyen distant que Hearth (SSH, mise à jour automatique avec redémarrage, coupure de courant) ouvre la fenêtre : « personne physique » est approché par « pas demandé par Hearth ».
- Base restaurée d'une sauvegarde où le mode était actif : machine démarrée depuis longtemps, pas de fenêtre et le mode reste actif ; machine tout juste démarrée, la fenêtre s'ouvre.

## Règles liées
- BR-TRUST-019, 021, 027, 031, ADR-0012, ADR-0025.

## Historique
- 2026-10-07 : création (HRT-25, session 2026-10-04-hearth-creation, T34).
