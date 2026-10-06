---
id: BR-UPDATE-022
domaine: UPDATE
titre: Le client affiche la version de l'agent installée et la version disponible
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-mises-a-jour.md (BR-UPDATE-022), HRT-17
maj: 2026-10-06
---

# BR-UPDATE-022 : Le client affiche la version de l'agent installée et la version disponible

## Règle
La carte « État du serveur : {nom} » des réglages affiche « Client {version} · Agent {version} » et, quand le flux de versions en propose une, la version disponible avec la mention « Mise à jour disponible ». Une version est « disponible » seulement si elle est STRICTEMENT plus récente que celle lue chez l'agent (`GET /agent/update`, relue juste avant d'envoyer) : jamais de rétrogradation proposée, ni la même version. Rien n'est proposé pour une installation gérée (BR-INSTALL-007) ni sans cible. La décision est prise par la coquille (`AgentUpdateView.available`), l'interface ne compare aucun numéro de version.

## Application (code)
- `apps/desktop/src-tauri/src/agent_update/domain.rs::is_newer`, `service.rs::view`, `dto.rs::AgentUpdateView`.
- `apps/desktop/src/components/organisms/AgentUpdateCard.vue`.

## Vérification
- `apps/desktop/src-tauri/tests/agent_update_domain.rs::only_a_strictly_newer_version_is_available_never_a_downgrade_or_the_same` ; `agent_update_runtime.rs::the_state_shows_the_version_of_the_agent_and_a_newer_version_from_the_feed` ; `apps/desktop/src/components/organisms/agentUpdate.test.ts`.

## Cas limites
- Version de l'agent illisible ou en préversion : rien n'est proposé. Lien coupé : la dernière lecture reste affichée, le bouton est inerte avec la raison.

## Règles liées
- BR-UPDATE-025, BR-UPDATE-023, ADR-0021

## Historique
- 2026-10-06 : création (HRT-17, lot interface, session 2026-10-04-hearth-creation, T28).
