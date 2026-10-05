---
id: BR-UPDATE-025
domaine: UPDATE
titre: Les réglages affichent la version du client (et, plus tard, celle de l'agent)
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-mises-a-jour.md (BR-UPDATE-025), HRT-16
maj: 2026-10-05
---

# BR-UPDATE-025 : Les réglages affichent la version du client (et, plus tard, celle de l'agent)

## Règle
La version du client qui tourne s'affiche dans les réglages (ligne « Version »), avec l'état des mises à jour (BR-UPDATE-007, 026). **Application partielle** : la version de l'AGENT de chaque serveur et la mention « Mise à jour disponible » du serveur relèvent de l'état du serveur (BR-UPDATE-022, 023), livrés avec l'écran de la mise à jour de l'agent.

## Application (code)
- `apps/desktop/src/pages/Settings.vue` (ligne « Version », commande `get_app_version`) ; `apps/desktop/src/components/organisms/UpdatePanel.vue`.

## Vérification
- `apps/desktop/src/pages/Settings.test.ts` (version affichée) ; `apps/desktop/src/components/organisms/updates.test.ts`.

## Cas limites
- Version de l'agent : à venir.

## Règles liées
- BR-UPDATE-022, BR-UPDATE-026

## Historique
- 2026-10-05 : création (HRT-16, session 2026-10-04-hearth-creation).
