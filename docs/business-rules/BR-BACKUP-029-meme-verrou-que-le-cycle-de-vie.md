---
id: BR-BACKUP-029
domaine: BACKUP
titre: Une sauvegarde prend le verrou exclusif du service, du début à la fin ; une autre action reçoit « Une action est déjà en cours sur ce service »
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-sauvegardes-service.md (BR-BACKUP-029), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 6, 7 et 9.4), HRT-97
maj: 2026-11-14
---

# BR-BACKUP-029 : Une sauvegarde prend le verrou exclusif du service, du début à la fin ; une autre action reçoit « Une action est déjà en cours sur ce service »

## Règle
Le verrou est celui des opérations (`OpKind::Backup`, exclusif). Vérifier, lister et estimer ne le prennent pas. Deux comptes qui demandent en même temps : le premier gagne.

## Application (code)
- `crates/hearth-agent/src/domain/ops/kind.rs::OpKind::lock_mode`
- `crates/hearth-agent/src/application/backups.rs::Backups::create`

## Vérification
- `crates/hearth-agent/tests/service_backups_https.rs::one_heavy_operation_at_a_time_and_the_service_is_busy_for_its_own_actions`

## Cas limites
- —

## Règles liées
- BR-LIFE-013
- BR-BACKUP-010

## Historique
- 2026-11-14 : création (HRT-97, côté agent).
