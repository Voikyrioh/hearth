---
id: BR-CONN-010
domaine: CONN
titre: Supprimer un serveur efface aussi ses identifiants mémorisés du coffre
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-connecter-serveur.md (BR-CONN-010)
maj: 2026-10-05
---

# BR-CONN-010 — Supprimer un serveur efface aussi ses identifiants mémorisés du coffre

## Règle
`remove_server` arrête la tâche du serveur puis efface le jeton et le mot de passe mémorisé du coffre, la dernière vue, les opérations en suspens et l'entrée du carnet. C'est la seule action qui efface le mot de passe mémorisé de façon voulue (la déconnexion le garde, BR-CONN-016). **La suppression gagne toujours** : elle attend les écritures en cours du serveur (verrou d'écriture par serveur) et marque le serveur supprimé ; une connexion qui était en vol revérifie le registre avant sa première écriture et ne réécrit ni coffre ni carnet. Si le coffre refuse d'effacer un secret, rien n'est dit « supprimé » : le serveur est remis en service et `LinkError::Vault` remonte.

## Application (code)
- `crates/hearth-link/src/manager/mod.rs::LinkManager::remove_server`, `::lock` (verrou d'écriture, serveur retiré refusé).

## Interface (coquille et vue)
- `pages/Servers.vue` : « Supprimer » ouvre la confirmation « Supprimer ce serveur ? » / « Ses identifiants mémorisés seront aussi supprimés. » ; coquille `LinkRuntime::remove_server`. Tests : `src/pages/Servers.test.ts::asks for a confirmation before removing…`, `apps/desktop/src-tauri/tests/link_runtime.rs`.

## Vérification
- Tests : `crates/hearth-link/tests/pinning.rs::servers_are_validated_listed_and_removed_with_their_secrets`, `tests/tracking.rs::a_server_removed_while_a_login_is_in_flight_is_never_written_back`, `::a_vault_that_refuses_to_erase_keeps_the_server_and_says_so`.

## Cas limites
- Un serveur inconnu rend `UnknownServer`, rien n'est effacé ailleurs.

## Règles liées
- BR-CONN-016.

## Historique
- 2026-10-05 — création (HRT-07, review Stephen round 1).
- 2026-10-05 : section Interface (HRT-10).
