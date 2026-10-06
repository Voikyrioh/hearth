---
id: BR-UPDATE-025
domaine: UPDATE
titre: Les réglages affichent la version du client et celle de l'agent de chaque serveur
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-mises-a-jour.md (BR-UPDATE-025), HRT-16
maj: 2026-10-06
---

# BR-UPDATE-025 : Les réglages affichent la version du client et celle de l'agent de chaque serveur

## Règle
La version du client qui tourne s'affiche dans les réglages (ligne « Version »), avec l'état des mises à jour (BR-UPDATE-007, 026). La version de l'AGENT de chaque serveur s'affiche dans la carte « État du serveur : {nom} » des mêmes réglages (« Client {version} · Agent {version} », HRT-17, lot interface), avec la mention « Mise à jour disponible » (BR-UPDATE-022, 023).

## Application (code)
- `apps/desktop/src/pages/Settings.vue` (ligne « Version », commande `get_app_version`) ; `apps/desktop/src/components/organisms/UpdatePanel.vue`.
- `apps/desktop/src/components/organisms/AgentUpdateCard.vue` (version de l'agent, `get_agent_update`).

## Vérification
- `apps/desktop/src/pages/Settings.test.ts` (version affichée) ; `apps/desktop/src/components/organisms/updates.test.ts`.
- `apps/desktop/src/components/organisms/agentUpdate.test.ts` (versions du client et de l'agent) ; `apps/desktop/src/stores/agentUpdates.test.ts`.

## Cas limites
- Version de l'agent indisponible (serveur non connecté, jamais lu) : « indisponible », sans message d'erreur.

## Règles liées
- BR-UPDATE-022, BR-UPDATE-026

## Historique
- 2026-10-05 : création (HRT-16, session 2026-10-04-hearth-creation).
- 2026-10-06 : version de l'agent affichée (HRT-17, lot interface, T28).
