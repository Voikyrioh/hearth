# ServerLayout

Gabarit · `apps/desktop/src/layouts/ServerLayout.vue`

Gabarit d'un serveur (`/servers/:id`) : `ServerNav` à gauche ; à droite `AppHeader` (hors de la frontière d'erreur : la pastille reste visible), `OfflineBanner` si « Hors ligne », puis la page dans `ErrorBoundary`. Pose `servers.currentId` d'après la route.

- Props : aucune
- Événements et slots : slot : `RouterView` (pages enfants)
- Notes : Tests : `shell.test.ts`.
