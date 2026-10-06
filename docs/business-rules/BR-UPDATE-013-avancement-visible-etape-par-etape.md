---
id: BR-UPDATE-013
domaine: UPDATE
titre: Une mise à jour de l'agent est visible étape par étape
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-mises-a-jour.md (BR-UPDATE-013), HRT-17
maj: 2026-10-06
---

# BR-UPDATE-013 : Une mise à jour de l'agent est visible étape par étape

## Règle
L'agent diffuse chaque étape sur le flux temps réel (sujet `update`, tout compte authentifié) : `download` (avec un pourcentage entier qui ne recule jamais, un message par pourcentage), `verify` (somme SHA-256 puis signature minisign), `install` (dépôt du binaire, contrôle de la version annoncée), `restart`, `check`, puis `done` avec le résultat. **Rien n'est écrit sur le disque avant la vérification de la somme ET de la signature** : le fichier est téléchargé en mémoire (128 Mio au plus, HTTPS seulement). Un client qui s'abonne en cours de route reçoit d'abord l'état courant. Les textes affichés viennent de l'interface, indexés par les codes.

## Application (code)
- `crates/hearth-agent/src/domain/update/progress.rs::PercentTracker`.
- `crates/hearth-agent/src/application/update.rs::UpdateService::{execute, step, subscribe}`.
- `crates/hearth-agent/src/entrypoint/ws/connection.rs` (sujet `update`) ; types `hearth-proto::api::update::UpdateStep`, `hearth-proto::stream::UpdateMessage`.
- `crates/hearth-agent/src/infrastructure/update/{download.rs, minisign.rs}`.

## Vérification
- `domain::update::progress::tests`.
- `tests/update_use_cases.rs` : `a_valid_update_shows_every_step_in_order_then_hands_over_to_the_supervisor`, `nothing_is_written_or_run_before_the_checksum_and_the_signature_are_verified`, `a_client_subscribing_in_the_middle_gets_the_current_step_first`.
- `tests/update_stream.rs` (vrai WebSocket).
- `tests/update_download.rs` (HTTPS réel local), `infrastructure::update::minisign::tests`.
- `deploy/e2e/scenario-update.sh` (signature d'une autre clé : rien téléchargé, rien écrit ; somme fausse : rien déposé).

## Interface (HRT-17, lot interface)
- Les étapes s'affichent une à la fois (Téléchargement avec son pourcentage, Vérification, Installation, Redémarrage, Contrôle : faite = coche, en cours = point braise, à venir = cercle vide) avec la phrase « Mise à jour de l'agent en cours. Étape : téléchargement (35 %)… ». Elles viennent du sujet `update` du flux (`ServerMessage::Update`, `Event::AgentUpdate`, événement `agent-update://progress`) ; l'interface ne calcule rien (le pourcentage est borné à 0 à 100 pour la barre).
- Code : `apps/desktop/src/components/molecules/AgentUpdateSteps.vue`, `apps/desktop/src/agentUpdate/messages.ts`, `apps/desktop/src/stores/agentUpdates.ts`, `crates/hearth-link/src/manager/task.rs::on_update`.
- Tests : `crates/hearth-link/tests/agent_update.rs::the_steps_arrive_on_the_stream_in_order_with_the_download_percentage` ; `apps/desktop/src-tauri/tests/agent_update_runtime.rs` (progression relayée à l'interface) ; `apps/desktop/src/components/organisms/agentUpdate.test.ts`, `apps/desktop/e2e/agent-update.spec.ts`.

## Cas limites
- Signature d'une autre clé : refusée avant tout téléchargement (`422 BAD_SIGNATURE`).
- Somme ou signature fausses du fichier : échec `bad_checksum` / `bad_signature`, rien écrit.
- Binaire qui n'annonce pas la version visée, ou qui ne s'exécute pas : `bad_binary`, dépôt retiré.

## Règles liées
- ADR-0008 (mises à jour signées), ADR-0012 (service système), ADR-0014 (dépendances de la mise à jour)

## Historique
- 2026-10-05 : création (HRT-17, lot agent, session 2026-10-04-hearth-creation).
- 2026-10-06 : section Interface (HRT-17, lot interface, session 2026-10-04-hearth-creation, T28).
