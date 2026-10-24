# Réglages d'un serveur de jeu (HRT-108)

Capacité `game-settings` annoncée par [`GET /hello`](./hello.md). Trois routes, toutes de niveau `Services` (le droit « gérer les services », refus consigné) ; elles ne servent qu'un service créé par Hearth depuis une fiche que CET agent propose ; tout autre service (clé inconnue, adopté, déployé à la main, fiche que cet agent ne propose pas) reçoit `404 NOT_A_GAME_SERVICE`, une réponse unique qui ne dit pas si la clé existe ; une clé mal formée reçoit `404 SERVICE_NOT_FOUND` ; un agent sans la capacité répond `404 NOT_FOUND` (route inconnue : une erreur pour le client, qui ne lit la vue que s'il lit la capacité). Le client lit la capacité `game-settings` du `/hello` pour montrer l'onglet « Réglages », puis ne lit la vue qu'à l'ouverture de l'onglet. Décisions : ADR-0051. Règles : BR-FICHE-037, 042, 044, 046 à 049, 052, 053, BR-JEU-035, 036.

Les libellés, aides, types et bornes d'un réglage ne passent pas ici : le client les lit dans la fiche ([`GET /catalog/{id}`](./catalog.md)). Ces routes ne portent que les VALEURS voulues et l'état du fichier. **Un secret n'y figure jamais**, ni sa valeur ni sa longueur.

## `GET /api/v1/services/{key}/settings`

Non journalisée en cas de succès (un refus faute de droit l'est). Réponse (`SettingsView`) :

| Champ | Contenu |
|---|---|
| `rev` | La révision à renvoyer en `base_rev` (0 tant que rien n'a été enregistré depuis l'installation) |
| `fiche` | `id`, `version`, `digest` de la fiche du service |
| `file_state` | `ok`, `missing` (le fichier n'existe pas), `unreadable` (le format ne sait pas le découper : rien n'y est retouché) |
| `file_stamp` | SHA-256 hexadécimal des fichiers de réglages tels que lus : à renvoyer en `file_stamp` (obligatoire) |
| `file_changed` | Les fichiers ne sont plus tels que Hearth les a écrits en dernier (BR-FICHE-048) |
| `raw_edit` | Le fichier brut peut être édité (faux : le jeu le réécrit à chaque démarrage, ou la fiche n'a aucun fichier de réglages) |
| `settings` | Un élément par réglage de la fiche : `id`, `state` (`written`, `unwritten`, `secret_set`), `value` (absente pour un secret et pour « non écrit »), `frozen`, `imposed`, `gone` (n'existe plus dans la version du jeu : gardé et grisé ; **toujours faux** tant que la version du jeu n'est pas lue, HRT-115), `effect` (`none`, `restart`, `recreate`, déduit du canal) |
| `drifts` | Les réglages dont le fichier ne dit plus ce que Hearth veut : `id`, `hearth`, `file`, `file_valid` (faux : la valeur du fichier n'est pas acceptée par le réglage, « Garder la valeur du fichier » sera refusé). Jamais un secret, jamais un réglage imposé |
| `others` | Les lignes que la fiche ne connaît pas : `key`, `value` (absente et `masked` quand le nom contient PASSWORD, PASS, SECRET, TOKEN ou KEY) |
| `imposed_broken` | Les réglages imposés que le fichier a défaits : signalés, jamais réécrits |

## `POST /api/v1/services/{key}/settings/analyze`

Une consultation (un `POST` de `READ_ONLY_POSTS` : ni acte, ni entrée de réussite). Corps : `{ "text": "…" }` (2 Mio au plus, un mot de passe possible : lu puis oublié). Réponse (`ImportView`) : `lines` (par réglage : `state` `take`, `same`, `rejected`, `imposed`, `frozen` ; `reason` d'un refus ; `pasted` sans les secrets ; `secret`), `unknown` (lignes de la fiche inconnues, gardées dans le fichier), `duplicates`, `imposed_undone`. `422 SETTINGS_FILE_UNREADABLE` si le texte n'est pas lisible par le format du jeu. Rien n'est écrit.

## `PUT /api/v1/services/{key}/settings`

Acte `GameSettingsSave` (`0x1E`), confirmé (`reauth`), couvert par l'élévation. Signés : la clé du service, `base_rev`, le nombre de changements, l'empreinte des IDENTIFIANTS changés, l'origine, l'effet. **Jamais une valeur.**

Corps (`SaveSettingsRequest`) :

```json
{
  "base_rev": 2, "file_stamp": "…", "origin": "form", "effect": "restart",
  "changes": [
    { "id": "motd", "action": "set", "value": "Bienvenue" },
    { "id": "seed", "action": "unset" },
    { "id": "max_players", "action": "reset" },
    { "id": "password", "action": "from_import" }
  ],
  "others": [ { "key": "mod_x", "value": "2" }, { "key": "mod_y", "remove": true } ],
  "drift_choices": [ { "id": "motd", "choice": "keep_file" } ],
  "import_text": "motd=…"
}
```

- `origin` : `form`, `import`, `reset`, `drift`. `action` : `set`, `unset` (« non écrit » : la ligne est retirée), `reset` (valeur proposée, ou ligne retirée si la fiche n'en propose pas ; refusé pour un secret), `from_import` (valeur relue dans `import_text` par l'agent). `choice` : `keep_file` (la description reprend la valeur du fichier), `keep_mine` (la valeur voulue est réécrite).
- `file_stamp` est OBLIGATOIRE : un corps qui ne l'envoie pas est refusé (`422 VALIDATION_ERROR`, rien n'est écrit). Les choix d'écart sont signés dans l'acte avec les identifiants (`file:<id>`, `mine:<id>`) : un acte signé sans eux ne couvre pas un `keep_mine`.
- Tous les changements sont jugés d'abord, tous ou aucun. Le fichier n'est retouché que sur les valeurs demandées : commentaire, ligne inconnue, ligne commentée gardent leurs octets (BR-FICHE-047). Rien n'est écrit si aucun octet ne change.
- Réponse `200` (`SaveSettingsResponse`) : `rev`, `effect` (le plus lourd), `file_written`, `changed`, `adopted`. L'agent ne redémarre rien : le client lance `POST /services/{key}/restart` s'il le veut (BR-JEU-035).

| Code | Cas |
|---|---|
| `409 SETTINGS_CONFLICT` | `base_rev` n'est plus la révision courante (`details.by`, `details.at`) ; ou l'effet signé n'est pas l'effet déduit (`details.reason = "effect"`) |
| `409 SETTINGS_FILE_CHANGED` | le fichier a bougé depuis la lecture (`file_stamp`) ou juste avant l'écriture, un autre enregistrement a gagné la course, ou des réglages en écart attendent un choix (`details.ids`) ; rien n'est remis par-dessus le fichier d'un autre |
| `422 SETTINGS_FILE_UNREADABLE` | le format ne sait pas découper le fichier (`details.reason`, `details.line`) ; jamais réécrit |
| `422 SETTING_REFUSED` | un changement est refusé (`details.id` seulement si c'est un réglage de la fiche, `details.reason` : `bounds`, `choice`, `charset`, `length`, `frozen`, `imposed`, `retired`, `unknown_id`, `variant`, `version`, `check`, `secret`, `carried_by_world`, `details.char` au plus un caractère) ; jamais la valeur |
| `409 RECREATE_NOT_AVAILABLE` | un réglage reçu par variable ou argument demande de recréer le service (`details.ids`) ; rien n'est écrit |
| `409 OPERATION_IN_PROGRESS` | une opération exclusive tient le service |
| `422 VALIDATION_ERROR` | corps incohérent (`details.field`) ; `details.field = "channel"` : la fiche de ce serveur n'a aucun fichier de réglages que Hearth sache lire (analyser ou enregistrer est impossible ; la vue, elle, est vide) |

Journal : `service.game.settings`, le NOMBRE de réglages changés, les noms de ceux qui ne sont pas des secrets, « mot de passe modifié » ; jamais une valeur (BR-FICHE-053).
