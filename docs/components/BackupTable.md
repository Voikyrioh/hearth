# BackupTable

Organisme · `apps/desktop/src/components/organisms/BackupTable.vue`

La table des sauvegardes, de la plus récente à la plus ancienne (BR-BACKUP-012, 013) : date et repères, note, taille, mode, intégrité, « Vérifier » et « Supprimer » (inactifs et expliqués sans le droit), ligne dépliable.

- Props : `backups`, `verifying`, `notes`, `canWrite`, `rightsReasonId`
- Événements : `verify`, `remove`
- Notes : tests `pages/ServicesBackups.test.ts`.
