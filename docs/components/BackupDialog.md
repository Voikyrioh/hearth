# BackupDialog

Organisme · `apps/desktop/src/components/organisms/BackupDialog.vue`

La fenêtre de création d'une sauvegarde (BR-BACKUP-006 à 012, 033) : estimation lue (jusqu'à 60 s), choix du mode seulement pour un service en marche, note de 80 caractères, gros fichiers, éléments ignorés cette fois, chiffres d'espace ; UNE fenêtre pour l'effet et la confirmation (acte `backup_create`, couvert par l'élévation).

- Props : `open`, `serverId`, `preview`, `perform`
- Événements : `close`, `retry`
- Notes : tests `pages/ServicesBackups.test.ts`.
