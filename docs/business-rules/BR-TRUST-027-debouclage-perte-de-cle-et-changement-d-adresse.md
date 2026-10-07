---
id: BR-TRUST-027
domaine: TRUST
titre: Un poste qui perd sa clé et son adresse pendant le mode attaque a trois voies de sortie : un autre poste reconnu, le redémarrage physique, la commande sur le serveur
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-06-fonctionnelle-poste-de-confiance.md (BR-TRUST-027) ; conception technique 2026-10-06 (0, 5.2 à 5.6, 5.8, 5.9, 6, 7, 8, 10, 13) ; Q7, Q9 à Q17 ; contexts/hearth/tickets/hrt/HRT-25.md ; ADR-0025
maj: 2026-10-07
---

# BR-TRUST-027 : Un poste qui perd sa clé et son adresse pendant le mode attaque a trois voies de sortie : un autre poste reconnu, le redémarrage physique, la commande sur le serveur

## Règle
Un poste reconnu dont la clé ET l'adresse ont changé perd l'accès jusqu'à la fin du mode (aucun critère). S'il a gardé sa clé, il a l'essai unique de la clé (BR-TRUST-012) ; sinon :

1. (a) l'accès depuis un autre poste reconnu (désactivation par un administrateur, BR-TRUST-018) ;
2. (b) le redémarrage physique de la machine : fenêtre de 30 minutes en régime d'alerte (BR-TRUST-020) ;
3. (c) `hearth-agent attack-mode off` sur le serveur (root, accès direct à la base, sans réseau, consigné `attack_mode.disable` origine « ligne de commande ») ; `attack-mode status` donne l'état, l'identifiant d'activation et le temps restant d'une suspension.

S'y ajoute la sortie automatique après 30 minutes sans tentative refusée (BR-TRUST-019). Il n'existe pas de sous-commande pour activer : activer exige un poste avec sa clé.

Le service lit la ligne du mode à chaque décision : la sous-commande, un autre processus, est prise en compte tout de suite, et le flux est prévenu à la passe suivante de la tâche périodique.

## Application (code)
- `crates/hearth-agent/src/entrypoint/attack_mode.rs::execute`, `crates/hearth-agent/src/entrypoint/cli.rs::AttackModeAction`.
- `crates/hearth-agent/src/application/attack_mode.rs::AttackModeService::disable_from_cli`.

## Vérification
- `attack_mode.rs` : `lockout_exit_1_the_mode_ends_by_itself_after_thirty_quiet_minutes`, `lockout_exit_2_the_local_command_ends_the_mode_without_the_network`, `lockout_exit_3_a_physical_reboot_suspends_the_mode_and_the_right_password_passes`.
- `attack_mode_cli.rs` : le vrai binaire (`status_tells_the_mode_and_the_activation_and_off_ends_it_with_a_journal_entry`, `there_is_no_subcommand_to_enable_the_mode`).
- `attack_mode_http.rs` : `the_local_command_is_seen_at_once_by_the_routes_of_a_running_agent`.

## Cas limites
- Aucun cas limite propre à cette règle au-delà de ceux des règles liées.

## Règles liées
- BR-TRUST-012, 018, 019, 020, 028, ADR-0025.

## Historique
- 2026-10-07 : création (HRT-25, session 2026-10-04-hearth-creation, T34).
