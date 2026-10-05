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
- `crates/hearth-link/src/adapters/http_transport.rs::require_pinned` : `Pin::Probe` est refusé **par l'adaptateur** pour tout sauf `hello` (connexion, déconnexion, requêtes, relecture d'opération, flux) : la protection ne dépend pas de la discipline du gestionnaire.

## Interface (coquille et vue)
- L'assistant ne demande identifiant et mot de passe qu'après « Confirmer » l'empreinte (3e temps) ; `probe_server` n'envoie aucun identifiant. Test : `src/composables/useAddServer.test.ts::probes, shows the fingerprint in 8 groups, registers on « Confirmer », then connects`.

## Vérification
- Tests : `domain::pinning::tests::a_different_fingerprint_blocks_and_sends_nothing`, `::no_stored_fingerprint_is_a_first_connection` ; `tests/pinning.rs` ; `adapters::http_transport::tests::the_probe_mode_is_refused_for_everything_but_hello`.

## Cas limites
- Après une empreinte changée, aucune requête authentifiée ne part (BR-CONN-003).

## Règles liées
- BR-CONN-001, BR-CONN-002.

## Historique
- 2026-10-05 — création (HRT-07, session 2026-10-04-hearth-creation).
- 2026-10-05 — précisé (HRT-07, review Stephen round 1).
- 2026-10-05 : section Interface (HRT-10).
