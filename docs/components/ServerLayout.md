# ServerLayout

Gabarit · `apps/desktop/src/layouts/ServerLayout.vue`

Gabarit d'un serveur (`/servers/:id`) : `ServerNav` à gauche ; à droite `AppHeader` et `OfflineBanner` (si « Hors ligne ») HORS de toute frontière d'erreur : la pastille reste toujours visible ; la page seule est dans `ErrorBoundary`. Pose `servers.currentId` d'après la route.

- Props : aucune
- Événements et slots : slot : `RouterView` (pages enfants)
- Notes : Tests : `shell.test.ts`.
