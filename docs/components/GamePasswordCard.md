# GamePasswordCard

Molécule · `apps/desktop/src/components/molecules/GamePasswordCard.vue`

La carte « Mot de passe du serveur » du détail d'un serveur de jeu (HRT-104, BR-JEU-023, 025, 027), sur `DashCard`. Masque FIXE de 8 puces ; « Révéler le mot de passe » est réservé au droit « gérer les services » et ne lit la valeur qu'à la demande (`stores/gamePassword.ts`, jamais persisté) ; « Mot de passe visible » ; remasqué, donc effacé, à la fermeture du détail, au changement de service, quand le lien n'est plus « Connecté », quand le droit est retiré ou que le moteur n'est plus prêt. Les comptes sans le droit lisent « Réservé aux comptes qui peuvent gérer les services. », sans bouton. Un serveur sans mot de passe le dit, sans bouton. Aucun bouton « copier » : la valeur n'entre pas dans le presse-papiers.

- Props : `serverId`, `serviceKey`, `hasPassword`, `gate`, `reasonId`, `connected`, `engineReady`, `gone`
- Notes : posée par `GameCards`. Tests : `pages/ServicesGame.test.ts`.
- Aucune valeur visuelle littérale : `npm run lint` échoue sur une couleur ou un `px` hors de `tokens.css`.
- Responsive : captures 1366 et 2560 px (`npm run e2e`, `apps/desktop/e2e/screenshots/`, non commitées).
