---
id: BR-RESIL-013
domaine: RESIL
titre: Session expirée et mot de passe mémorisé : reconnexion sans ressaisie
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-lien-resilient.md (BR-RESIL-013), ADR-0007
maj: 2026-10-05
---

# BR-RESIL-013 — Session expirée et mot de passe mémorisé : reconnexion sans ressaisie

## Règle
Quand l'agent répond `SESSION_EXPIRED` (au retour du lien ou par un message `session` du flux) et que le mot de passe est au coffre, la bibliothèque rouvre une session en silence (l'état affiché ne change pas) puis rouvre le flux. Si le mot de passe est refusé (`INVALID_CREDENTIALS`), l'accès est révoqué. Sans mot de passe mémorisé, l'état passe à `SessionExpired` ; le panneau de saisie relève de l'interface (à venir, HRT-12) et la reconnexion se fait par `login`.

## Application (code)
- `crates/hearth-link/src/domain/state.rs::LinkMachine::on_session_expired` (`Input::SessionExpired { can_reauth }`, effet `Effect::Reauthenticate`), `::on_reauthenticated`.
- `crates/hearth-link/src/manager/task.rs` : exécution de `Effect::Reauthenticate` (coffre, `Transport::login`).

## Vérification
- Tests : `domain::state::tests::an_expired_session_with_a_saved_password_reconnects_silently`, `::a_silent_reconnection_that_hits_refused_credentials_means_access_revoked`, `::a_silent_reconnection_that_cannot_reach_the_server_keeps_the_reauth_step`.
- Intégration : `tests/fault_proxy.rs::an_expired_session_with_a_saved_password_reconnects_silently`.

## Cas limites
- `429 TOO_MANY_ATTEMPTS` ou `503 BUSY` avec `retry_after_s` : la prochaine tentative n'a pas lieu avant ce délai (plafonné à 1 h), ni avant le délai habituel s'il est plus long ; on ne renvoie pas un login toutes les 30 s pendant le verrouillage (`Input::RetryAfter`, test : `domain::state::tests::a_server_that_says_to_wait_delays_the_next_attempt`).
- Mot de passe mémorisé refusé : voir BR-CONN-017 (retour au formulaire, pas « Accès révoqué »).
- La reconnexion silencieuse est une tentative comme une autre : si le serveur est injoignable, elle est retentée avec les mêmes délais.
- Si la session toute neuve est refusée aussitôt (`SESSION_EXPIRED` juste après la reconnexion), la reconnexion silencieuse n'est pas relancée en boucle serrée : elle suit les délais de reconnexion (test : `domain::state::tests::a_new_session_refused_at_once_is_retried_with_the_delays_not_in_a_tight_loop`).

## Règles liées
- BR-RESIL-012, BR-RESIL-014.

## Historique
- 2026-10-05 — création (HRT-07, session 2026-10-04-hearth-creation).
- 2026-10-05 — précisé (HRT-07, review Stephen round 1).
