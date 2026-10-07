---
id: BR-ACCT-010
domaine: ACCT
titre: Supprimer un compte ferme ses sessions
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-gerer-comptes.md (BR-ACCT-010), HRT-03
maj: 2026-10-06
---

# BR-ACCT-010 — Supprimer un compte ferme ses sessions

## Règle
La suppression d'un compte ferme immédiatement toutes ses sessions ouvertes, dans la même transaction. La clé étrangère `sessions.account_id` est en `ON DELETE CASCADE` en filet de sécurité.

## Application (code)
- `crates/hearth-agent/src/domain/sessions.rs::closure_on_account_deletion`.
- `crates/hearth-agent/src/application/accounts.rs::AccountService::delete`.
- Route : `DELETE /api/v1/accounts/{id}` (`entrypoint/http/accounts.rs::delete`).

## Interface (HRT-13)
Bouton « Supprimer » → `ConfirmDialog` qui nomme le compte (« Supprimer le compte <identifiant> ? Cette action est irréversible. ») → commande `delete_account`. Succès : « Compte <identifiant> supprimé ». Test : `accounts_runtime.rs::deleting_an_account_closes_its_sessions_and_an_unknown_one_is_refused`.

## Vérification
- Tests : `domain::sessions::tests::deletion_and_revocation_close_every_session` ; `crates/hearth-agent/tests/accounts_use_cases.rs::removing_an_account_closes_its_sessions`.

## Cas limites
- Suppression refusée (dernier administrateur) → les sessions restent ouvertes.

## Règles liées
- BR-ACCT-007, BR-ACCT-012.

## Historique
- 2026-10-04 — création (HRT-03, session 2026-10-04-hearth-creation).
- 2026-10-06 — section « Interface » (HRT-13, session 2026-10-04-hearth-creation).
- 2026-10-07 : HRT-28 : la suppression accepte un membre `reauth` (BR-TRUST-036) ; couverte par l'élévation (BR-TRUST-043).
