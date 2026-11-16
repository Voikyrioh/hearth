# AddGameServer

Page · `apps/desktop/src/pages/AddGameServer.vue`

L'écran d'entrée « Ajouter un serveur de jeu » (HRT-243, BR-DEPLOY-001, 039) : trois chemins (Catalogue conseillé, « Continuer sans fiche », « J'ai déjà un serveur de jeu »), « Fermer » en tête, et en pied l'état du droit et du moteur. Il ne crée rien. « Continuer sans fiche » est inactif avec sa raison quand le moteur n'est pas prêt.

- Notes : Route `/servers/:id/services/ajouter` (nom `services-add`, gardée par `deployRouteGuard`). Tests : `pages/AddGameServer.test.ts`.
