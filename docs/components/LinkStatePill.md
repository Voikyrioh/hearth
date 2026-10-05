# LinkStatePill

Molécule · `apps/desktop/src/components/molecules/LinkStatePill.vue`

Pastille d'état du lien : point de couleur + libellé exact « Connecté », « Reconnexion… », « Hors ligne », « Session expirée », « Accès révoqué ». Reconnexion : clignotement doux (1 s), coupé si mouvement réduit. `role="status"`. BR-RESIL-001.

- Props : `state` (`connected`, `reconnecting`, `offline`, `session_expired`, `access_revoked`)
- Événements et slots : aucun
- Notes : Jetons : `--ok`, `--warn`, `--crit`, `--pill-dot`, `--motion-pulse`. Tests : `molecules.test.ts`, `shell.test.ts`.
