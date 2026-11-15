# GameCards

Organisme · `apps/desktop/src/components/organisms/GameCards.vue`

Les quatre cartes de plus d'un serveur de jeu, AU-DESSUS de « Général » (HRT-104, BR-JEU-001) : adresse, ports à ouvrir, mot de passe, arrêt du jeu. Elle lit la vue de l'agent (`GET /services/{key}/game`, `composables/useGameView.ts`) à l'ouverture du détail d'un service créé par Hearth (ni adopté, ni en cours de déploiement) et la relit quand l'état, les ports ou l'opération qui tient le service changent. Un service qui n'est pas de fiche (404 `NOT_A_GAME_SERVICE`) ou une lecture en échec ne montre aucune carte : rien n'est inventé.

- Props : `serverId`, `service`, `gate`, `reasonId`, `connected`, `engineReady`, `gone`, `supported` (capacité `game-settings`)
- Notes : posé par `ServiceDetail`. Tests : `pages/ServicesGame.test.ts`.
- Aucune valeur visuelle littérale : `npm run lint` échoue sur une couleur ou un `px` hors de `tokens.css`.
- Responsive : captures 1366 et 2560 px (`npm run e2e`, `apps/desktop/e2e/screenshots/`, non commitées).
