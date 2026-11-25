# LogExchangeRow

Molécule · `apps/desktop/src/components/molecules/LogExchangeRow.vue`

Une commande envoyée depuis l'onglet Console, rangée dans le journal (`LogViewer`) à son heure, avec la réponse du jeu en texte brut ou la phrase de l'interface (HRT-269). Remplace `ConsoleExchange`. Entrée `exchange` du tampon `LogBuffer`, ajoutée par `serviceLogs.pushExchange` ; trouvée par la recherche du journal.

- Props : `entry`, `query`, `active`, `current`
