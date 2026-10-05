---
id: BR-CONN-016
domaine: CONN
titre: La déconnexion ferme la session et efface le jeton, pas le mot de passe mémorisé
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-connecter-serveur.md (BR-CONN-016)
maj: 2026-10-05
---

# BR-CONN-016 — La déconnexion ferme la session et efface le jeton, pas le mot de passe mémorisé

## Règle
`logout` ferme la session côté serveur (au mieux : un serveur injoignable n'empêche pas de se déconnecter) et efface le jeton du coffre. Le mot de passe mémorisé et la case « se souvenir » sont **conservés** ; seul « oublier ce serveur » (`remove_server`) efface tout. La déconnexion est mémorisée dans le carnet : aucune reconnexion automatique ensuite, pas même au démarrage suivant. L'état affiché est `SessionExpired` avec la raison `UserDisconnected` (distincte d'une session expirée) ; se connecter de nouveau (`login`) lève la déconnexion.

## Application (code)
- `crates/hearth-link/src/domain/state.rs::LinkMachine` (`Input::LoggedOut`, `Reason::UserDisconnected`, `Start::Disconnected`).
- `crates/hearth-link/src/manager/mod.rs::LinkManager::logout`, `initial_start` ; `domain/server.rs::ServerRecord::signed_out`.

## Vérification
- Tests : `domain::state::tests::logging_out_stops_everything_and_shows_session_expired`, `::every_stopped_state_carries_its_reason` ; `crates/hearth-link/tests/pinning.rs::logout_closes_the_session_but_keeps_the_remembered_password`.

## Cas limites
- Le jeton est effacé même si le serveur est injoignable.

## Règles liées
- BR-CONN-010, BR-CONN-017.

## Historique
- 2026-10-05 — création (HRT-07, review Stephen round 1).
