# BackupModsPanel

Molécule · `apps/desktop/src/components/molecules/BackupModsPanel.vue`

Les mods d'une sauvegarde, dans sa ligne dépliée de l'onglet Sauvegardes (HRT-294, BR-MOD-034 à 036, maquette Sauvegardes) : le moteur et la version de Minecraft au moment de la sauvegarde, la liste triée par nom (nom, version, moteur, état, origine), « Mods retirés : {r} » à part, « Copier la liste » (les mods activés, mise en forme de `services/modsCopy.ts`, titre « Mods de la sauvegarde du {date} ») et « Comparer avec les mods actuels » (`BackupModsCompare`). Une sauvegarde sans liste (avant la 0.2.2, illisible, dossier ignoré) le dit sans rien reconstituer. Lu par `composables/useBackupMods.ts` ; règles pures `services/backupMods.ts`.

- Props : `state` (`BackupModsEntry`), `createdAt`, `address`
- Événements : `compare`, `hideCompare`
- Notes : tests `pages/ServicesBackupsMods.test.ts`, e2e `e2e/mods.spec.ts`.

Le tableau ne déborde plus de sa carte (largeurs de BackupTable, résumé des mods à la ligne). La comparaison s'affiche à côté, sans bouton quand elle est montrée (plus de « Masquer la comparaison »).
