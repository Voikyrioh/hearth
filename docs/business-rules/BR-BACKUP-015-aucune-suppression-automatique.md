---
id: BR-BACKUP-015
domaine: BACKUP
titre: Hearth ne supprime jamais une sauvegarde de lui-même : ni rotation, ni limite, ni nettoyage
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-sauvegardes-service.md (BR-BACKUP-015), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 6, 7 et 9.4), HRT-97
maj: 2026-11-14
---

# BR-BACKUP-015 : Hearth ne supprime jamais une sauvegarde de lui-même : ni rotation, ni limite, ni nettoyage

## Règle
Seul un compte qui a le droit supprime une sauvegarde, une par une (`DELETE /backups/{id}`, jamais couvert par l'élévation). La liste donne l'espace pris (`total_bytes`) et libre ; le client avertit au-delà de 10 sauvegardes (`WARN_ABOVE`). Les seules suppressions faites par Hearth sont celles de SES fichiers partiels (annulation, disque presque plein, délai, échec de vérification) : jamais une sauvegarde publiée et vérifiée.

## Application (code)
- `crates/hearth-agent/src/application/backups.rs::Backups::delete`
- `crates/hearth-proto/src/api/backups.rs::WARN_ABOVE`

## Vérification
- `crates/hearth-agent/tests/service_backups_https.rs::a_backup_of_hearth_is_deleted_with_its_manifest_and_nothing_else`

## Cas limites
- Aucun code de l'agent ne parcourt le registre pour supprimer.

## Règles liées
- BR-BACKUP-027

## Historique
- 2026-11-14 : création (HRT-97, côté agent).
