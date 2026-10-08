# ServerLayout

Gabarit · `apps/desktop/src/layouts/ServerLayout.vue`

Gabarit d'un serveur (`/servers/:id`) : `ServerNav` à gauche ; à droite `AppHeader` et `OfflineBanner` (si « Hors ligne ») HORS de toute frontière d'erreur : la pastille reste toujours visible ; la page seule est dans `ErrorBoundary`. Pose `servers.currentId` d'après la route.

- Props : aucune
- Événements et slots : slot : `RouterView` (pages enfants)
- Notes : Tests : `shell.test.ts`.

- HRT-12 : enveloppe la page de `StaleSurface` (données périmées, BR-RESIL-007) ; le panneau de reconnexion et le bandeau restent HORS de la surface désaturée.
- HRT-26 : pose, sous le bandeau hors ligne et HORS de la surface périmée, `AttackModeBanner` (mode actif ou suspendu) puis `SecurityAlertBanner` (alerte), et rend l'unique `AttackModeDialog` (état `security.dialog`). Le dernier état connu reste affiché quand le lien tombe (« Dernier état connu à {heure} »).
- HRT-42 : la colonne de contenu est bornée à `--page-max` (1 760 px) et alignée à gauche. FIX:01M4EDC0FVT0ZHGASFEMWX7066. Décision de Claude, à confirmer par Voiky (conception grand écran, décisions 3 à 6).
