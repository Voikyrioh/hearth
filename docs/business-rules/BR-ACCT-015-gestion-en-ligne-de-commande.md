---
id: BR-ACCT-015
domaine: ACCT
titre: Les opérations de gestion de comptes sont disponibles en ligne de commande sur le serveur
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-gerer-comptes.md (BR-ACCT-015), HRT-03
maj: 2026-10-04
---

# BR-ACCT-015 — Les opérations de gestion de comptes sont disponibles en ligne de commande sur le serveur

## Règle
`hearth-agent account add|list|passwd|role|remove|revoke` accède directement à la base, sans réseau, avec les mêmes validations et protections que l'interface (dernier administrateur, règles de mot de passe, fermeture des sessions). Le mot de passe est saisi sans écho avec confirmation, ou fourni par `HEARTH_ACCOUNT_PASSWORD` ; jamais en argument. Messages de la spécification, code de sortie non nul en cas d'erreur. Les migrations sont appliquées avant toute sous-commande.

## Application (code)
- `crates/hearth-agent/src/entrypoint/cli.rs::AccountAction` — sous-commandes.
- `crates/hearth-agent/src/entrypoint/account.rs::execute` — traduction vers `AccountService`, aucune règle ici.
- `crates/hearth-agent/src/entrypoint/terminal.rs::TerminalPasswords` — saisie sans écho (`rpassword`) ou variable d'environnement.
- `crates/hearth-agent/src/app.rs::run` — assemblage (`Command::Account`).

## Vérification
- Tests : `tests/account_cli.rs` (binaire lancé en processus : ajout puis liste, mot de passe faible, doublon, dernier administrateur, `passwd`, `revoke`, compte inconnu) ; `entrypoint::cli::tests` (le mot de passe n'est jamais accepté en argument).

## Cas limites
- La ligne de commande n'a pas de compte appelant : elle s'exécute avec les droits du système sur le serveur ; la suppression de son propre compte (BR-ACCT-012) ne la concerne pas.
- Sans terminal et sans `HEARTH_ACCOUNT_PASSWORD`, la saisie échoue avec un message qui indique la variable.
- Aucune opération de la ligne de commande n'est encore consignée au journal (BR-ACCT-016, HRT-05).

## Règles liées
- BR-ACCT-007, BR-ACCT-008, BR-ACCT-010, BR-ACCT-011, BR-ACCT-016.

## Historique
- 2026-10-04 — création (HRT-03, session 2026-10-04-hearth-creation).
