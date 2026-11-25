---
id: BR-MOD-034
domaine: MOD
titre: Chaque sauvegarde d'un serveur à mods garde la liste de ses mods, exacte par rapport à l'archive, dans la base de l'agent
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-11-16-fonctionnelle-gerer-les-mods-d-un-serveur-minecraft.md
maj: 2026-11-18
---

# BR-MOD-034 : La liste des mods d'une sauvegarde

## Règle
Toute sauvegarde d'un service dont la fiche a `[mods.jars]` et dont la variante charge des mods enregistre la liste de ses mods : sauvegarde de l'utilisateur (à chaud ou avec arrêt), « Avant mods » (HRT-258), copie de sécurité d'une restauration plus tard. La lecture (`ModsSnapshot::read`, 60 s au plus, sans rien modifier) est prise AVANT l'arrêt et avant la copie. Elle est recoupée avec le MANIFESTE de la sauvegarde, vérité de ce que l'archive contient : un fichier de `mods` ou `mods-retires` n'y garde son identité et son origine que si son chemin ET son SHA-256 sont ceux du manifeste ; sinon il y figure sans identité (« non reconnu dans cette sauvegarde »), origine `manual`. Un fichier lu mais absent du manifeste (disparu pendant la copie) n'entre pas. Un jar non lu à temps rend la liste `partial`.

- Écrite dans la transaction qui passe la ligne à `ready` (`BackupRepo::mark_ready`) : une sauvegarde prête a sa liste, une interrompue n'en a pas. Retirée avec la sauvegarde, dans la même transaction (`BackupRepo::delete`). Tables `backup_mod_sets` et `backup_mod_entries` (migration `0019`), jamais le manifeste, l'archive ni le volume.
- Lire les mods ne fait JAMAIS échouer une sauvegarde : lecture en échec ou trop longue = état `unreadable`, aucune entrée ; une liste que la base refuse s'abandonne et la sauvegarde se publie sans.
- Dossier `mods` ignoré (BR-BACKUP-003) ou dossier géré absent de l'archive : `not_in_backup`, aucune entrée.
- Au plus 1 024 fichiers par dossier (`not_listed` compte le reste) ; nom hors alphabet sûr gardé sans nom (rang seulement).
- Une sauvegarde d'avant la 0.2.2 n'a pas de liste : `not_recorded`, jamais reconstituée.
- La plage de Minecraft visée par chaque mod n'est pas gardée (la colonne `minecraft_req` reste vide) : seuls l'identifiant, le nom, la version, le moteur et l'environnement le sont.

## Application (code)
- `crates/hearth-agent/src/domain/mods/backup_list.rs::reconcile`
- `crates/hearth-agent/src/application/backups.rs::{Backups::read_mods, Backups::mods_record, Backups::publish}`
- `crates/hearth-agent/src/application/ports/mods_snapshot.rs`, `application/mods.rs` (implémentation du port)
- `crates/hearth-agent/src/infrastructure/sqlite/backups_repo.rs::{write_mods, mods_of, mods_summaries}`

## Vérification
- `domain::mods::backup_list::tests::*`
- `tests/service_backups_https.rs::mods_list::*`

## Cas limites
- Sauvegarde avec arrêt : la lecture des mods a lieu avant l'étape d'arrêt (aucun appel au moteur depuis le début de l'opération).

## Règles liées
- BR-MOD-020, BR-MOD-033, BR-MOD-035, BR-MOD-036, BR-BACKUP-003, BR-BACKUP-012, BR-BACKUP-014, BR-BACKUP-027

## Historique
- 2026-11-18 : création (HRT-293).
