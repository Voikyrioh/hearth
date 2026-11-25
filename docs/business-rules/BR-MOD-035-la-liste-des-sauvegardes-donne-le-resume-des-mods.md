---
id: BR-MOD-035
domaine: MOD
titre: La liste des sauvegardes d'un serveur à mods donne le résumé de la liste de chacune ; la lire ne demande que le droit de voir les sauvegardes
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-11-16-fonctionnelle-gerer-les-mods-d-un-serveur-minecraft.md
maj: 2026-11-18
---

# BR-MOD-035 : Le résumé des mods dans la liste des sauvegardes (partie agent)

## Règle
`GET /services/{key}/backups` ajoute à chaque sauvegarde d'un service qui a des mods un champ `mods` : l'état de sa liste (`recorded`, `partial`, `unreadable`, `not_in_backup`, `not_recorded`) et les nombres de mods actifs, désactivés et retirés ; absent pour un service sans mods (donc compatible avec un client plus ancien). `GET /services/{key}/backups/{id}/mods` rend la liste complète. Les deux ne demandent que le droit de voir les sauvegardes du service (BR-BACKUP-001) et ne sont pas inscrites au journal d'activité (BR-BACKUP-032). Le résumé est lu dans `backup_mod_sets` par une requête, sans lire les entrées. La colonne, le détail et « Copier la liste » sont l'affaire du client (HRT-294).

## Application (code)
- Client (HRT-294) : `apps/desktop/src/services/backupMods.ts::{columnText, copyOf}`, `BackupTable.vue` (colonne « Mods »), `BackupModsPanel.vue`
- `crates/hearth-agent/src/application/backups.rs::{Backups::list, Backups::mods_of}`
- `crates/hearth-agent/src/entrypoint/http/backups.rs::mods`

## Vérification
- `tests/service_backups_https.rs::mods_list::*`

## Règles liées
- BR-MOD-034, BR-MOD-036, BR-BACKUP-001, BR-BACKUP-032

## Historique
- 2026-11-18 : création (HRT-293), partie agent.
