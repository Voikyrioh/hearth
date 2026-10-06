---
id: BR-ACCT-007
domaine: ACCT
titre: Il doit toujours rester au moins un administrateur
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-gerer-comptes.md (BR-ACCT-007), HRT-03
maj: 2026-10-06
---

# BR-ACCT-007 — Il doit toujours rester au moins un administrateur

## Règle
Un compte administrateur ne peut être ni supprimé ni rétrogradé s'il est le seul administrateur. Le comptage et l'écriture ont lieu dans la même transaction d'écriture (`BEGIN IMMEDIATE`) : deux suppressions simultanées des deux derniers administrateurs, la seconde est refusée. Message : « Il doit toujours rester au moins un administrateur ».

## Application (code)
- `crates/hearth-agent/src/domain/accounts/admin_guard.rs::check_removal` — suppression.
- `crates/hearth-agent/src/domain/accounts/admin_guard.rs::check_role_change` — changement de rôle.
- `crates/hearth-agent/src/application/accounts.rs::AccountService::delete` et `::change_role` — comptent les administrateurs via `uow.accounts().count_admins()` (`AccountTx`), appellent les fonctions du domaine, puis valident ou abandonnent.
- Routes : `PATCH /api/v1/accounts/{id}` et `DELETE /api/v1/accounts/{id}` ; refus `409 LAST_ADMIN`.

## Interface (HRT-13)
`AccountTable.vue` grise « Changer le rôle » et « Supprimer » du dernier administrateur avec l'explication ; l'agent reste l'arbitre : son refus `LAST_ADMIN` s'affiche « Il doit toujours rester au moins un administrateur » (et « Tu es le dernier administrateur, ce compte ne peut pas être supprimé » pour son propre compte), la liste est relue. Tests : `Accounts.test.ts::explains the last administrator when the agent refuses a removal`, `accounts_runtime.rs::roles_change_and_the_last_administrator_can_be_neither_demoted_nor_removed`.

## Vérification
- Tests : `domain::accounts::admin_guard::tests` ; `crates/hearth-agent/tests/accounts_use_cases.rs::the_last_administrator_cannot_be_removed_nor_demoted`, `two_simultaneous_removals_of_the_last_two_administrators_leave_one` ; `crates/hearth-agent/tests/account_cli.rs::removing_the_last_administrator_is_refused`.

## Cas limites
- Un compte lecture seule se supprime toujours.
- Promotion et rôle inchangé ne sont jamais bloqués.
- La garde s'applique aussi à la ligne de commande (BR-ACCT-015).

## Règles liées
- BR-ACCT-015.

## Historique
- 2026-10-04 — création (HRT-03, session 2026-10-04-hearth-creation).
- 2026-10-06 — section « Interface » (HRT-13, session 2026-10-04-hearth-creation).
