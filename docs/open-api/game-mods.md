# Mods d'un serveur de jeu installé (HRT-256, ADR-0062)

La liste des mods d'un serveur installé est lue dans le volume du service, écrit par le jeu et par ses mods : l'agent la tient pour hostile. Règles `BR-MOD-020`, `021`, `022`, `030`, `031`, `032`, `033` ; lecture d'archive : ADR-0055. Écrire (poser, retirer, désactiver) : `POST …/mods/apply` ci-dessous (HRT-258, `BR-MOD-024` à `027`).

## `GET /api/v1/services/{key}/mods`

Accès : tout compte authentifié, sans journal (`Access::Authenticated`, `audit: None`). Lecture seule : ni le volume ni la base ne changent.

```json
{
  "applicable": "mods | no_mods | plugins | no_way",
  "game": {
    "loader": "fabric | quilt | forge | neoforge",
    "minecraft": "1.20.1",
    "minecraft_from": "setting | log | modrinth_manifest | unknown",
    "updates_every_start": false,
    "pack": "none | server_pack | modrinth_pack"
  },
  "mods": [{
    "file": "sodium.jar",
    "rank": 0,
    "place": "active | retired",
    "enabled": true,
    "origin": "server_pack | hearth | modrinth_pack | manual",
    "readable_name": true,
    "identity": { "id": "sodium", "name": "Sodium", "version": "0.5.3", "loader": "fabric", "minecraft_declared": true, "environment": "both | server | client | unknown" },
    "verdict": { "code": "compatible | unverified | to_complete | incompatible_loader | incompatible_minecraft | duplicate | unrecognized", "missing": ["fabric-api"], "needed_by": ["iris"] }
  }],
  "unread": 0,
  "truncated": false
}
```

- `file` est absent quand `readable_name` est faux (nom hors de l'alphabet sûr) : le fichier se montre par son `rank`. `identity` est absent pour un jar non reconnu ou non lu.
- `enabled` est faux pour `.jar.disabled` ; un mod désactivé ou retiré garde son verdict mais ne compte ni pour les dépendances ni pour les doublons.
- `unread` : jars non lus faute de budget (20 s) ; le client relit, les jars lus sont en cache. `truncated` : plus de 1 024 fichiers dans un dossier.
- `game.minecraft` : le réglage de la description, sinon le manifeste Modrinth, sinon la ligne de démarrage de `logs/latest.log` (64 Kio) ; jamais deviné (`unknown`).
- `pack = server_pack` quand la base a des lignes d'un pack serveur pour ce service.

Écart avec la conception technique (section 7) : `identity.minecraft` (la plage en texte) est remplacée par `minecraft_declared` (la plage est comprise ou non) car la plage n'est gardée que sous forme d'intervalles ; `rank` est ajouté ; un service sans fiche est rendu `NOT_A_GAME_SERVICE` ; un service adopté est rendu `SERVICE_ADOPTED`, comme la console.

Erreurs : `404 SERVICE_NOT_FOUND` (clé mal formée), `404 NOT_A_GAME_SERVICE` (pas un service de fiche), `409 SERVICE_ADOPTED` (service adopté, comme la console), `422 MODS_FOLDER_BLOCKED` (`data/mods` ou `data/mods-retires` lien ou pas un dossier), `500 INTERNAL_ERROR`.

## `POST /api/v1/services/{key}/mods/check` (HRT-257)

Accès : droit « gérer les services » (niveau `Services`) ; une consultation (`READ_ONLY_POSTS`), seul un refus faute de droit est consigné (`service.mods`). Rien n'est écrit dans le service. Règle `BR-MOD-023`.

Corps : `{ "deposits": ["01J...", "..."] }` (1 à 8 identifiants de dépôts de la zone d'attente du compte, sans répétition).

```json
{ "files": [{
  "deposit": "01J...",
  "outcome": "add | replace | refused",
  "identity": { "id": "lithium", "name": "Lithium", "version": "0.11.2", "loader": "fabric", "minecraft_declared": true, "environment": "server" },
  "target_file": "lithium-0.11.2.jar",
  "replaces": { "file": "lithium-0.11.1.jar", "version": "0.11.1" },
  "verdict": { "code": "compatible | unverified | to_complete", "missing": [] },
  "refusal": "MOD_UNRECOGNIZED | MOD_ARCHIVE_REFUSED | MOD_TOO_LARGE | MOD_INCOMPATIBLE_LOADER | MOD_INCOMPATIBLE_MINECRAFT | MOD_DUPLICATE_IN_BATCH | MOD_NAME_TAKEN | MOD_IN_MODRINTH_PACK | MOD_SAME_VERSION",
  "refusal_values": { "loader": "forge", "server_loader": "fabric", "server_minecraft": "1.20.1" }
}]}
```

`identity` est donnée dès qu'elle est lue, même pour un refus de moteur ou de version. `target_file` est le nom choisi par Hearth, jamais le nom envoyé. Un dépôt refusé est retiré de la zone d'attente aussitôt ; un dépôt accepté y reste.

Écarts avec la conception technique (section 7) : `MODS_BATCH_INVALID` répond `422` (le protocole n'emploie pas `400`) ; `refusal_values.minecraft` (plage du mod en texte) est omis, la plage n'étant gardée que sous forme d'intervalles (`identity.minecraft_declared` dit seulement qu'elle est comprise).

Erreurs : `404 SERVICE_NOT_FOUND`, `404 NOT_A_GAME_SERVICE`, `404 DEPOSIT_UNKNOWN` (un dépôt inconnu ou d'un autre compte arrête tout le contrôle, rien n'est retiré), `409 MODS_NOT_APPLICABLE` (Vanilla, Paper, Purpur), `422 MODS_BATCH_INVALID`, `500 INTERNAL_ERROR`.

## `POST /api/v1/services/{key}/mods/apply` (HRT-258, HRT-259)

Acte signé `ModsApply` (`0x20`), **jamais couvert par l'élévation** : le mot de passe est toujours demandé. Droit « gérer les services ». Les paramètres signés : la clé du service, le nombre de changements et l'empreinte de leur liste triée (chacun préfixé de son genre : `add:{dépôt}`, `retire:`, `restore:`, `disable:`, `enable:` ; jamais un contenu).

```json
{
  "add": [{ "deposit": "01J…", "sha256": "hex", "replace": "lithium-0.11.1.jar" }],
  "retire": ["iris-1.6.4.jar"],
  "restore": ["clumps-12.0.0.3.jar"],
  "disable": ["fabric-carpet-1.20-1.4.112.jar"],
  "enable": ["iris-1.6.4.jar.disabled"]
}
```

Bornes : 1 à 64 changements, 8 ajouts au plus, noms de l'alphabet sûr (`.jar` ou `.jar.disabled`), aucun fichier de `mods` deux fois, un nom de `restore` une fois ; `disable` ne prend que des `.jar`, `enable` que des `.jar.disabled`. `sha256` est l'empreinte du dépôt que le client a calculée : l'agent la compare à celle qu'il lit, jamais cru. Réponse `202` : `{ "operation_id": "…", "backup_id": "…" }` ; suivi par `GET /operations/services/{id}` (genre `mods`, étapes `stop`, `copy`, `verify`, `place`, `start`, `watch`) et la queue de journal existante.

Refus synchrones (rien n'est arrêté ni écrit) : `404 NOT_A_GAME_SERVICE`, `409 SERVICE_ADOPTED`, `409 MODS_NOT_APPLICABLE`, `409 OPERATION_IN_PROGRESS` (autre opération sur le service), `409 HEAVY_IO_BUSY` (autre opération lourde sur le serveur), `422 MODS_BATCH_INVALID`, `422 MOD_REFUSED` (`details.deposit`, `details.refusal` : un code de `mods/check`, ou `MOD_REPLACE_MISMATCH` pour un `replace` qui ne désigne pas le fichier de même identifiant), `422 MOD_FILE_MISSING`, `422 MOD_IN_MODRINTH_PACK`, `422 MOD_ALREADY_ACTIVE`, `422 MOD_NAME_TAKEN`, `422 MODS_FOLDER_BLOCKED`, `507 NO_SPACE` / `422 PATH_REFUSED` selon l'espace des sauvegardes (BR-BACKUP-009), `401/403` acte non signé ou droit absent.

Écarts acceptés avec la conception technique (section 7) : un service sans fiche répond `404 NOT_A_GAME_SERVICE` (l'adopté répond `409 SERVICE_ADOPTED`, comme la conception et la console) ; les refus de verrou (`409 OPERATION_IN_PROGRESS`, `409 HEAVY_IO_BUSY`) et d'espace reprennent ceux des sauvegardes ; les bornes sont jugées avant la lecture du volume et répondent `422 MODS_BATCH_INVALID` ; les chemins des mouvements du journal (`FileMove`) sont relatifs à la racine du service (`data/mods/x.jar`), pas au dossier `data` ; l'empreinte `sha256` d'un dépôt est calculée par la coquille du client (commande `file_sha256`), jamais crue par l'agent, qui la recalcule.

Issue : `succeeded` ; `cancelled` (avant `place`) ; `failed` (`step_failed`, erreur au milieu de `place` : mouvements défaits) ; `rolled_back` (le serveur n'a pas dit « prêt » : `stopped_right_after_start`, `timeout`, `not_restarted`) ; `interrupted` (retour impossible). Journal d'activité : `service.mods`, des nombres et des noms sûrs.
