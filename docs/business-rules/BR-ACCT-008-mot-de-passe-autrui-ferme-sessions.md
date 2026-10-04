---
id: BR-ACCT-008
domaine: ACCT
titre: Changer le mot de passe d'un autre compte ferme ses sessions
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-gerer-comptes.md (BR-ACCT-008), HRT-03
maj: 2026-10-04
---

# BR-ACCT-008 — Changer le mot de passe d'un autre compte ferme ses sessions

## Règle
Un administrateur (ou la ligne de commande) qui définit le mot de passe d'un compte ferme immédiatement toutes les sessions ouvertes de ce compte. Le changement du mot de passe et la fermeture des sessions sont atomiques (une seule transaction).

## Application (code)
- `crates/hearth-agent/src/domain/sessions.rs::closure_on_password_change` (`PasswordChange::ByAdmin` → `SessionClosure::All`).
- `crates/hearth-agent/src/application/accounts.rs::AccountService::set_password`.

## Vérification
- Tests : `domain::sessions::tests::admin_password_change_closes_every_session` ; `tests/accounts_use_cases.rs::an_administrator_password_change_closes_every_session`.

## Cas limites
- Si la transaction échoue, ni le mot de passe ni les sessions ne changent.

## Règles liées
- BR-ACCT-009, BR-ACCT-011.

## Historique
- 2026-10-04 — création (HRT-03, session 2026-10-04-hearth-creation).
