---
id: BR-ACCT-005
domaine: ACCT
titre: Le mot de passe ne contient pas l'identifiant du compte
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-gerer-comptes.md (BR-ACCT-005), HRT-03
maj: 2026-10-06
---

# BR-ACCT-005 — Le mot de passe ne contient pas l'identifiant du compte

## Règle
Le mot de passe ne doit pas contenir l'identifiant du compte, quelle que soit la casse. Message : « Le mot de passe ne doit pas contenir l'identifiant ».

## Application (code)
- `crates/hearth-proto/src/account_rules.rs::unmet_password_rules` — la règle « sans l'identifiant », SOURCE UNIQUE (HRT-13) : l'agent, sa ligne de commande et la commande `check_account_input` du client l'appellent ; le domaine de l'agent n'en garde qu'un appel.
- `crates/hearth-agent/src/domain/accounts/password.rs::unmet_rules` — règle `PasswordRule::ContainsUsername` (comparaison en minuscules).

## Interface (HRT-13)
Le critère « ne doit pas contenir l'identifiant » est évalué avec l'identifiant du compte concerné (celui qu'on tape à la création, celui de la ligne pour le mot de passe d'un autre, celui du carnet pour le sien) ; il ne joue que si l'identifiant est valide (`hearth_proto::account_rules::check_input`). Test : `PasswordDialog` dans `components/organisms/accounts.test.ts`.

## Vérification
- Tests : `domain::accounts::password::tests::password_must_not_contain_the_username_whatever_the_case`, `a_password_equal_to_the_username_is_refused`, `a_similar_but_different_word_is_accepted`.

## Cas limites
- Identifiant au milieu, au début ou à la fin → refusé.
- Mot proche mais différent (« Mari-e ») → accepté.

## Règles liées
- BR-ACCT-004.

## Historique
- 2026-10-04 — création (HRT-03, session 2026-10-04-hearth-creation).
- 2026-10-06 — section « Interface » (HRT-13, session 2026-10-04-hearth-creation).
