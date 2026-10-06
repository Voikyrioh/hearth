---
id: BR-ACCT-002
domaine: ACCT
titre: L'identifiant fait 3 à 32 caractères parmi minuscules, chiffres, tiret, underscore
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-gerer-comptes.md (BR-ACCT-002), HRT-03
maj: 2026-10-06
---

# BR-ACCT-002 — L'identifiant fait 3 à 32 caractères parmi minuscules, chiffres, tiret, underscore

## Règle
L'identifiant contient 3 à 32 caractères : lettres minuscules, chiffres, tiret (`-`), underscore (`_`). Aucun espace ni caractère spécial. La saisie est ramenée en minuscules avant le contrôle (voir BR-ACCT-003). Messages : « L'identifiant est requis », « L'identifiant doit contenir au moins 3 caractères », « L'identifiant doit contenir au plus 32 caractères », « L'identifiant contient des caractères non autorisés ».

## Application (code)
- `crates/hearth-proto/src/account_rules.rs::check_username` — la règle de format, SOURCE UNIQUE (HRT-13) : l'agent, sa ligne de commande et la commande `check_account_input` du client l'appellent ; le domaine de l'agent n'en garde qu'un appel.
- `crates/hearth-agent/src/domain/accounts/username.rs::Username::parse` — contrôle du format et normalisation ; erreurs typées `UsernameError` dont le message est celui de la spécification.

## Interface (HRT-13)
Validation en direct sous le champ « Identifiant » : la commande pure `check_account_input` (`apps/desktop/src-tauri/src/accounts/service.rs::check_input`) appelle `hearth_proto::account_rules::check_username`, la MÊME fonction que l'agent (l'interface n'a aucune copie ; le pont simulé du navigateur en a une réplique verrouillée par `crates/hearth-proto/tests/vectors/account-input.json`, joué par Rust et par `simulated-accounts.test.ts`). Messages : `fr.ts` groupe `accounts`.

## Vérification
- Tests : `domain::accounts::username::tests` (bornes 2/3/32/33, caractères refusés, vide, messages).

## Cas limites
- Vide → « requis », pas « trop court ».
- Caractères invalides et longueur insuffisante en même temps (« é ») → caractères non autorisés.
- Longueur comptée en caractères, pas en octets.

## Règles liées
- BR-ACCT-003 (unicité insensible à la casse).

## Historique
- 2026-10-04 — création (HRT-03, session 2026-10-04-hearth-creation).
- 2026-10-06 — section « Interface » (HRT-13, session 2026-10-04-hearth-creation).
