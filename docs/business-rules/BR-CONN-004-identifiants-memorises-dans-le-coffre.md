---
id: BR-CONN-004
domaine: CONN
titre: Les identifiants ne sont mémorisés que dans le coffre de Windows, si « Se souvenir de moi » est cochée
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-connecter-serveur.md (BR-CONN-004), technique-socle §6
maj: 2026-10-05
---

# BR-CONN-004 — Identifiants mémorisés dans le coffre de Windows

## Règle
Le mot de passe n'est mémorisé que si l'utilisateur coche « Se souvenir de moi sur ce PC » (cochée par défaut au 3e temps de l'ajout) ; il va alors dans le Gestionnaire d'identification de Windows, sous la clé `Hearth/{id du serveur}` (le jeton de session sous `Hearth/{id}/token`). Rien n'est jamais écrit en clair dans un fichier : `servers.json` ne garde que l'identifiant du compte et l'indicateur `remember`. Décochée, le mot de passe déjà mémorisé est effacé. « Oublier mes identifiants » efface le mot de passe mémorisé sans fermer la session en cours.

## Bibliothèque
- `crates/hearth-link/src/manager/mod.rs::LinkManager::login` (écrit ou efface le mot de passe selon `remember`) et `::forget_credentials` (oubli).
- Port `Vault` : `crates/hearth-link/src/ports/vault.rs` ; `domain/secret.rs::Secret` (effacé de la mémoire, `Debug` masqué).

## Interface (coquille et vue)
- `apps/desktop/src-tauri/src/vault.rs::CredentialVault`, `WindowsCredentials`, `credential_target` : adaptateur du coffre Windows.
- `apps/desktop/src/components/organisms/LoginForm.vue` : case « Se souvenir de moi sur ce PC », cochée par défaut ; `pages/Servers.vue` : « Oublier mes identifiants ».

## Vérification
- Tests : `crates/hearth-link/tests/pinning.rs::forgetting_the_credentials_erases_the_password_but_keeps_the_session`, `::servers_are_validated_listed_and_removed_with_their_secrets` ; `apps/desktop/src-tauri/tests/vault.rs` (clés, aller-retour, effacement ; `the_windows_credential_manager_stores_reads_and_erases_a_secret` contre le vrai Gestionnaire, `--ignored`, lancé par le job Windows de la CI) ; `tests/link_runtime.rs::the_wizard_registers_only_on_a_successful_login_then_remembers_forgets_and_removes` (coffre sous `Hearth/{id}`, rien après un refus) ; `src/pages/Servers.test.ts`, `e2e/connect.spec.ts` (oubli des identifiants).
- À la main : Gestionnaire d'identification de Windows, identifiants génériques `Hearth/…`.

## Cas limites
- Écriture de la première connexion : carnet d'abord, secrets ensuite ; une application tuée entre les deux laisse un serveur sans session (visible, supprimable), jamais un secret orphelin ; un échec défait tout et referme la session obtenue (`tracking.rs::a_vault_that_refuses_to_write_leaves_no_server_and_closes_the_new_session`).
- Un refus de connexion n'écrit rien au coffre.
- Coffre inaccessible : l'erreur ne contient aucun secret ; la connexion échoue (`vault`), l'utilisateur est invité à réessayer.

## Règles liées
- BR-CONN-005, BR-CONN-010, BR-CONN-016, BR-CONN-017.

## Historique
- 2026-10-05 — création (HRT-10).
