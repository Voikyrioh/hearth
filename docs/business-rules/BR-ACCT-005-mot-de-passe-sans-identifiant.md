---
id: BR-ACCT-005
domaine: ACCT
titre: Le mot de passe ne contient pas l'identifiant du compte
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-gerer-comptes.md (BR-ACCT-005), HRT-03
maj: 2026-10-04
---

# BR-ACCT-005 — Le mot de passe ne contient pas l'identifiant du compte

## Règle
Le mot de passe ne doit pas contenir l'identifiant du compte, quelle que soit la casse. Message : « Le mot de passe ne doit pas contenir l'identifiant ».

## Application (code)
- `crates/hearth-agent/src/domain/accounts/password.rs::unmet_rules` — règle `PasswordRule::ContainsUsername` (comparaison en minuscules).

## Vérification
- Tests : `domain::accounts::password::tests::password_must_not_contain_the_username_whatever_the_case`, `a_password_equal_to_the_username_is_refused`, `a_similar_but_different_word_is_accepted`.

## Cas limites
- Identifiant au milieu, au début ou à la fin → refusé.
- Mot proche mais différent (« Mari-e ») → accepté.

## Règles liées
- BR-ACCT-004.

## Historique
- 2026-10-04 — création (HRT-03, session 2026-10-04-hearth-creation).
