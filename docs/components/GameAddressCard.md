# GameAddressCard

Molécule · `apps/desktop/src/components/molecules/GameAddressCard.vue`

La carte « Adresse pour vos joueurs » du détail d'un serveur de jeu (HRT-104, BR-JEU-018, 020, 021), sur `DashCard` : « {adresse}:{port} » en police à chasse fixe et « Copier » (« Copiée » 2 secondes, notification « Adresse copiée. » annoncée ; un échec dit de copier à la main). Pour un jeu à plusieurs ports : « Ce jeu utilise d'autres ports : ils sont listés dans la carte « Ports à ouvrir ». ». Elle dit que l'adresse ne marche que depuis le réseau du serveur et que Hearth ne peut pas vérifier qu'elle est joignable. Service arrêté : le contenu est grisé et « Le service est arrêté : personne ne peut se connecter. » s'ajoute, hors du grisé.

- Props : `address` (`Address | null`, `services/game.ts::addressOf`), `stopped`
- Notes : posée par `GameCards`. Tests : `pages/ServicesGame.test.ts`, `services/game.test.ts`, `e2e/hrt104-game.spec.ts`.
- Aucune valeur visuelle littérale : `npm run lint` échoue sur une couleur ou un `px` hors de `tokens.css`.
- Responsive : captures 1366 et 2560 px (`npm run e2e`, `apps/desktop/e2e/screenshots/`, non commitées).
