# GameStopCard

Molécule · `apps/desktop/src/components/molecules/GameStopCard.vue`

La carte « Arrêt du jeu » du détail d'un serveur de jeu (HRT-104, BR-JEU-010), sur `DashCard` : ce que le jeu fait de son monde en s'arrêtant, la phrase qui reste écrite en permanence (un arrêt hors de Hearth n'enregistre le monde que par l'enregistrement automatique du jeu) et le délai d'arrêt de la fiche.

- Props : `view` (`GameView`)
- Notes : posée par `GameCards`. Tests : `pages/ServicesGame.test.ts`, `services/game.test.ts`.
- Aucune valeur visuelle littérale : `npm run lint` échoue sur une couleur ou un `px` hors de `tokens.css`.
- Responsive : captures 1366 et 2560 px (`npm run e2e`, `apps/desktop/e2e/screenshots/`, non commitées).
