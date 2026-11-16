---
id: BR-DEPLOY-039
domaine: DEPLOY
titre: L'écran « Ajouter un serveur de jeu » propose trois chemins et ne crée rien
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-11-16-fonctionnelle-ajouter-un-serveur-de-jeu.md (BR-DEPLOY-039), HRT-243
maj: 2026-11-16
---

# BR-DEPLOY-039 — Les trois chemins d'ajout

## Règle
L'écran « Ajouter un serveur de jeu » (route `services/ajouter`, nom `services-add`) propose trois chemins, dans cet ordre : le Catalogue (conseillé), un programme du choix de l'utilisateur (« Continuer sans fiche », le formulaire d'image), un serveur de jeu déjà présent (l'aide « J'ai déjà un serveur de jeu »). Il ne crée rien. « Continuer sans fiche » est inactif avec sa raison écrite dessous quand le moteur de conteneurs n'est pas prêt ; « Ouvrir le Catalogue » et « Voir comment faire » sont toujours actifs. Le pied dit l'état du droit et du moteur. La route est gardée comme l'était le formulaire (`deployRouteGuard`).

## Application (code)
- `apps/desktop/src/pages/AddGameServer.vue`, `components/molecules/AddPathCard.vue`
- `apps/desktop/src/router/index.ts` (route `services-add`)

## Vérification
- `apps/desktop/src/pages/AddGameServer.test.ts`

## Règles liées
- BR-DEPLOY-001, BR-DEPLOY-027, BR-GRANT-014

## Historique
- 2026-11-16 — création (HRT-243).
