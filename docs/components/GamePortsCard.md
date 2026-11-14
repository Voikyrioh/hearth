# GamePortsCard

Molécule · `apps/desktop/src/components/molecules/GamePortsCard.vue`

La carte « Ports à ouvrir » du détail d'un serveur de jeu (HRT-104, BR-JEU-017 à 019), sur `DashCard` : tableau de lecture (port, protocole, rôle en mots), « Copier la liste » (« {port}/{protocole} », une ligne par port, notification « Liste copiée. »), la phrase qui dit d'ouvrir ces ports sur la box et le pare-feu, « Hearth ne peut pas vérifier que ces ports sont joignables depuis Internet. » et, tant qu'un numéro diffère de celui de la fiche, un rappel permanent « Les ports ont changé : mettez à jour votre box et votre pare-feu. ». Coexiste avec la carte « Ports » du lot 1.

- Props : `ports` (`GamePort[]`)
- Notes : posée par `GameCards`. Tests : `pages/ServicesGame.test.ts`, `services/game.test.ts`.
- Aucune valeur visuelle littérale : `npm run lint` échoue sur une couleur ou un `px` hors de `tokens.css`.
- Responsive : captures 1366 et 2560 px (`npm run e2e`, `apps/desktop/e2e/screenshots/`, non commitées).
