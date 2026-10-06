---
id: BR-ACCT-006
domaine: ACCT
titre: Aucun mot de passe existant ne peut être affiché ni récupéré
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-gerer-comptes.md (BR-ACCT-006), HRT-03
maj: 2026-10-06
---

# BR-ACCT-006 — Aucun mot de passe existant ne peut être affiché ni récupéré

## Règle
Un mot de passe n'est jamais rendu en clair après sa création : l'agent ne conserve que son hachage Argon2id, et ni le mot de passe ni le hachage ne sont affichés, journalisés ni placés dans un message d'erreur. Aucune route ni sous-commande ne renvoie le hachage.

## Application (code)
- `crates/hearth-agent/src/domain/secret.rs::Secret` — `Debug` masqué (`Secret(***)`), pas de `Display`, pas de `Clone`, mémoire effacée à la libération (`zeroize`) ; `expose()` explicite.
- `crates/hearth-agent/src/domain/accounts/account.rs::Account` — le hachage est un `Secret`.
- `crates/hearth-agent/src/application/ports/password_hasher.rs::PasswordHasher` — ne manipule que des `Secret`.
- `crates/hearth-agent/src/application/accounts.rs::AccountView` — ce que rendent les cas d'usage : le haché n'en fait pas partie.

## Interface (HRT-13)
Aucun mot de passe dans un état Pinia, un journal du pont, une notification, un événement Tauri, un `Debug` ou un message d'erreur ; champs vidés après chaque envoi (réussi ou non) et à la fermeture de la fenêtre (`CreateAccountDialog.vue`, `PasswordDialog.vue`). Tests : `accounts.test.ts::creates an account: success announced, list re-read, no password kept anywhere`, `accounts_wire.rs::no_password_shows_in_the_debug_of_a_planned_action_nor_of_a_refusal`, `accounts_runtime.rs` (réponse de création sans mot de passe).

## Vérification
- Tests : `domain::secret::tests` ; `domain::accounts::password::tests::debug_of_a_plain_password_does_not_reveal_it` ; `crates/hearth-agent/tests/accounts_use_cases.rs::errors_never_contain_the_password`.

## Cas limites
- Le mot de passe fourni par `HEARTH_ACCOUNT_PASSWORD` reste dans l'environnement du processus appelant : à utiliser pour l'automatisation seulement.
- Un mot de passe n'est jamais passé en argument de ligne de commande.

## Règles liées
- BR-ACCT-004.

## Historique
- 2026-10-04 — création (HRT-03, session 2026-10-04-hearth-creation).
- 2026-10-06 — section « Interface » (HRT-13, session 2026-10-04-hearth-creation).
