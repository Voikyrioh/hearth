# ServerRail

Organisme · `apps/desktop/src/components/organisms/ServerRail.vue`

Barre des serveurs (64 px) : logo (retour à l'accueil), un `ServerAvatar` par serveur enregistré (clic = son tableau de bord, anneau et `aria-current` sur le serveur affiché), infobulle au survol et au focus sur chaque outil, bulle de nom visuelle (`aria-hidden`, à droite de la barre, jamais sur une autre icône) sur chaque lien, avatars compris (HRT-45) : un seul nom accessible par lien, lien « + » vers l'assistant d'ajout (`/servers/new`), lien « Mes serveurs » vers le carnet (dès qu'un serveur existe), réglages en bas. Chaque avatar suit l'état du lien de SON serveur (BR-RESIL-020). Flèches haut/bas, Début et Fin déplacent le focus.

- Props : aucune (lit les stores `servers` et `link`)
- Événements et slots : aucun
- Notes : Utilise `vue-router`. Tests : `organisms.test.ts`, `e2e/shell.spec.ts`.
- HRT-47 (S2) : 8 px de marge verticale dans la liste, l'anneau du serveur sélectionné n'est plus rogné. FIX:01M4EPX8PWN1RRX8A4HZGTKF0E.
