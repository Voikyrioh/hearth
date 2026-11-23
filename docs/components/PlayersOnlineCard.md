# PlayersOnlineCard

Molécule · `apps/desktop/src/components/molecules/PlayersOnlineCard.vue`

La carte « Joueurs connectés » (HRT-291, BR-CONSOLE-011, maquette Connectes) : « N sur M places », les pseudos, l'âge de la lecture et « relire ». Posée en tête de la vue d'ensemble d'un serveur Minecraft (par `GameCards`), dans l'onglet Console (HRT-269) et sur l'écran « Vérifiez avant d'appliquer » des mods (HRT-263). Jamais un chiffre deviné : arrêté, qui démarre ou console muette, elle dit une phrase fixe. Un service sans console parlée n'a pas de carte. Les pseudos sont du texte du jeu, montrés tels quels (jamais du HTML). Lue par `composables/useOnlinePlayers.ts` (à l'ouverture, toutes les 30 s tant que l'écran existe, à la demande) ; règles pures `services/players.ts`.

- Props : `state` (`OnlinePlayersState`), `now` (ms), `refreshing`, `compact` (version courte), `linkLabel` (texte du lien vers la console, absent : pas de lien)
- Événements : `refresh`, `openConsole`
- Notes : tests `components/molecules/playersOnline.test.ts`.
