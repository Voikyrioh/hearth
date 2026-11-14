---
id: BR-BACKUP-032
domaine: BACKUP
titre: Chaque acte est inscrit au journal d'activité avec des nombres et une cause typée, jamais un chemin, un nom de fichier ni la note
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-sauvegardes-service.md (BR-BACKUP-032), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 6, 7 et 9.4), HRT-97
maj: 2026-11-14
---

# BR-BACKUP-032 : Chaque acte est inscrit au journal d'activité avec des nombres et une cause typée, jamais un chemin, un nom de fichier ni la note

## Règle
`Target::ServiceBackup` n'a aucun champ pour un chemin, un nom de fichier ou une note : « sauvegarde créée » (mode, fichiers, taille, arrêt forcé, fichiers changés), « sauvegarde supprimée » (taille, « ligne du registre seulement »), « éléments ignorés modifiés » (nombres). Causes : celles des opérations (`cancelled_by_account`, `timeout`, `interrupted`, `step_failed`) plus `no_space`, `disk_almost_full`, `backup_corrupt`, `not_restarted`. Vérifier, lister et estimer ne sont pas journalisés ; annuler la fenêtre n'écrit rien.

## Application (code)
- `crates/hearth-agent/src/domain/audit/event.rs::Target::ServiceBackup, ::BackupNote`
- `crates/hearth-agent/src/application/service_lifecycle.rs::reason_of`

## Vérification
- `crates/hearth-agent/tests/service_backups_https.rs::a_hot_backup_of_a_running_service_stops_nothing_and_ends_verified`
- `crates/hearth-agent/tests/service_backups_https.rs::the_note_is_sealed_in_the_registry_and_never_in_the_journal_the_manifest_or_the_clear`

## Cas limites
- —

## Règles liées
- BR-BACKUP-012
- BR-LIFE-018

## Historique
- 2026-11-14 : création (HRT-97, côté agent).
