# ServerNav

Organisme · `apps/desktop/src/components/organisms/ServerNav.vue`

Navigation du serveur (208 px) : nom, adresse en chasse fixe, entrées « Tableau de bord », « Comptes », « Journal d'activité », « Sécurité ». « Comptes » et « Journal d'activité » sont absentes pour le rôle Lecture seule (BR-ACCT-013) ; « Sécurité » est ouverte à tous (HRT-23). Entrée active : fond carte et puce braise. Flèches haut/bas, Début, Fin.

- Props : `server` (`ServerInfo`)
- Événements et slots : aucun
- Notes : Utilise `vue-router`. Tests : `organisms.test.ts`.
