# Sauvegarder les données d'un service (HRT-97)

Capacité `service-backups` annoncée par [`GET /hello`](./hello.md). Six routes. La **liste** est ouverte à tout compte authentifié pour un service créé par Hearth ; tout le reste est au niveau « gérer les services » (`Access::Services`, quel que soit le rôle, `BR-BACKUP-001`). Règles `BR-BACKUP-001` à `016`, `025`, `027` à `029`, `032`, `033` ; décisions ADR-0047 (sauvegardes), ADR-0045 (opérations), ADR-0046 (fichiers). Types : `hearth_proto::api::backups`. Le téléchargement (HRT-95) et la restauration (HRT-98) n'existent pas encore : aucune route ne les porte.

Les chemins de ces réponses sont ceux du SERVICE (cible de montage, chemin relatif à ce dossier), jamais ceux de la machine.

## `GET /api/v1/services/{key}/backups`

`BackupsView` : `backups[]` (les plus récentes d'abord), `total_bytes`, `free_bytes` (disque des sauvegardes, absent si la mesure échoue), `same_disk`, `settings` (`ignored[]`, `included[]`), `nothing_to_save`. Une `BackupView` : `id`, `created_at`, `archive_bytes`, `data_bytes`, `files`, `mode` (`stopped`, `hot`, `stop_copy_start`), `flags[]` (`hot`, `maybe_inconsistent`, `forced_stop`, `before_restore`), `changed_files`, `note` (ouverte pour la réponse, jamais écrite ailleurs), `account`, `image`, `integrity` (`verified`, `corrupt`, `unchecked`, `missing`), `verified_at`. L'intégrité « Corrompue » est aussi rendue quand l'archive n'est plus celle que Hearth a écrite ; « Disparue » (`missing`) quand le fichier n'existe plus.

Erreurs : `404 SERVICE_NOT_FOUND` ; `409 SERVICE_ADOPTED` (les sauvegardes d'un service créé hors de Hearth ne sont pas prises en charge, `BR-BACKUP-028`) ; `409 ENGINE_UNAVAILABLE` ; `422 PATH_REFUSED` (`details.reason` : `backups_root_overlap` « La racine des sauvegardes chevauche un dossier de données. », ou `backups_root_unavailable`).

## `POST /api/v1/services/{key}/backups/preview`

La fenêtre de création : une consultation du droit, rien n'est écrit ni journalisé. `BackupPreview` : `running`, `stop_grace_s`, `roots[]` (cibles sauvegardées), `not_saved[]` (dossiers du serveur, jamais lus), `bytes`, `files`, `partial` (60 s, « au moins »), `big_files[]` (1 Gio ou plus, les plus gros d'abord, 20 au plus), `ignored[]` (avec `why` : `looks_like_backups`, `chosen`), `free_bytes`, `reserve_bytes` (le plus grand de 5 Gio et de 5 % du disque), `enough_space` (faux : aucune case ne permet de passer outre), `same_disk`, `nothing_to_save`.

## `POST /api/v1/services/{key}/backups` (acte `BackupCreate` `0x19`)

Acte d'administration confirmé, couvert par l'élévation. Cible signée : la clé du service. Paramètres signés : le mode et le nombre d'éléments de `ignore_once` ; ni la note ni un chemin.

- **Corps** (`BackupRequest`, avec `reauth` à côté) : `mode` (`hot` ou `stop_copy_start` pour un service en marche ; pour un service qui n'est pas en marche le mode effectif est toujours `stopped`), `note` (80 caractères au plus, facultative), `ignore_once[]` (`{ root, path }`, pour cette sauvegarde seulement).
- **Réponse `202`** (`BackupAccepted`) : `{ operation_id, backup_id }`. L'issue se lit dans l'opération ([opérations de service](./service-ops.md), `kind: backup`) : étapes `stop`, `copy`, `verify`, `start` (selon le mode), progression `items_done`, `bytes_done`, `bytes_total`, annulable jusqu'à la fin.
- **Issues** : `succeeded` ; `succeeded` avec la cause `not_restarted` (le service arrêté ne repart pas, il reste « Arrêté ») ; `failed` avec `no_space`, `disk_almost_full` (la partie écrite est supprimée), `backup_corrupt` (la vérification de création a échoué : archive et manifeste supprimés), `timeout` (6 h), `engine_*`, `step_failed` ; `cancelled` ; `interrupted` après un redémarrage de l'agent (la partie écrite est retirée au démarrage après vérification de l'inode, rien n'est repris).
- **Erreurs** : `403` sans le droit ; `404 SERVICE_NOT_FOUND` ; `409 SERVICE_ADOPTED` ; `409 SERVICE_BUSY` (`details.by`, `details.kind`) ; `409 HEAVY_IO_BUSY` (une autre sauvegarde ou restauration tient la place) ; `409 SERVICE_NOT_DRIVABLE` (déploiement en cours, pause...) ; `409 ENGINE_UNAVAILABLE` ; `507 NO_SPACE` (`details.needed_bytes`, `free_bytes`, `reserve_bytes`, `missing_bytes`, `free_after_bytes`) ; `422 VALIDATION_ERROR` (`details.field` : `mode`, `note`, `items`, `service` avec la raison `nothing_to_save`) ; `422 PATH_REFUSED` (racine).
- **Journal d'activité** (`BR-BACKUP-032`) : `service.backup.create`, « service {nom}, sauvegarde créée ({mode}, {n} fichiers, {taille}) » avec, le cas échéant, « , arrêt forcé » et « , {n} fichiers changés pendant la copie : peut être incohérente ». Jamais un chemin, un nom de fichier ni la note.

## `POST /api/v1/backups/{id}/verify`

Relit l'archive et la juge contre son manifeste (`BR-BACKUP-013`). Une consultation du droit : elle n'écrit que l'état d'intégrité, rien au journal, pas de verrou de service. `VerifyResult` : `integrity` (`verified`, `corrupt`, `missing`), `verified_at`, `files_checked`. Synchrone dans cette tranche. Erreurs : `404 BACKUP_NOT_FOUND` (aussi pour un identifiant que Hearth n'a pas écrit : un fichier de la racine qui n'est pas au registre n'existe pas pour lui), `409 BACKUP_IN_USE`.

## `DELETE /api/v1/backups/{id}` (acte `BackupDelete` `0x1B`)

Jamais couvert par l'élévation (le mot de passe à chaque fois). Cible signée : l'identifiant. Retire l'archive puis le manifeste voisin (seulement s'il a l'empreinte du registre) puis la ligne. **Si l'archive n'est plus celle que Hearth a écrit** (autre inode) ou a disparu : seule la ligne du registre part, le fichier n'est pas touché. `BackupDeleted` : `removed_files`. Erreurs : `404 BACKUP_NOT_FOUND`, `409 BACKUP_IN_USE`. Journal : `service.backup.delete`.

## `PUT /api/v1/services/{key}/backup-settings` (acte `BackupSettings` `0x1C`)

Les éléments ignorés du service (`BR-BACKUP-003`). Cible signée : la clé du service. Paramètres signés : le nombre d'éléments et l'empreinte de leur liste (`BackupSettings::act_digest`), jamais un chemin. Corps (`BackupSettings`) : `ignored[]` (éléments ajoutés), `included[]` (ignorés par défaut que le compte fait entrer dans les sauvegardes). Réponse `200` : la liste enregistrée. Journal : `service.backup.settings`, avec les nombres. Erreurs : `422 VALIDATION_ERROR` (`items`).

## Code

`entrypoint/http/backups.rs`, `application/backups.rs` (`Backups`), `domain/backups/`, `domain/ops/backup.rs`, `infrastructure/{backup_archive,backup_files}.rs`, `infrastructure/sqlite/backups_repo.rs`, migration `0017`.
