# StaleSurface

Molécule · `apps/desktop/src/components/molecules/StaleSurface.vue`

Enveloppe commune des données périmées : quand `stale`, le contenu est désaturé (`grayscale(.85)`, opacité .62) et daté par un `StaleStamp` en haut à droite. Le contenu reste lisible, jamais retiré. BR-RESIL-007.

- Props : `stale`, `lastContactAt`
- Événements et slots : slot par défaut
- Notes : Pose `data-stale="true"`. Jetons : `--grayscale-stale`, `--opacity-stale`. Tests : `molecules.test.ts`, `shell.test.ts`.

- HRT-12 : monté UNE fois par `ServerLayout` autour de la page (désaturation, opacité 0,62, âge en direct) quand le lien n'est pas « Connecté » : une page ne l'enveloppe pas elle-même (BR-RESIL-007). Vérifié par `shell.test.ts` (toutes les routes) et `e2e/offline.spec.ts`.
- HRT-38 : prop `stamped` (faux : page sans donnée lue, aucune estampille) ; le gabarit estompe et date toujours dès « Reconnexion… » (BR-RESIL-007, question ouverte pour Voiky). Composable `usePageData` (`providePageData` dans `ServerLayout`, `usePageData` dans les pages Comptes et Sécurité).
