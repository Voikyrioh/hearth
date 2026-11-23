# ConsoleTab

Organisme · `apps/desktop/src/components/organisms/ConsoleTab.vue`

L'onglet « Console » d'un serveur de jeu dont l'agent parle à la console (HRT-269, BR-CONSOLE-002 à 010, maquettes Console, ConsoleSensible, ConsoleEtats) : journal en direct (LogViewer), commandes envoyées d'ici et leurs réponses, champ « Commande », commandes rapides de la fiche, carte des joueurs, encart « Ce que Hearth surveille », confirmation d'une commande sensible. Tenu par `composables/useServiceConsole.ts`.

- Props : serverId, service, view, gate, connected, engineReady, gone, lastContactAt
- Événements : refresh, start, follow
- Notes : tests `pages/ServicesConsole.test.ts`, e2e `e2e/console.spec.ts`.
