# Catalogue des fiches de serveurs de jeu (HRT-101)

Capacité `catalog` annoncée par [`GET /hello`](./hello.md). Les fiches sont embarquées dans le binaire de l'agent (ADR-0050) ; ces deux routes les rendent en lecture seule. Aucune écriture, aucun secret : une fiche n'en contient aucun, et les réponses d'un utilisateur ne passent jamais par ici.

Installer un serveur depuis une fiche : [`POST /deployments`](./services.md) avec un membre `source`.

## `GET /api/v1/catalog`

Tout compte authentifié (niveau `Authenticated`). Non journalisée.

```json
{
  "fiches": [
    { "id": "terraria", "name": "Terraria", "summary": "Le serveur officiel de Terraria, sans mod.",
      "version": 5, "status": "published", "installable": true,
      "memory": { "recommended_bytes": 2147483648, "evidence": "measured" } },
    { "id": "factorio", "name": "Factorio", "summary": "Le serveur officiel de Factorio, sans mod.",
      "version": 1, "status": "preview", "installable": false,
      "preview_reason": "Le nom de la partie, son mot de passe et le nombre de joueurs ne se règlent pas encore depuis Hearth." }
  ],
  "refused": [ { "dir": "autre-jeu", "invariant": "I2" } ]
}
```

- `fiches` : les fiches que CET agent LISTE (HRT-236, BR-FICHE-002) : les `published` et les `preview` (en cours de preuve). Un agent de publication ne PROPOSE (détail, installation) que les fiches `published` : ses fiches `preview` sont listées avec `installable: false` et leur raison. Une construction d'essai (variable de construction `HEARTH_PREVIEW_FICHES`, refusée par `cargo xtask agent --release`) propose aussi les `preview` (`installable` suit alors la plateforme).
- `memory` (HRT-237, BR-FICHE-064) : la mémoire recommandée de la fiche et sa preuve (`evidence`, même forme que dans le détail) ; absent quand la fiche n'a pas de mémoire recommandée. Membre facultatif : un client ancien l'ignore.
- `steam_app_id` (HRT-238, BR-FICHE-062) : l'identifiant du JEU dans le magasin Steam ; absent d'un jeu hors Steam. `illustration` : `{ digest: "sha256:<64 hex>", bytes, width: 920, height: 430, media_type: "image/jpeg" }` pour un jeu absent de Steam (l'illustration embarquée, servie par la route ci-dessous) ; jamais les deux. Membres facultatifs : un client ancien les ignore.
- `preview_reason` : présent seulement pour une fiche `preview` (BR-FICHE-061) : ce qui la retient, en une ou deux phrases pour le joueur. Membre facultatif : un client ancien l'ignore.
- `refused` : les fiches embarquées que le chargeur a écartées (dossier et invariant violé, jamais un contenu). Chacune est consignée une fois au démarrage dans le journal d'activité (`catalog.fiche.refused`, auteur : le système).
- `status` : `published`, `preview` ou `unknown` (valeur d'une version plus récente). `installable: false` : la fiche est listée mais ne s'installe pas sur ce serveur (plateforme absente, ou fiche en cours de preuve d'un agent de publication).
- Un élément illisible d'une version plus récente est écarté et compté par le client (`Items::unreadable`).

## `GET /api/v1/catalog/{id}`

Tout compte authentifié. `404 FICHE_UNKNOWN` si l'agent ne propose pas cette fiche (inconnue, ou `preview` listée par un agent publié : même réponse).

Réponse (`FicheDetail`) :

| Champ | Contenu |
|---|---|
| `card` | La carte de la liste |
| `digest` | L'empreinte de la fiche (SHA-256 hexadécimal, 64 caractères) : à reprendre dans `source.digest` de `POST /deployments` (sinon `409 FICHE_CHANGED`) |
| `image` | `repository`, `tag`, `digest` (l'empreinte de l'image, absente tant qu'elle n'est pas relevée) |
| `owner_uid`, `owner_gid` | Le propriétaire numérique des dossiers du service |
| `variants` | `id`, `label`, `help`, `default` |
| `ports` | `id`, `container`, `protocol` (`tcp`, `udp`), `role`, `publish` (`always`, `optional`, `never`), `default_host`, `mapping` (`nat`, `same_number` : le jeu annonce son port, service et serveur portent le même nombre ; une valeur plus récente se lit `unknown`), `only_variants` (vide : toutes) |
| `folders` | `id`, `role`, `target` (chemin DANS le service), `backup`, `only_variants`. Jamais un chemin du serveur |
| `agreements` | `id`, `title`, `url`, `sentence` : les contrats à accepter (BR-FICHE-058) |
| `questions` | Les réglages essentiels non imposés, huit au plus : `id`, `label`, `help`, `secret`, `proposed` (jamais pour un secret) et, depuis HRT-102, tout ce qu'il faut pour les poser sans rien inventer : `kind` (`text`, `secret`, `int`, `bool`, `choice`, `id_list`, `opaque` ; un genre plus récent se lit `unknown`), `choices` (`value`, `label` ; pour `bool`, les deux valeurs du jeu, activé puis désactivé), `min`, `max` (entier), `min_len`, `max_len`, `charset` (`line`, `file_stem`, `arg_safe` ; une valeur plus récente se lit `unknown`), `frozen_after_creation`, `variants` (vide : toutes), `secret_kind` (`game_password`, `admin_password`, `server_token` ; `unknown` si plus récent) et `obtain_url` |
| `resources` | `cpu`, `memory`, `disk`, chacun avec son minimum, sa recommandation et sa **preuve** (`evidence` : `measured`, `measured_idle`, `{ "publisher": { "url": … } }`, `unmeasured`) : aucun chiffre sans preuve (BR-FICHE-008) |
| `first_start` | Le premier démarrage (HRT-102, BR-FICHE-010, 056) : `ready_timeout_s`, `world_generation`, `downloads` (`what`, `source`, `bytes`, `evidence`), `stages` (les libellés que Hearth reconnaît dans les journaux). Absent d'un agent plus ancien |
| `stop` | La façon d'arrêter le jeu (HRT-102, BR-JEU-002) : `grace_s`, `world_saved` (`by_image_on_signal`, `by_sequence`, `not_saved` ; une valeur plus récente se lit `unknown`), `sequence_tested`. Absent d'un agent plus ancien |
| `imposed` | Les réglages que la fiche pose à la place de l'utilisateur (HRT-102, BR-JEU-039) : `label`, `value` (absente quand elle est tirée, calculée ou secrète), `why` |
| `world_import` | Ce que la reprise d'un monde remplit et ne touche pas (HRT-106, BR-MONDE-014) : `names_setting` (le réglage que l'agent remplit avec le nom du monde repris) et `carried_by_world` (les réglages que le monde décide). Absent d'une fiche qui ne déclare pas ce qu'est un monde et d'un agent plus ancien : le client ne propose alors pas la reprise |
| `themes` | Les thèmes en mots simples (HRT-109, BR-FICHE-041), dans l'ordre de la fiche : `id`, `label`, `order`. Vide d'un agent plus ancien |
| `settings` | Le catalogue COMPLET de réglages de la fiche, imposés compris (HRT-109, BR-FICHE-037). Un élément : `question` (le contrôle, de la même forme qu'une entrée de `questions` : `id`, `label`, `help`, `kind`, `choices`, bornes, `proposed`, `frozen_after_creation`, `secret`…), `key` (le nom technique du jeu), `theme`, `level` (`essential`, `common`, `advanced` ; une valeur plus récente se lit `unknown`), `game_default` (absent : « non vérifié », jamais pour un secret), `imposed_why`, `since`, `until`, `replaced_by` (versions du jeu) et `list` (`item_charset`, `separator`, `max_items` d'une liste d'identifiants). Les valeurs voulues et l'état du fichier ne passent pas ici : [`game-settings`](./game-settings.md). Vide d'un agent plus ancien |

## Règles

BR-FICHE-001 à 004, 006, 007, 008, 010, 016, 037, 041, 056, 061, 062, 064. Le client (HRT-102) lit ces deux routes par `read_catalog` et `read_fiche` de la coquille ; une fiche inconnue de l'agent se lit « inconnue », jamais comme une panne.

## `GET /api/v1/catalog/{id}/illustration`

L'illustration embarquée d'une fiche LISTÉE (publiée ou en cours de preuve), HRT-238, ADR-0061, BR-FICHE-062. Tout compte authentifié (`Authenticated`), sans journal, sans corps de requête. L'identifiant est validé comme identifiant stable avant toute recherche (jamais répété).

- `200` : corps binaire `image/jpeg` (307 200 octets au plus), `Content-Length`, `Cache-Control: private, max-age=31536000, immutable`, `ETag: "sha256:<64 hex>"`, `X-Content-Type-Options: nosniff`, `Content-Security-Policy: default-src 'none'`. `If-None-Match` égal à l'`ETag` : `304` sans corps.
- `404 FICHE_UNKNOWN` : identifiant inconnu ou non stable, ou fiche écartée. `404 ILLUSTRATION_ABSENT` : fiche listée sans illustration (dont toute fiche Steam). `401`, `426` : garde commune.
- Aucune lecture de fichier à l'exécution : les octets sont ceux du binaire (`build.rs`, `include_bytes!`) ; l'agent ne contacte jamais Steam.

## Voies de mods (HRT-124)

`GET /catalog/{id}` porte un membre `mods` quand la fiche déclare des voies de mods (`ways`, genres de pack et variantes permises, indication du dossier « Tes mods »). Absent : aucune voie (BR-MOD-014). Détail et lecture d'un pack : [game-packs](./game-packs.md). Le rôle de dossier `packs` (lecture seule, hors sauvegarde) s'ajoute aux rôles de `folders`.
