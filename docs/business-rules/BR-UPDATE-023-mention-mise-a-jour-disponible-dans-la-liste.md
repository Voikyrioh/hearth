---
id: BR-UPDATE-023
domaine: UPDATE
titre: Seul le serveur qui a une mise à jour disponible porte la mention « Mise à jour disponible » dans la liste
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-mises-a-jour.md (BR-UPDATE-023), HRT-17
maj: 2026-10-06
---

# BR-UPDATE-023 : Seul le serveur qui a une mise à jour disponible porte la mention « Mise à jour disponible » dans la liste

## Règle
Dans la liste des serveurs, la mention « Mise à jour disponible » n'apparaît que sur la ligne du serveur dont l'agent a une version plus récente disponible (BR-UPDATE-022) et qui n'est pas en installation gérée. L'état se lit à chaque connexion du serveur ; un serveur jamais joint pendant la session n'affiche rien.

## Application (code)
- `apps/desktop/src/components/organisms/ServerRow.vue` (`updateAvailable`), `apps/desktop/src/stores/agentUpdates.ts::withUpdate`.

## Vérification
- `apps/desktop/src/stores/agentUpdates.test.ts` (« la liste des serveurs ») ; `apps/desktop/e2e/agent-update.spec.ts` (« la liste des serveurs ne nomme que celui qui a une mise à jour »).

## Cas limites
- Pendant la mise à jour la mention disparaît de la carte (l'opération est en cours).

## Règles liées
- BR-UPDATE-022

## Historique
- 2026-10-06 : création (HRT-17, lot interface, session 2026-10-04-hearth-creation, T28).
