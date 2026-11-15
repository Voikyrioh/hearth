# Reprendre un monde existant (HRT-105)

Capacité `world-import` annoncée par [`GET /hello`](./hello.md). Deux consultations (la liste des mondes, l'inspection d'un monde) et un membre `source.world` de [`POST /deployments`](./services.md). ADR-0059, règles `BR-MONDE-002` à `016`, `023`, `024`, `026`, `039`.

Les deux consultations exigent le droit « gérer les services » (niveau `Services`) ; un refus faute de droit est consigné au journal d'activité (`service.world.import`). **Elles ne sont ni diffusées ni journalisées** (`BR-MONDE-003`) et n'écrivent rien. Aucune réponse ne contient un chemin absolu : un monde est désigné par le dossier ouvert tel qu'il est écrit dans la configuration de l'agent (`[services] allowed_roots` d'`agent.toml`) et par un chemin relatif à ce dossier.

## `GET /api/v1/catalog/{id}/worlds`

Les mondes que la fiche reconnaît dans chaque dossier ouvert aux services. `404 FICHE_UNKNOWN` si l'agent ne propose pas la fiche ; `422 VALIDATION_ERROR` (`details.reason = "not_supported"`) si la fiche ne déclare pas ce qu'est un monde (`BR-MONDE-039`).

```json
{
  "roots": [
    { "folder": "/srv/jeux", "readable": true, "truncated": false,
      "worlds": [
        { "path": "Worlds/Alpha.wld", "name": "Alpha", "files": 1, "bytes": 300000,
          "modified_at": "2026-11-14T09:00:00Z", "usable": true }
      ] }
  ],
  "max_depth": 3,
  "max_entries": 5000
}
```

- `roots` vide : le serveur n'a ouvert aucun dossier aux services (`BR-MONDE-002`).
- `readable: false` : le dossier n'a pas pu être ouvert (absent, droits). `truncated` : la recherche s'est arrêtée à 5 000 entrées ou à trois niveaux : des mondes peuvent manquer.
- `files` et `bytes` : connus d'un monde « fichier » (le fichier et ses compagnons) sans rien ouvrir ; absents d'un monde « dossier » (voir `inspect`).
- `usable: false` : le nom du monde est refusé par le réglage de la fiche (`BR-MONDE-014`) ; le monde est listé mais ne s'installe pas.
- Les éléments dont le nom évoque une sauvegarde (`backup`, `sauvegarde`, `.bak`), les liens et les fichiers spéciaux ne sont jamais des mondes.

## `POST /api/v1/catalog/{id}/worlds/inspect`

Corps : `{ "folder": "/srv/jeux", "path": "Worlds/Alpha.wld" }`. Une consultation : un `POST` qui ne modifie rien, sans `Idempotency-Key`, sans élévation.

```json
{ "files": 1, "bytes": 300000, "not_copied": 1, "not_copied_names": ["Alpha.wld.bak"],
  "skipped_links": 0, "skipped_link_names": [], "partial": false, "free_bytes": 53687091200 }
```

- `files`, `bytes` : ce que la copie écrirait. `not_copied` (+ 50 noms au plus) : les éléments dont le nom évoque une sauvegarde, ni lus ni copiés. `skipped_links` : liens et fichiers spéciaux, non copiés (`BR-MONDE-005`, `006`).
- `partial` : la mesure s'est arrêtée à 60 secondes, les chiffres sont « au moins ». `free_bytes` : l'espace libre là où la copie sera écrite.
- `422 VALIDATION_ERROR`, `details.field = "world"`, `details.reason` : `unknown_folder` (dossier non ouvert), `bad_path` (absolu, `..`, vide), `not_a_world`, `not_found`, `too_big`, `name_unusable`, `unreadable`.

## `source.world` de `POST /deployments`

```json
{ "source": { "type": "fiche", "id": "terraria", "version": 4, "digest": "...",
              "world_imported": true,
              "world": { "folder": "/srv/jeux", "path": "Worlds/Alpha.wld",
                         "version_understood": true, "keep_if_changed": false } } }
```

- `world_imported` et `world` vont ensemble (sinon `422`, `source` / `world_import_inconsistent` ou `world_import_unavailable`).
- `version_understood` : le geste « Je comprends » (`BR-MONDE-016`) ; l'agent refuse sans (`source.world` / `version_not_understood`), même d'un client modifié.
- `keep_if_changed` : « Garder cette copie » d'avance si des fichiers changent pendant la copie (`BR-MONDE-009`) ; sinon la copie est retirée et rien n'est créé.
- Le nom du monde devient la réponse du réglage de nom de la fiche ; une réponse différente du client est refusée (`source.answers` / `world_name_fixed`, `BR-MONDE-014`). La signature de l'acte couvre le dossier, le chemin et les deux gestes.
- Refus synchrones en plus de ceux de l'installation : `source.world` + `unavailable` (agent sans reprise), `unknown_folder`, `bad_path`, `not_a_world`, `not_found`, `too_big`, `name_unusable`, `not_supported` (monde « dossier » : listé et inspecté, pas encore installé) ; `507 NO_SPACE` si la taille du monde plus le besoin de la fiche dépasse l'espace libre (`BR-MONDE-012`, sans case pour passer outre) ; `409 HEAVY_IO_BUSY` si une sauvegarde, une restauration ou une autre reprise tient la place (`BR-MONDE-013`).
- L'opération `deploy` gagne l'étape `world` entre `pull` et `create` : progression en fichiers et octets (`items_done`, `bytes_done`, `bytes_total`), annulable jusqu'à `create` (`DELETE /operations/services/{id}` : seul le dossier d'import part, la source n'est pas touchée).
- Journal d'activité (`service.deploy`) : « monde existant repris depuis le dossier ouvert {dossier} » avec nombre de fichiers et d'octets, éléments non copiés, issue ; jamais le sous-chemin ni un nom (`BR-MONDE-023`).
