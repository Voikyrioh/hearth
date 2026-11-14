---
id: BR-BACKUP-010
domaine: BACKUP
titre: Une seule opération lourde à la fois sur un serveur, tous services confondus ; une copie dure 6 heures au plus
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-sauvegardes-service.md (BR-BACKUP-010), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 6, 7 et 9.4), HRT-97
maj: 2026-11-14
---

# BR-BACKUP-010 : Une seule opération lourde à la fois sur un serveur, tous services confondus ; une copie dure 6 heures au plus

## Règle
L'opération `backup` prend la place `heavy_io` du serveur (`HEAVY_IO_BUSY`, `details.service`) et le verrou exclusif de son service. Au bout de 6 heures la copie s'arrête, sa partie écrite est supprimée (cause `timeout`). La priorité d'entrée-sortie basse n'est pas posée dans cette tranche (`ioprio_set` exige `unsafe`) : la copie tourne sur un fil bloquant ; l'effet est à mesurer sur forge.

## Application (code)
- `crates/hearth-agent/src/domain/ops/locks.rs (heavy_io)`
- `crates/hearth-agent/src/application/backups.rs::Backups::create, ::Control`

## Vérification
- `crates/hearth-agent/tests/service_backups_https.rs::one_heavy_operation_at_a_time_and_the_service_is_busy_for_its_own_actions`

## Cas limites
- Le verrou du serveur est en mémoire, vide au démarrage.

## Règles liées
- BR-BACKUP-029

## Historique
- 2026-11-14 : création (HRT-97, côté agent).
