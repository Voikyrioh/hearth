---
id: BR-CONN-017
domaine: CONN
titre: Mot de passe mémorisé devenu invalide : retour au formulaire, pas « Accès révoqué »
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-connecter-serveur.md (BR-CONN-017)
maj: 2026-10-05
---

# BR-CONN-017 — Mot de passe mémorisé devenu invalide : retour au formulaire, pas « Accès révoqué »

## Règle
Si la reconnexion silencieuse reçoit `INVALID_CREDENTIALS` (mot de passe changé côté serveur), l'état passe à `SessionExpired` avec la raison `StoredPasswordRefused` : une seule tentative, aucune boucle, jeton et mot de passe mémorisé effacés du coffre (la case « se souvenir » tombe), l'identifiant reste au carnet. L'interface rouvre le formulaire de connexion avec l'identifiant prérempli. C'est distinct d'un accès révoqué par un administrateur (`AccessRevoked`, raison `Revoked`).

## Application (code)
- `crates/hearth-link/src/domain/state.rs::LinkMachine` (`Input::StoredPasswordRefused`, `Reason::StoredPasswordRefused`).
- `crates/hearth-link/src/manager/attempt.rs::classify`, `reauthenticate` ; `manager/task.rs::Runner::stored_password_refused`.

## Interface (coquille et vue)
- `components/organisms/ReconnectPanel.vue` : raison `stored_password_refused` : formulaire de connexion, identifiant prérempli, aucun message bloquant. Test : `src/components/organisms/connect.test.ts::shows no blocking message when the remembered password was refused`.

## Vérification
- Tests : `domain::state::tests::every_stopped_state_carries_its_reason`, `manager::attempt::tests::each_refusal_has_its_own_outcome` ; `crates/hearth-link/tests/fault_proxy.rs::a_stored_password_that_is_refused_asks_for_the_login_form_not_access_revoked`.

## Cas limites
- Un `429` pendant la reconnexion silencieuse n'est pas un refus : la tentative attend `retry_after_s` (BR-RESIL-013).

## Règles liées
- BR-RESIL-013, BR-RESIL-014, BR-CONN-016.

## Historique
- 2026-10-05 — création (HRT-07, review Stephen round 1).
- 2026-10-05 : section Interface (HRT-10).
