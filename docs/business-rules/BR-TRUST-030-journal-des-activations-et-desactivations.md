---
id: BR-TRUST-030
domaine: TRUST
titre: Le journal d'activité enregistre toute activation et désactivation du mode attaque, manuelle ou automatique
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-06-fonctionnelle-poste-de-confiance.md (BR-TRUST-030) ; conception technique 2026-10-06 (0, 5.2 à 5.6, 5.8, 5.9, 6, 7, 8, 10, 13) ; Q7, Q9 à Q17 ; contexts/hearth/tickets/hrt/HRT-25.md ; ADR-0025
maj: 2026-10-07
---

# BR-TRUST-030 : Le journal d'activité enregistre toute activation et désactivation du mode attaque, manuelle ou automatique

## Règle
Une entrée dans la transaction du changement : `attack_mode.enable` (administrateur et son poste), `attack_mode.disable` (administrateur, ou origine « ligne de commande »), `attack_mode.auto_disable` (origine **système**, `origin_kind = 'system'`, migration 0006). Un changement refusé (`403`, `409`, mot de passe faux) est consigné par la couche d'accès sous `attack_mode.change`, résultat « refusé » ou « échoué ». Activer un mode actif et désactiver un mode éteint n'écrivent rien. Aucun champ libre, aucun secret : types fermés (`AuditAction`, `Reason`, `Target`).

## Application (code)
- `crates/hearth-agent/src/domain/audit/{action.rs, event.rs}` (`AuditAction::AttackMode*`, `Origin::System`).
- `crates/hearth-agent/src/application/attack_mode.rs::AttackModeService::{change, auto_disable}`.

## Vérification
- `attack_mode.rs` : `enabling_and_disabling_are_idempotent_and_journaled_once_with_the_administrator`, `lockout_exit_1_the_mode_ends_by_itself_after_thirty_quiet_minutes`, `lockout_exit_2_the_local_command_ends_the_mode_without_the_network`.
- `attack_mode_http.rs` : `an_administrator_with_a_proved_key_and_the_password_enables_then_disables_the_mode`, `a_read_only_account_is_refused_with_403_and_the_refusal_is_journaled`.
- `tests/migration_0006.rs` (origine système).

## Cas limites
- Aucun cas limite propre à cette règle au-delà de ceux des règles liées.

## Règles liées
- BR-TRUST-018, 019, 028, 031, 032, BR-AUDIT-005, BR-AUDIT-007, ADR-0025.

## Historique
- 2026-10-07 : création (HRT-25, session 2026-10-04-hearth-creation, T34).
