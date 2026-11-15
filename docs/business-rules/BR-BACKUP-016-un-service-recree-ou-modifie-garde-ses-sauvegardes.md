---
id: BR-BACKUP-016
domaine: BACKUP
titre: Les sauvegardes sont attachées à l'identité du service (sa clé `h_…`) : un service recréé ou modifié garde les siennes
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-sauvegardes-service.md (BR-BACKUP-016), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 6, 7 et 9.4), HRT-97
maj: 2026-11-14
---

# BR-BACKUP-016 : Les sauvegardes sont attachées à l'identité du service (sa clé `h_…`) : un service recréé ou modifié garde les siennes

## Règle
Le registre est indexé par `service_key`, sans clé étrangère : les sauvegardes survivent à la description d'un service supprimé (BR-DEL-024, un écran des orphelines est possible sans rien changer). La restauration vérifiera la même identité (HRT-98).

## Application (code)
- `crates/hearth-agent/migrations/0017_backups.sql (backups.service_key)`
- `crates/hearth-agent/src/infrastructure/sqlite/backups_repo.rs`

## Vérification
- `crates/hearth-agent/tests/service_backups_https.rs::the_registry_drops_writing_lines_at_startup_and_keeps_published_ones`

## Cas limites
- « Recréer » et « Modifier » ne sont pas encore livrés (HRT-87).

## Règles liées
- BR-DEL-024

## Historique
- 2026-11-14 : création (HRT-97, côté agent).
