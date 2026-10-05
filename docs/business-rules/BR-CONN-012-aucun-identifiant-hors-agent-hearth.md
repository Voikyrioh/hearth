---
id: BR-CONN-012
domaine: CONN
titre: Aucun identifiant n'est envoyé si le serveur n'est pas un agent Hearth
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-connecter-serveur.md (BR-CONN-012)
maj: 2026-10-05
---

# BR-CONN-012 — Aucun identifiant n'est envoyé si le serveur n'est pas un agent Hearth

## Règle
Un serveur n'est reconnu comme agent Hearth que si son `/hello` annonce `product = "hearth"`. Sinon la prise de contact échoue (« ce serveur n'est pas un agent Hearth ») et ni identifiant ni jeton ne partent. La règle est une fonction du domaine ; l'adaptateur de transport l'applique à la réponse de `/hello`.

## Application (code)
- `crates/hearth-link/src/domain/agent_identity.rs::check_product`.
- Appelée par `crates/hearth-link/src/adapters/http_transport.rs` (`Transport::hello`), que `LinkManager::probe` et `LinkManager::login` utilisent avant tout envoi d'identifiant.

## Interface (coquille et vue)
- `apps/desktop/src-tauri/src/link_dto.rs::LinkFailure::NotAgent` ; texte « Cette adresse ne répond pas comme un agent Hearth. Vérifie l'adresse. » sous le champ de l'adresse (`composables/useAddServer.ts`).

## Vérification
- Tests : `domain::agent_identity::tests::only_the_hearth_product_name_is_an_agent` ; `crates/hearth-link/tests/pinning.rs::the_probe_of_something_that_is_not_an_agent_fails_cleanly`.

## Cas limites
- Un serveur qui répond autre chose que du TLS, ou rien : erreur typée, jamais de blocage (délai).

## Règles liées
- BR-CONN-011.

## Historique
- 2026-10-05 — création (HRT-07, review Stephen round 1).
- 2026-10-05 : section Interface (HRT-10).
