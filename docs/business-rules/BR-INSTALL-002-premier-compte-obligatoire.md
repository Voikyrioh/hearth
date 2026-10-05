---
id: BR-INSTALL-002
domaine: INSTALL
titre: Une première installation crée obligatoirement un premier compte administrateur
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-installer-agent.md (BR-INSTALL-002), HRT-15
maj: 2026-10-05
---

# BR-INSTALL-002 : Une première installation crée obligatoirement un premier compte administrateur

## Règle
Quand aucun administrateur n'existe (première installation, ou base sans administrateur après une installation interrompue), l'installation crée le premier compte : identifiant et mot de passe saisis (sans écho, avec confirmation) ou fournis par `HEARTH_ADMIN_USER` et `HEARTH_ADMIN_PASSWORD` (ou `HEARTH_ADMIN_PASSWORD_HASH`, haché Argon2id au format PHC). Sans terminal et sans variables, l'installation s'arrête **avant toute écriture**. Une réinstallation qui retrouve un administrateur ne demande rien.

## Application (code)
- `crates/hearth-agent/src/domain/install/plan.rs::plan_install` : `needs_first_admin` = aucun administrateur observé.
- `crates/hearth-agent/src/domain/install/credentials.rs` : identifiant, mot de passe, forme du haché.
- `crates/hearth-agent/src/application/install.rs::Installer::run` (étape 5) : crée le compte par le port `AdminAccounts`.
- `crates/hearth-agent/src/entrypoint/install.rs::first_admin` : variables ou questions, validées avant la première écriture.
- `crates/hearth-agent/src/application/accounts.rs::AccountService::create_with_hash` : compte créé depuis un haché, sans mot de passe en clair.

## Vérification
- `domain::install::plan::tests` (première installation, base sans administrateur).
- `tests/install_flow.rs` : `a_non_interactive_first_installation_creates_everything_and_says_so`, `a_password_hash_creates_the_account_without_any_password`, `an_interactive_installation_asks_the_name_the_password_twice_and_the_port`, `without_a_terminal_and_without_variables_nothing_is_written`.

## Cas limites
- Un haché fourni n'a pas de règle de complexité (le mot de passe n'est pas connu) : seul le format Argon2id est contrôlé.
- Le mot de passe n'est jamais un argument de la ligne de commande (`--password` n'existe pas).

## Règles liées
- BR-INSTALL-009, BR-INSTALL-010, BR-ACCT-001

## Historique
- 2026-10-05 : création (HRT-15, session 2026-10-04-hearth-creation).
