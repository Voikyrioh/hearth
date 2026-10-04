---
id: BR-ACCT-010
domaine: ACCT
titre: Supprimer un compte ferme ses sessions
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-gerer-comptes.md (BR-ACCT-010), HRT-03
maj: 2026-10-04
---

# BR-ACCT-010 — Supprimer un compte ferme ses sessions

## Règle
La suppression d'un compte ferme immédiatement toutes ses sessions ouvertes, dans la même transaction. La clé étrangère `sessions.account_id` est en `ON DELETE CASCADE` en filet de sécurité.

## Application (code)
- `crates/hearth-agent/src/domain/sessions.rs::closure_on_account_deletion`.
- `crates/hearth-agent/src/application/accounts.rs::AccountService::delete`.

## Vérification
- Tests : `domain::sessions::tests::deletion_and_revocation_close_every_session` ; `tests/accounts_use_cases.rs::removing_an_account_closes_its_sessions`.

## Cas limites
- Suppression refusée (dernier administrateur) → les sessions restent ouvertes.

## Règles liées
- BR-ACCT-007, BR-ACCT-012.

## Historique
- 2026-10-04 — création (HRT-03, session 2026-10-04-hearth-creation).
