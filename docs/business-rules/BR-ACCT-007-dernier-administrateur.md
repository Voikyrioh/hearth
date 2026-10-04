---
id: BR-ACCT-007
domaine: ACCT
titre: Il doit toujours rester au moins un administrateur
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-gerer-comptes.md (BR-ACCT-007), HRT-03
maj: 2026-10-04
---

# BR-ACCT-007 — Il doit toujours rester au moins un administrateur

## Règle
Un compte administrateur ne peut être ni supprimé ni rétrogradé s'il est le seul administrateur. Le comptage et l'écriture ont lieu dans la même transaction d'écriture (`BEGIN IMMEDIATE`) : deux suppressions simultanées des deux derniers administrateurs, la seconde est refusée. Message : « Il doit toujours rester au moins un administrateur ».

## Application (code)
- `crates/hearth-agent/src/domain/accounts/admin_guard.rs::check_removal` — suppression.
- `crates/hearth-agent/src/domain/accounts/admin_guard.rs::check_role_change` — changement de rôle.
- `crates/hearth-agent/src/application/accounts.rs::AccountService::delete` et `::change_role` — comptent les administrateurs via `AccountTransaction::count_admins`, appellent les fonctions du domaine, puis valident ou abandonnent.

## Vérification
- Tests : `domain::accounts::admin_guard::tests` ; `tests/accounts_use_cases.rs::the_last_administrator_cannot_be_removed_nor_demoted`, `two_simultaneous_removals_of_the_last_two_administrators_leave_one` ; `tests/account_cli.rs::removing_the_last_administrator_is_refused`.

## Cas limites
- Un compte lecture seule se supprime toujours.
- Promotion et rôle inchangé ne sont jamais bloqués.
- La garde s'applique aussi à la ligne de commande (BR-ACCT-015).

## Règles liées
- BR-ACCT-015.

## Historique
- 2026-10-04 — création (HRT-03, session 2026-10-04-hearth-creation).
