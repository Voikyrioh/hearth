---
id: BR-BACKUP-011
domaine: BACKUP
titre: Une sauvegarde en cours montre sa progression et peut être annulée : la partie écrite est supprimée, un service que Hearth avait arrêté est relancé
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-sauvegardes-service.md (BR-BACKUP-011), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 6, 7 et 9.4), HRT-97
maj: 2026-11-14
---

# BR-BACKUP-011 : Une sauvegarde en cours montre sa progression et peut être annulée : la partie écrite est supprimée, un service que Hearth avait arrêté est relancé

## Règle
La progression (fichiers, octets, estimation) est dans l'opération (`items_done`, `bytes_done`, `bytes_total`, `percent`), relue toutes les 500 ms. L'annulation (`DELETE /operations/services/{id}`) est permise jusqu'à la fin : l'opération passe `cancelling`, la copie s'arrête au fichier suivant (ou en cours de fichier), sa partie écrite et sa ligne partent, le service arrêté est relancé. Une annulation qui arrive APRÈS la fin de la copie (le statut « en annulation » n'a qu'une issue) supprime la sauvegarde déjà publiée, ses fichiers revérifiés par leur inode et sa ligne : c'est « la partie écrite » de ce que le compte a annulé ; l'opération est « annulée ».

## Application (code)
- `crates/hearth-agent/src/application/backups.rs::Backups::copy, ::report`

## Vérification
- `crates/hearth-agent/tests/service_backups_https.rs::cancelling_a_backup_removes_the_part_written_and_restarts_a_service_it_stopped`

## Cas limites
- La vue peut être quittée sans arrêter la copie.

## Règles liées
- BR-BACKUP-008

## Historique
- 2026-11-14 : création (HRT-97, côté agent).
