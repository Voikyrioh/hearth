# Security

Page · `apps/desktop/src/pages/Security.vue`

Sécurité d'un serveur (HRT-23), ouverte à tous les rôles : chacun voit et retire SES postes de confiance (l'agent est l'arbitre). Contient « Tes postes de confiance » (`TrustedDeviceTable`) et la fenêtre de retrait (`RemoveDeviceDialog`). La liste se relit à chaque retour du lien et à chaque issue d'opération incertaine (store `devices`). L'emplacement du panneau « Mode attaque » (HRT-26) est au-dessus de la liste : rien n'y est affiché tant qu'il n'existe pas.

- Props : aucune
- Événements et slots : aucun
- Notes : Route `/servers/:id/security`, entrée « Sécurité » de `ServerNav` pour tous les rôles. Tests : `pages/Security.test.ts`, `router/shell.test.ts`, `e2e/security.spec.ts`.
