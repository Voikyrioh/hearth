---
id: BR-ACCT-003
domaine: ACCT
titre: L'identifiant est unique et insensible à la casse
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-gerer-comptes.md (BR-ACCT-003), HRT-03
maj: 2026-10-04
---

# BR-ACCT-003 — L'identifiant est unique et insensible à la casse

## Règle
Deux comptes « marie » et « MARIE » ne peuvent pas coexister. L'identifiant est normalisé en minuscules à la saisie ; la colonne `accounts.username` est unique (collation `NOCASE`). Message : « Cet identifiant est déjà utilisé ».

## Application (code)
- `crates/hearth-agent/src/domain/accounts/username.rs::Username::parse` — normalise en minuscules ASCII (une saisie « MARIE » devient « marie »).
- `crates/hearth-agent/migrations/0001_accounts_sessions_meta.sql` — `username TEXT NOT NULL UNIQUE COLLATE NOCASE` : filet de sécurité contre une insertion concurrente.
- `crates/hearth-agent/src/application/accounts.rs::AccountService::create` — refuse avec `AccountError::UsernameTaken` (vérification dans la transaction d'écriture, violation d'unicité rendue par le dépôt en `StoreError::Duplicate`).

## Vérification
- Tests : `domain::accounts::username::tests::uppercase_is_normalized_so_uniqueness_ignores_case` ; `crates/hearth-agent/tests/accounts_use_cases.rs::usernames_are_unique_whatever_the_case` ; `crates/hearth-agent/tests/accounts_repo.rs::the_database_refuses_two_usernames_differing_by_case`.

## Cas limites
- Deux créations simultanées du même identifiant : l'écriture est sérialisée (`BEGIN IMMEDIATE`), la seconde reçoit « Cet identifiant est déjà utilisé ».

## Règles liées
- BR-ACCT-002.

## Historique
- 2026-10-04 — création (HRT-03, session 2026-10-04-hearth-creation).
