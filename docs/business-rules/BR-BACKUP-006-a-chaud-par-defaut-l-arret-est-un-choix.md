---
id: BR-BACKUP-006
domaine: BACKUP
titre: Un service qui n'est pas en marche se copie directement ; pour un service en marche le compte choisit entre la copie à chaud et l'arrêt, et Hearth n'arrête jamais un service sans ce choix
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-sauvegardes-service.md (BR-BACKUP-006), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 6, 7 et 9.4), HRT-97
maj: 2026-11-14
---

# BR-BACKUP-006 : Un service qui n'est pas en marche se copie directement ; pour un service en marche le compte choisit entre la copie à chaud et l'arrêt, et Hearth n'arrête jamais un service sans ce choix

## Règle
`effective_mode` : service arrêté, en erreur : mode `stopped` quel que soit le mode demandé ; service en marche : `hot` ou `stop_copy_start` seulement, `stopped` est refusé (`422`, `mode`) ; un mode inconnu est refusé. Rien n'est arrêté en mode `hot`.

## Application (code)
- `crates/hearth-agent/src/domain/ops/backup.rs::effective_mode`
- `crates/hearth-agent/src/application/backups.rs::Backups::create`

## Vérification
- `crates/hearth-agent/tests/service_backups_https.rs::a_running_service_needs_an_explicit_mode_and_is_never_stopped_without_it`
- `crates/hearth-agent/tests/service_backups_https.rs::a_service_that_is_not_running_is_copied_directly_whatever_mode_was_asked`
- `crates/hearth-agent/tests/service_backups_https.rs::a_hot_backup_of_a_running_service_stops_nothing_and_ends_verified`

## Cas limites
- —

## Règles liées
- BR-BACKUP-007
- BR-BACKUP-008

## Historique
- 2026-11-14 : création (HRT-97, côté agent).
