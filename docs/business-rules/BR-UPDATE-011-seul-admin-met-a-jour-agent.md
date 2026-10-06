---
id: BR-UPDATE-011
domaine: UPDATE
titre: Seul un administrateur déclenche la mise à jour de l'agent
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-mises-a-jour.md (BR-UPDATE-011), HRT-17
maj: 2026-10-06
---

# BR-UPDATE-011 : Seul un administrateur déclenche la mise à jour de l'agent

## Règle
Seul un compte administrateur peut déclencher une mise à jour de l'agent à distance (`POST /agent/update`). Un compte en lecture seule voit l'état de la mise à jour (`GET /agent/update`, `GET /agent/update/last`, sujet `update` du flux) mais reçoit `403 FORBIDDEN_ROLE` (« Seul un administrateur peut mettre à jour l'agent ») avant toute lecture du corps ; le refus est consigné au journal d'activité (BR-UPDATE-024).

## Application (code)
- `crates/hearth-agent/src/entrypoint/http/mod.rs::ENDPOINTS` : `POST /agent/update` en `Access::Admin`, action de journal `AgentUpdate`.
- `crates/hearth-agent/src/entrypoint/http/auth.rs::guard` (rôle contrôlé avant le corps, refus consigné).

## Vérification
- `tests/update_http.rs` : `a_read_only_account_is_refused_and_the_refusal_is_journaled`, `without_a_session_the_route_answers_401`, `the_last_result_is_readable_by_any_account_and_empty_before_the_first_update`.
- `tests/http_api.rs` : balayage de toutes les routes `Admin`.
- `deploy/e2e/scenario-update.sh` (compte en lecture seule : 403).

## Interface (HRT-17, lot interface)
- Le bouton « Mettre à jour l'agent » est visible quand une version plus récente est disponible ; pour un compte Lecture seule il est DÉSACTIVÉ (`aria-disabled`, focus clavier gardé) avec l'infobulle « Seul un administrateur peut mettre à jour l'agent » ; il n'ouvre jamais la confirmation. Le rôle affiché est celui de la dernière connexion : l'AGENT reste l'arbitre, son refus `FORBIDDEN_ROLE` est l'échec typé `forbidden` (`agent_update/wire.rs::refusal_from_error`) rendu par la même phrase.
- Code : `apps/desktop/src/components/organisms/AgentUpdateCard.vue` (`isAdmin`, `hint`), `apps/desktop/src-tauri/src/agent_update/wire.rs`, `apps/desktop/src/agentUpdate/messages.ts`.
- Tests : `apps/desktop/src/components/organisms/agentUpdate.test.ts`, `apps/desktop/e2e/agent-update.spec.ts` (lecture seule : bouton désactivé, infobulle exacte) ; `apps/desktop/src-tauri/tests/agent_update_runtime.rs` : `a_read_only_account_is_refused_by_the_agent_with_the_typed_role_failure`.

## Cas limites
- La couche d'accès est la seule à contrôler le rôle (BR-ACCT-013). Le rôle est celui de la session au moment de la requête.

## Règles liées
- ADR-0008 (mises à jour signées), ADR-0012 (service système), ADR-0014 (dépendances de la mise à jour)

## Historique
- 2026-10-05 : création (HRT-17, lot agent, session 2026-10-04-hearth-creation).
- 2026-10-06 : section Interface (HRT-17, lot interface, session 2026-10-04-hearth-creation, T28).
