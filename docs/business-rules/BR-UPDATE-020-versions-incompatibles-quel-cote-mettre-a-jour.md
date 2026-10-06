---
id: BR-UPDATE-020
domaine: UPDATE
titre: Versions incompatibles : le message dit lequel mettre à jour, avec le bouton qui convient
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-mises-a-jour.md (BR-UPDATE-020), HRT-17
maj: 2026-10-06
---

# BR-UPDATE-020 : Versions incompatibles : le message dit lequel mettre à jour, avec le bouton qui convient

## Règle
Quand les versions du client et de l'agent ne sont plus compatibles (BR-CONN-014), un message dit lequel mettre à jour : « Les versions du client et de l'agent ne sont pas compatibles. Mets à jour l'agent. » ou « … Mets à jour le client. ». Client trop ancien : le bouton du bandeau lance la mise à jour du client (HRT-16, `Mettre à jour le client`), ou la cherche (`Chercher une mise à jour du client`) s'il n'y en a pas d'annoncée. Agent trop ancien : le message seul. **Limite** : le client ne peut pas mettre à jour l'agent dans ce cas, car l'agent répond `426 INCOMPATIBLE_VERSION` à toute route sauf `/hello` (BR-CONN-014) : ni session ni demande de mise à jour ne passent ; lever cette limite demande un changement de l'agent (ADR-0021). Le message est aussi celui de l'assistant d'ajout de serveur et de la carte « État du serveur ».

## Application (code)
- `apps/desktop/src/components/organisms/OfflineBanner.vue` (texte selon `blocked` et le rôle, bouton du client), `apps/desktop/src/layouts/ServerLayout.vue` (`clientAction`).
- `apps/desktop/src/components/organisms/AgentUpdateCard.vue` (`compat`).
- `apps/desktop/src/i18n/fr.ts` (`link.agentTooOld`, `link.clientTooOld`, `failure.*`).

## Vérification
- `apps/desktop/src/components/organisms/connect.test.ts` (« says which side to update… ») ; `apps/desktop/src/composables/useAddServer.test.ts` ; `apps/desktop/src/components/organisms/agentUpdate.test.ts` (« versions incompatibles ») ; `apps/desktop/e2e/agent-update.spec.ts`.

## Cas limites
- Aucune mise à jour de l'agent depuis l'application tant que les versions sont incompatibles (limite ci-dessus).

## Règles liées
- BR-CONN-014, BR-UPDATE-021, BR-UPDATE-002

## Historique
- 2026-10-06 : création (HRT-17, lot interface, session 2026-10-04-hearth-creation, T28).
