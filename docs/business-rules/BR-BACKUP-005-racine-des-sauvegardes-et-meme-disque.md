---
id: BR-BACKUP-005
domaine: BACKUP
titre: La racine des sauvegardes est `/var/lib/hearth-backups/` par défaut, réglée par l'administrateur du serveur dans la configuration, jamais depuis Hearth
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-sauvegardes-service.md (BR-BACKUP-005), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 6, 7 et 9.4), HRT-97
maj: 2026-11-14
---

# BR-BACKUP-005 : La racine des sauvegardes est `/var/lib/hearth-backups/` par défaut, réglée par l'administrateur du serveur dans la configuration, jamais depuis Hearth

## Règle
`[backups] root` de `agent.toml` ou `HEARTH_BACKUPS_ROOT`. Un sous-dossier par service (sa clé `h_…`, `0700` à l'utilisateur de l'agent) contient `<id>.tar.gz` et `<id>.manifest.json`, `0600`. La liste dit si la racine est sur le même disque que les données (`same_disk`). Hors Linux (poste de développement), la racine par défaut est `hearth-backups` à côté du dossier de données.

## Application (code)
- `crates/hearth-agent/src/infrastructure/config/mod.rs::ServicesConfig::backups_root`
- `crates/hearth-agent/src/infrastructure/backup_files.rs`
- `crates/hearth-agent/src/application/backups.rs::Backups::same_disk`

## Vérification
- `crates/hearth-agent/src/infrastructure/config/mod.rs::tests`
- `crates/hearth-agent/src/infrastructure/backup_files.rs::tests::the_root_is_created_private_and_a_link_is_never_a_root (Linux)`

## Cas limites
- Une racine qui est un lien, ou qui n'appartient pas à l'agent, ou que le groupe ou les autres modifient : refusée (`backups_root_unavailable`).

## Règles liées
- BR-BACKUP-004

## Historique
- 2026-11-14 : création (HRT-97, côté agent).
