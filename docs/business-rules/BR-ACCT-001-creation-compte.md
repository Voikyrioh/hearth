---
id: BR-ACCT-001
domaine: ACCT
titre: Un compte est créé avec un identifiant unique, un mot de passe et un rôle
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-gerer-comptes.md (BR-ACCT-001), HRT-03
maj: 2026-10-04
---

# BR-ACCT-001 — Un compte est créé avec un identifiant unique, un mot de passe et un rôle

## Règle
Un compte est créé avec un identifiant, un mot de passe et un rôle (Administrateur ou Lecture seule). L'identifiant respecte BR-ACCT-002 et BR-ACCT-003, le mot de passe BR-ACCT-004 et BR-ACCT-005. L'agent conserve le haché Argon2id (m = 19 Mio, t = 2, p = 1), jamais le mot de passe (BR-ACCT-006), un identifiant technique ULID, la date de création et la date du dernier changement de mot de passe. Message : « Compte <identifiant> créé ».

## Application (code)
- `crates/hearth-agent/src/application/accounts.rs::AccountService::create` — enchaîne les règles du domaine (`Username::parse`, `PlainPassword::new`), hache, puis insère dans une transaction.
- `crates/hearth-agent/src/domain/accounts/account.rs::Account` — le compte.
- `crates/hearth-agent/src/domain/accounts/role.rs::Role` — les deux rôles.
- `crates/hearth-agent/migrations/0001_accounts_sessions_meta.sql` — table `accounts`.

## Vérification
- Tests : `tests/accounts_use_cases.rs::create_stores_a_hash_and_never_the_password`, `create_refuses_an_invalid_username`, `create_lists_every_unmet_password_rule` ; `tests/account_cli.rs::add_then_list`.

## Cas limites
- Échec d'une règle : aucun compte créé.
- `last_login_at` est vide jusqu'à la première connexion (HRT-04).

## Règles liées
- BR-ACCT-002 à BR-ACCT-006.

## Historique
- 2026-10-04 — création (HRT-03, session 2026-10-04-hearth-creation).
