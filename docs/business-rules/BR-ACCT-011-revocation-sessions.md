---
id: BR-ACCT-011
domaine: ACCT
titre: Révoquer les sessions d'un compte ferme toutes ses sessions sans changer le mot de passe
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-gerer-comptes.md (BR-ACCT-011), HRT-03
maj: 2026-10-04
---

# BR-ACCT-011 — Révoquer les sessions d'un compte ferme toutes ses sessions sans changer le mot de passe

## Règle
La révocation ferme immédiatement toutes les sessions ouvertes du compte et ne touche pas au mot de passe : le compte peut se reconnecter aussitôt. Message : « Sessions de <identifiant> fermées ».

## Application (code)
- `crates/hearth-agent/src/domain/sessions.rs::closure_on_revocation`.
- `crates/hearth-agent/src/application/accounts.rs::AccountService::revoke_sessions`.
- Route : `DELETE /api/v1/accounts/{id}/sessions` (`entrypoint/http/accounts.rs::revoke_sessions`).

## Vérification
- Tests : `crates/hearth-agent/tests/accounts_use_cases.rs::revoking_closes_sessions_and_keeps_the_password` ; `crates/hearth-agent/tests/account_cli.rs`.

## Cas limites
- Compte inconnu → « Ce compte n'existe pas ».
- Aucune session ouverte → succès, 0 session fermée.

## Règles liées
- BR-ACCT-008.

## Historique
- 2026-10-04 — création (HRT-03, session 2026-10-04-hearth-creation).
