---
id: BR-BACKUP-008
domaine: BACKUP
titre: Le mode « Arrêter, sauvegarder, relancer » arrête par le cycle de vie, copie, puis relance seulement ce qui tournait, même si la sauvegarde échoue ou est annulée
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-sauvegardes-service.md (BR-BACKUP-008), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 6, 7 et 9.4), HRT-97
maj: 2026-11-14
---

# BR-BACKUP-008 : Le mode « Arrêter, sauvegarder, relancer » arrête par le cycle de vie, copie, puis relance seulement ce qui tournait, même si la sauvegarde échoue ou est annulée

## Règle
Étapes `stop`, `copy`, `verify`, `start`. L'arrêt voulu est écrit avant l'ordre, avec le délai de grâce du service ; une coupure forcée est dite (repères `forced_stop` et `maybe_inconsistent`). Seuls les conteneurs que CETTE opération a arrêtés sont relancés. Si le service ne repart pas : il reste « Arrêté », l'opération réussie porte la cause `not_restarted`. Le démarrage de l'agent au milieu laisse le service arrêté (opération « interrompue », rien n'est rejoué).

## Application (code)
- `crates/hearth-agent/src/application/backups.rs::Backups::stop_service, ::start_service, ::finish_after`
- `crates/hearth-agent/src/domain/ops/backup.rs::op_plan`

## Vérification
- `crates/hearth-agent/tests/service_backups_https.rs::stopping_to_copy_stops_with_the_service_delay_copies_then_restarts_what_ran`
- `crates/hearth-agent/tests/service_backups_https.rs::cancelling_a_backup_removes_the_part_written_and_restarts_a_service_it_stopped`

## Cas limites
- Un arrêt qui échoue avec un refus net oublie l'arrêt voulu ; un résultat inconnu le garde.

## Règles liées
- BR-BACKUP-006
- BR-LIFE-010

## Historique
- 2026-11-14 : création (HRT-97, côté agent).
