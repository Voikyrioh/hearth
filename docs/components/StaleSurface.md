# StaleSurface

Molécule · `apps/desktop/src/components/molecules/StaleSurface.vue`

Enveloppe commune des données périmées : quand `stale`, le contenu est désaturé (`grayscale(.85)`, opacité .62) et daté par un `StaleStamp` en haut à droite. Le contenu reste lisible, jamais retiré. BR-RESIL-007.

- Props : `stale`, `lastContactAt`
- Événements et slots : slot par défaut
- Notes : Pose `data-stale="true"`. Jetons : `--grayscale-stale`, `--opacity-stale`. Tests : `molecules.test.ts`, `shell.test.ts`.
