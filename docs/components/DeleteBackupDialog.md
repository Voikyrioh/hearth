# DeleteBackupDialog

Organisme · `apps/desktop/src/components/organisms/DeleteBackupDialog.vue`

La confirmation de la suppression d'une sauvegarde (BR-BACKUP-027) : « Il n'y a pas de corbeille », mention de la seule sauvegarde vérifiée, ligne seule si le fichier a été remplacé ; acte `backup_delete`, jamais couvert par l'élévation (mot de passe toujours demandé).

- Props : `open`, `serverId`, `backup`, `backups`, `perform`
- Événements : `close`
- Notes : tests `pages/ServicesBackups.test.ts`.
