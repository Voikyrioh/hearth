# GameSequenceDialog

Organisme · `apps/desktop/src/components/organisms/GameSequenceDialog.vue`

La séquence d'un arrêt ou d'un redémarrage de serveur de jeu (HRT-104, BR-JEU-003, 005, 008, 011, 012), sur `FormDialog` : « Sauvegarde du monde » (avec le temps écoulé), « Arrêt » (avec le décompte du délai d'arrêt de la fiche), puis « Démarrage » pour un redémarrage. « Le monde est enregistré. » une fois l'étape faite. « Fermer » n'arrête rien : la séquence continue sur le serveur et figure dans la bande « Opérations en cours ». L'issue se dit dans la fenêtre (`outcomeOf`) ; le résultat inconnu ne relance rien. Quand la confirmation manque, la fenêtre de décision prend la main (`OperationDecisionDialog`) et cette vue garde « Répondre ».

- Props : `open`, `sequence` (`GameSequence`), `operation`, `ended`
- Notes : événements `close`, `decide` ; tenue par `Services.vue` avec `useServiceLifecycle` et `useOperationFollow`. Tests : `pages/ServicesGame.test.ts`.
- Aucune valeur visuelle littérale : `npm run lint` échoue sur une couleur ou un `px` hors de `tokens.css`.
- Responsive : captures 1366 et 2560 px (`npm run e2e`, `apps/desktop/e2e/screenshots/`, non commitées).
