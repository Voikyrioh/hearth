# ConsoleExchange

Molécule · `apps/desktop/src/components/molecules/ConsoleExchange.vue`

Les commandes envoyées depuis cet écran et ce qui leur est arrivé (HRT-269, BR-CONSOLE-002, 004, 007) : la commande et l'heure, la réponse du jeu en texte brut, la phrase de l'interface (réponse vide, pas de réponse à temps, console injoignable), la commande refusée barrée avec ce qu'il faut faire à la place. Écart à la maquette : les entrées sont dans une liste sous le journal, pas intercalées à leur heure dans le journal.

- Props : entries
- Événements : aucun
- Notes : tests `pages/ServicesConsole.test.ts`, e2e `e2e/console.spec.ts`.
