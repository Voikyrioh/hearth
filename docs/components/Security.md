# Security

Page · `apps/desktop/src/pages/Security.vue`

Sécurité d'un serveur (HRT-23, HRT-26), titre « Sécurité et mode attaque », ouverte à tous les rôles : chacun voit l'état du mode et SES postes de confiance (l'agent est l'arbitre). Contient la carte « Mode attaque » (`AttackModePanel`, au-dessus : c'est l'action urgente), « Tes postes de confiance » (`TrustedDeviceTable`) et la fenêtre de retrait (`RemoveDeviceDialog`). Les bandeaux d'alerte et de mode attaque sont posés par `ServerLayout`, pas par la page. L'état et la liste se relisent à chaque retour du lien et à chaque issue d'opération incertaine (stores `security` et `devices`).

- Props : aucune
- Événements et slots : aucun
- Notes : Route `/servers/:id/security`, entrée « Sécurité » de `ServerNav` pour tous les rôles. Tests : `pages/Security.test.ts`, `pages/SecurityMode.test.ts`, `router/shell.test.ts`, `e2e/security.spec.ts`, `e2e/attack-mode.spec.ts`.
