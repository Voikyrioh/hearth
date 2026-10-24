# Catalogue des fiches de serveurs de jeu (HRT-101)

Capacité `catalog` annoncée par [`GET /hello`](./hello.md). Les fiches sont embarquées dans le binaire de l'agent (ADR-0050) ; ces deux routes les rendent en lecture seule. Aucune écriture, aucun secret : une fiche n'en contient aucun, et les réponses d'un utilisateur ne passent jamais par ici.

Installer un serveur depuis une fiche : [`POST /deployments`](./services.md) avec un membre `source`.

## `GET /api/v1/catalog`

Tout compte authentifié (niveau `Authenticated`). Non journalisée.

```json
{
  "fiches": [
    { "id": "terraria", "name": "Terraria", "summary": "Le serveur officiel de Terraria, sans mod.",
      "version": 1, "status": "preview", "installable": true }
  ],
  "refused": [ { "dir": "autre-jeu", "invariant": "I2" } ]
}
```

- `fiches` : les fiches que CET agent propose. Un agent de publication ne propose que les fiches `published` ; une construction d'essai (variable de construction `HEARTH_PREVIEW_FICHES`, refusée par `cargo xtask agent --release`) propose aussi les `preview`. Au jour de HRT-101, aucune fiche n'est `published` : la liste d'un agent publié est vide.
- `refused` : les fiches embarquées que le chargeur a écartées (dossier et invariant violé, jamais un contenu). Chacune est consignée une fois au démarrage dans le journal d'activité (`catalog.fiche.refused`, auteur : le système).
- `status` : `published`, `preview` ou `unknown` (valeur d'une version plus récente). `installable: false` : la fiche est listée mais ne s'installe pas sur ce serveur.
- Un élément illisible d'une version plus récente est écarté et compté par le client (`Items::unreadable`).

## `GET /api/v1/catalog/{id}`

Tout compte authentifié. `404 FICHE_UNKNOWN` si l'agent ne propose pas cette fiche (inconnue, ou `preview` sur un agent publié : même réponse).

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

## Règles

BR-FICHE-001 à 004, 006, 007, 008, 010, 016, 056. Le client (HRT-102) lit ces deux routes par `read_catalog` et `read_fiche` de la coquille ; une fiche inconnue de l'agent se lit « inconnue », jamais comme une panne.
