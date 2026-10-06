---
id: BR-ACCT-008
domaine: ACCT
titre: Changer le mot de passe d'un autre compte ferme ses sessions
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-gerer-comptes.md (BR-ACCT-008), HRT-03
maj: 2026-10-06
---

# BR-ACCT-008 — Changer le mot de passe d'un autre compte ferme ses sessions

## Règle
Un administrateur (ou la ligne de commande) qui définit le mot de passe d'un compte ferme immédiatement toutes les sessions ouvertes de ce compte. Le changement du mot de passe et la fermeture des sessions sont atomiques (une seule transaction).

## Application (code)
- `crates/hearth-agent/src/domain/sessions.rs::closure_on_password_change` (`PasswordChange::ByAdmin` → `SessionClosure::All`).
- `crates/hearth-agent/src/application/accounts.rs::AccountService::set_password`.
- Route : `PUT /api/v1/accounts/{id}/password` (`entrypoint/http/accounts.rs::set_password`, `docs/open-api/accounts.md`) ; le jeton fermé répond ensuite `SESSION_REVOKED` (BR-RESIL-014).

## Interface (HRT-13)
Bouton « Mot de passe » d'une ligne → `PasswordDialog.vue` (nouveau + confirmation, sans ancien) → `useAccountActions.setPassword` → commande `set_account_password` (`PUT /accounts/{id}/password`). Succès : « Mot de passe changé ». Test : `accounts_runtime.rs::an_admin_setting_a_password_closes_every_session_of_that_account_and_only_that_one`.

## Vérification
- Tests : `domain::sessions::tests::admin_password_change_closes_every_session` ; `crates/hearth-agent/tests/accounts_use_cases.rs::an_administrator_password_change_closes_every_session`.

## Cas limites
- Si la transaction échoue, ni le mot de passe ni les sessions ne changent.

## Règles liées
- BR-ACCT-009, BR-ACCT-011.

## Historique
- 2026-10-04 — création (HRT-03, session 2026-10-04-hearth-creation).
- 2026-10-06 — section « Interface » (HRT-13, session 2026-10-04-hearth-creation).
