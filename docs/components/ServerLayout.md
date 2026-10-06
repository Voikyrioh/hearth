# ServerLayout

Gabarit · `apps/desktop/src/layouts/ServerLayout.vue`

Gabarit d'un serveur (`/servers/:id`) : `ServerNav` à gauche ; à droite `AppHeader` et `OfflineBanner` (si « Hors ligne ») HORS de toute frontière d'erreur : la pastille reste toujours visible ; la page seule est dans `ErrorBoundary`. Pose `servers.currentId` d'après la route.

- Props : aucune
- Événements et slots : slot : `RouterView` (pages enfants)
- Notes : Tests : `shell.test.ts`.

- HRT-12 : enveloppe la page de `StaleSurface` (données périmées, BR-RESIL-007) ; le panneau de reconnexion et le bandeau restent HORS de la surface désaturée.
