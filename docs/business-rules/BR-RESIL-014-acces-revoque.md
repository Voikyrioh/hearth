---
id: BR-RESIL-014
domaine: RESIL
titre: Une session fermée par l'administration répond SESSION_REVOKED, pas SESSION_EXPIRED
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-lien-resilient.md (BR-RESIL-014), technique-socle §7, HRT-04
maj: 2026-10-05
---

# BR-RESIL-014 — Accès révoqué

## Règle
Quand les sessions d'un compte sont fermées par un changement de mot de passe (BR-ACCT-008, 009), une suppression de compte (BR-ACCT-010) ou une révocation (BR-ACCT-011), l'empreinte de chaque jeton fermé est retenue (table `revoked_sessions`, 90 jours : `domain::sessions::REVOCATION_RETENTION`) : le client qui revient avec ce jeton reçoit `401 SESSION_REVOKED` (état « Accès révoqué »), pas `SESSION_EXPIRED` (qui lui ferait rejouer une reconnexion silencieuse). La déconnexion explicite (`DELETE /sessions/current`) ne laisse pas de trace de révocation.

## Application (code)
- `crates/hearth-agent/src/domain/sessions.rs::check` (`SessionEnd::Revoked`).
- `crates/hearth-agent/src/infrastructure/sqlite/session_repo.rs` — `impl SessionTx for SqliteUnitOfWork::close` enregistre les empreintes révoquées dans la même transaction que la suppression ; `SqliteSessionRepo::is_revoked` les relit.
- Côté client (HRT-07) : `crates/hearth-link/src/domain/state.rs::LinkMachine::stop_trying` (`Input::AccessRevoked`) — `SESSION_REVOKED` (ou `INVALID_CREDENTIALS` à la reconnexion silencieuse) fait passer l'état à `AccessRevoked` : plus aucune tentative, jeton et mot de passe effacés du coffre (pas de reconnexion avec les anciens identifiants), événement `SessionEnded`. L'état porte la raison `Revoked` (l'agent répond `SESSION_REVOKED` pour un changement de mot de passe, une suppression de compte ou une révocation : la bibliothèque ne les distingue pas). Un mot de passe mémorisé refusé n'est pas un accès révoqué (BR-CONN-017).

## Vérification
- Tests : `domain::sessions::tests::a_revoked_token_ends_as_revoked` ; `tests/sessions_https.rs::a_session_is_revoked_when_its_password_changes`.
- Tests côté client : `domain::state::tests::row12_connected_account_revoked_shows_access_revoked_and_stops_trying`, `::row13_access_revoked_then_another_account_is_connected` ; `crates/hearth-link/tests/fault_proxy.rs::a_session_revoked_during_the_stream_shows_access_revoked_and_stops`.

## Cas limites
- Passé 90 jours, la trace est purgée : le jeton reçoit `SESSION_EXPIRED`, la reconnexion silencieuse échoue alors avec `INVALID_CREDENTIALS` côté client (« Accès révoqué » quand même).

## Affichage dans le client
- État « Accès révoqué » : pastille `apps/desktop/src/components/molecules/LinkStatePill.vue`, actions désactivées par la prop `needsLink` (BR-RESIL-001, 008).

## Règles liées
- BR-RESIL-013.
- BR-RESIL-012.

## Historique
- 2026-10-04 — création (HRT-04, session 2026-10-04-hearth-creation).
- 2026-10-05 — côté client ajouté (HRT-07, session 2026-10-04-hearth-creation).
- 2026-10-05 — précisé (HRT-07, review Stephen round 1).
