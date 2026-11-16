# DeployReview

Organisme · `apps/desktop/src/components/organisms/DeployReview.vue`

Le récapitulatif « Vérifiez avant de lancer » (HRT-69, BR-DEPLOY-015, 018, 031) : la troisième étape de la PAGE « Déployer un service » (HRT-246, plus une fenêtre) pour tout ce qui part ET la confirmation du socle (`AdminActPanel`, posé sur `useAdminAct`, genre `service_deploy`, jamais couvert par l'élévation de 5 minutes). Nom, image et version (« par défaut »), ports et exposition, nombre de variables et leurs noms (valeurs masquées), commande personnalisée ou non (le texte n'est pas répété), redémarrage, ressources, puis les points d'attention qui s'appliquent, chacun une case : port réservé par un service arrêté, manque de place. « Déployer le service » reste inactif tant qu'un point n'est pas validé. « Modifier » revient au formulaire intact.

- Props : `serverId`, `serverName`, `draft`, `check` (constats frais), `forceLowDisk` (refus de l'agent faute de place), `perform(adminPassword, accept)`
- Événements : `leave` (« Modifier » : le formulaire intact), `launched` (l'envoi est parti)
- Notes : posé par `pages/DeployService.vue` ; le parcours est dans `composables/useDeployFlow.ts`. Tests : `pages/DeployFlow.test.ts`.
