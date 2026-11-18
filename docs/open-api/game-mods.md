# Mods d'un serveur de jeu installé (HRT-256, ADR-0062)

La liste des mods d'un serveur installé est lue dans le volume du service, écrit par le jeu et par ses mods : l'agent la tient pour hostile. Règles `BR-MOD-020`, `021`, `022`, `030`, `031`, `032`, `033` ; lecture d'archive : ADR-0055. Écrire (poser, retirer, désactiver) arrive avec HRT-258.

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

Écart avec la conception technique (section 7) : `identity.minecraft` (la plage en texte) est remplacée par `minecraft_declared` (la plage est comprise ou non) car la plage n'est gardée que sous forme d'intervalles ; `rank` est ajouté ; `SERVICE_ADOPTED` et `SERVICE_NOT_FOUND` pour un service sans fiche sont rendus `NOT_A_GAME_SERVICE` comme toutes les routes de jeu.

Erreurs : `404 SERVICE_NOT_FOUND` (clé mal formée), `404 NOT_A_GAME_SERVICE` (pas un service de fiche), `422 MODS_FOLDER_BLOCKED` (`data/mods` ou `data/mods-retires` lien ou pas un dossier), `500 INTERNAL_ERROR`.

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
