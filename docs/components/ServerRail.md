# ServerRail

Organisme · `apps/desktop/src/components/organisms/ServerRail.vue`

Barre des serveurs (64 px) : logo (retour à l'accueil), un `ServerAvatar` par serveur enregistré (clic = son tableau de bord, anneau et `aria-current` sur le serveur affiché), lien « + » vers l'assistant d'ajout (`/servers/new`), lien « Mes serveurs » vers le carnet (dès qu'un serveur existe), réglages en bas. Chaque avatar suit l'état du lien de SON serveur (BR-RESIL-020). Flèches haut/bas, Début et Fin déplacent le focus.

- Props : aucune (lit les stores `servers` et `link`)
- Événements et slots : aucun
- Notes : Utilise `vue-router`. Tests : `organisms.test.ts`, `e2e/shell.spec.ts`.
