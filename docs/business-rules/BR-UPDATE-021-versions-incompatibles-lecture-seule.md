---
id: BR-UPDATE-021
domaine: UPDATE
titre: Versions incompatibles et compte Lecture seule : demander à un administrateur
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-mises-a-jour.md (BR-UPDATE-021), HRT-17
maj: 2026-10-06
---

# BR-UPDATE-021 : Versions incompatibles et compte Lecture seule : demander à un administrateur

## Règle
Si les versions ne sont pas compatibles et que l'agent est celui à mettre à jour, le message d'un compte Lecture seule dit : « Les versions du client et de l'agent ne sont pas compatibles. Demande à un administrateur de mettre à jour l'agent. ». Pour un client trop ancien, c'est la même phrase pour tous (le client est celui du PC de l'utilisateur).

## Application (code)
- `apps/desktop/src/components/organisms/OfflineBanner.vue` (prop `role`), `apps/desktop/src/components/organisms/AgentUpdateCard.vue` (`compatMessage`).

## Vérification
- `apps/desktop/src/components/organisms/connect.test.ts` ; `apps/desktop/src/components/organisms/agentUpdate.test.ts` ; `apps/desktop/e2e/agent-update.spec.ts` (« versions incompatibles »).

## Cas limites
- Le rôle est celui de la dernière connexion (aucune session n'est possible tant que les versions sont incompatibles).

## Règles liées
- BR-UPDATE-020, BR-UPDATE-011

## Historique
- 2026-10-06 : création (HRT-17, lot interface, session 2026-10-04-hearth-creation, T28).
