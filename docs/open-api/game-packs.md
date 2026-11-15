# Packs de mods d'un serveur de jeu (HRT-124)

Un pack de mods se dépose dans la zone d'attente de l'agent (transfert reprenable) puis s'installe avec le serveur de jeu. L'agent LIT le pack (il ne le déballe pas), le range en lecture seule sous un nom fixe et passe son chemin au programme du jeu. Règles `BR-MOD-001`, `BR-MOD-007`, `BR-MOD-008`, `BR-MOD-012`, `BR-MOD-016`, `BR-MOD-017` ; décision ADR-0054 (points 4, 5 et 8) et ADR-0055 (lecture d'archive hostile).

- **Capacité** : `game-packs` dans `GET /hello`. Sans elle, un client ne montre pas le formulaire de pack. Annoncée depuis le câblage de la zone d'attente des transferts (HRT-95).
- **Aucune voie CurseForge** : l'agent ne contacte aucun hôte pour un pack, n'a aucune clé CurseForge, et une fiche ne peut écrire ni variable `CF_*` ni `AUTO_CURSEFORGE` (`BR-MOD-017`).

## `GET /api/v1/catalog/{id}` : le membre `mods`

Absent : la fiche n'offre aucune voie (`BR-MOD-014`). Sinon :

```json
{ "mods": {
    "ways": ["drop_files", "drop_pack"],
    "pack": { "genres": [
      { "kind": "server_pack",   "variants": ["fabric", "forge"] },
      { "kind": "modrinth_pack", "variants": ["modrinth"] } ] },
    "files_hint": "Dépose ici tes fichiers de mods." } }
```

`variants` : les variantes de la fiche (moteurs de serveur) permises avec ce genre. Un dossier de rôle `packs` apparaît aussi dans `folders` (lecture seule, hors sauvegarde).

## `POST /api/v1/deployments/pack-check`

Droit « gérer les services » (niveau `Services`), une consultation : rien n'est rangé, rien n'est écrit, aucun journal sauf un refus faute de droit. Corps :

```json
{ "deposit": "<identifiant du dépôt>" }
```

Réponse `200` :

```json
{ "genre": "server_pack", "size_bytes": 794497024, "files": 2314, "mods": 652,
  "unpacked_bytes": 1650000000, "top_dirs": ["config", "defaultconfigs", "mods"],
  "modrinth": null }
```

`genre` : `server_pack` ou `modrinth_pack`, lu dans le CONTENU. `modrinth` (pack Modrinth seulement) : `name`, `version`, `minecraft?`, `loader?` (`fabric-loader`, `quilt-loader`, `forge`, `neoforge`), `loader_version?`, `downloads`, `hosts` (parmi `cdn.modrinth.com`, `github.com`, `raw.githubusercontent.com`, `gitlab.com`). Textes nettoyés et bornés. Erreurs :

| Statut | Code | Quand |
|---|---|---|
| 404 | `PACK_UNKNOWN` | aucun dépôt complet de ce compte sous cet identifiant (inconnu, d'un autre compte, déjà rangé ou périmé) ; le dépôt d'un autre compte n'est jamais retiré |
| 422 | `PACK_REFUSED` | `details.reason` : `ARCHIVE_REFUSED` (avec `details.archive`, le code de l'ADR-0055), `PACK_IS_A_DOWNLOAD_LIST` (avec `details.mods`), `PACK_HOST_FORBIDDEN` (avec `details.host`), `PACK_PATH_ESCAPES`, `PACK_INDEX_INVALID`, `PACK_WRAPPED_IN_FOLDER`, `PACK_UNRECOGNIZED` ; le dépôt est retiré aussitôt |

Jamais un nom d'entrée de l'archive dans une réponse.

## `POST /api/v1/deployments` : `source.pack`

`source` (installation depuis une fiche) accepte `"pack": { "deposit": "<identifiant>" }`. L'agent juge le pack (mêmes refus que ci-dessus), exige que la fiche offre ce genre avec la variante choisie (`422 VALIDATION_ERROR`, `field = "source.pack"`, `reason = "not_offered"`, `"variant_mismatch"` ou `"unavailable"`), vérifie la place (taille du pack plus taille déballée : `507 NO_SPACE`), puis range le pack AVANT la création du conteneur : `<données>/services/<clé>/<dossier des packs>/pack.zip` (ou `pack.mrpack`), monté en lecture seule ; la variable de la fiche pour ce genre reçoit le chemin dans le conteneur. L'identifiant du dépôt est signé avec l'acte (`deploy_digest`) ; rien n'est ajouté à la signature d'une installation sans pack.
