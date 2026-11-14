---
id: BR-BACKUP-027
domaine: BACKUP
titre: Supprimer une sauvegarde est refusé tant qu'une opération l'utilise ; Hearth ne supprime que ses propres sauvegardes
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-sauvegardes-service.md (BR-BACKUP-027), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 6, 7 et 9.4), HRT-97
maj: 2026-11-14
---

# BR-BACKUP-027 : Supprimer une sauvegarde est refusé tant qu'une opération l'utilise ; Hearth ne supprime que ses propres sauvegardes

## Règle
`DELETE /backups/{id}` (acte `0x1B`) : refusé (`409 BACKUP_IN_USE`) tant qu'une création ou une vérification l'utilise. Retire l'archive puis le manifeste voisin (seulement s'il a l'empreinte du registre) puis la ligne, dans cet ordre : un échec laisse une ligne « disparue », jamais un fichier orphelin hors registre. Le mot de passe est demandé à chaque fois. Le message « C'est la seule sauvegarde vérifiée de ce service » est affiché par le client. L'usage par un téléchargement ou une restauration viendra avec eux.

## Application (code)
- `crates/hearth-agent/src/application/backups.rs::Backups::delete, ::UseGuard`

## Vérification
- `crates/hearth-agent/tests/service_backups_https.rs::a_backup_of_hearth_is_deleted_with_its_manifest_and_nothing_else`
- `crates/hearth-agent/tests/service_backups_https.rs::an_archive_replaced_by_hand_is_corrupt_and_deleting_it_only_drops_the_registry_line`

## Cas limites
- —

## Règles liées
- BR-BACKUP-004
- BR-BACKUP-013
- BR-BACKUP-015

## Historique
- 2026-11-14 : création (HRT-97, côté agent).
