---
id: BR-CONN-011
domaine: CONN
titre: Aucun identifiant n'est envoyé avant la confirmation de l'empreinte
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-connecter-serveur.md (BR-CONN-011), ADR-0005
maj: 2026-10-05
---

# BR-CONN-011 — Aucun identifiant n'est envoyé avant la confirmation de l'empreinte

## Règle
`login` refuse de partir tant que le serveur n'est pas dans le carnet avec une empreinte confirmée, et la connexion TLS de `login` est épinglée sur cette empreinte. La première prise de contact (`probe`) ne fait qu'un `GET /hello` sans authentification, sans jeton, sans identifiant.

## Application (code)
- `crates/hearth-link/src/domain/pinning.rs::PinDecision::allows_credentials`.
- `crates/hearth-link/src/adapters/http_transport.rs` : `Pin::Probe` réservé à `hello`, `Pin::Pinned` pour tout le reste.

## Vérification
- Tests : `domain::pinning::tests::a_different_fingerprint_blocks_and_sends_nothing`, `::no_stored_fingerprint_is_a_first_connection` ; `tests/pinning.rs`.

## Cas limites
- Après une empreinte changée, aucune requête authentifiée ne part (BR-CONN-003).

## Règles liées
- BR-CONN-001, BR-CONN-002.

## Historique
- 2026-10-05 — création (HRT-07, session 2026-10-04-hearth-creation).
