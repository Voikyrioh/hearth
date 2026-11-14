---
id: BR-BACKUP-009
domaine: BACKUP
titre: L'espace nécessaire est estimé avant ; la sauvegarde est refusée sous la réserve (le plus grand de 5 Gio et de 5 % du disque), sans case pour passer outre ; la copie s'arrête sous la moitié de la réserve
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-sauvegardes-service.md (BR-BACKUP-009), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 6, 7 et 9.4), HRT-97
maj: 2026-11-14
---

# BR-BACKUP-009 : L'espace nécessaire est estimé avant ; la sauvegarde est refusée sous la réserve (le plus grand de 5 Gio et de 5 % du disque), sans case pour passer outre ; la copie s'arrête sous la moitié de la réserve

## Règle
`domain::backups::space` : refus si la taille des éléments inclus (sans la compression, estimation bornée à 60 s) dépasse l'espace libre moins la réserve (`507 NO_SPACE`, `needed_bytes`, `free_bytes`, `reserve_bytes`). Une mesure d'espace qui échoue n'est jamais un feu vert. Pendant la copie, l'espace est relu tous les 256 Mio lus, entre deux dossiers : sous la moitié de la réserve, la copie s'arrête et SA partie écrite est supprimée (cause `disk_almost_full`).

## Application (code)
- `crates/hearth-agent/src/domain/backups/space.rs`
- `crates/hearth-agent/src/application/backups.rs::Backups::create, ::Guarded, ::disk_low`

## Vérification
- `crates/hearth-agent/src/domain/backups/space.rs::tests`
- `crates/hearth-agent/tests/service_backups_https.rs::a_backup_that_would_eat_the_reserve_is_refused_with_the_figures_and_writes_nothing`
- `crates/hearth-agent/tests/service_backups_https.rs::a_disk_that_fills_during_the_copy_stops_it_and_removes_the_partial_archive`

## Cas limites
- Si l'estimation est partielle (60 s), la copie continue en surveillant l'espace.

## Règles liées
- BR-BACKUP-010

## Historique
- 2026-11-14 : création (HRT-97, côté agent).
