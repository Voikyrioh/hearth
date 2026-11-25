# Console d'un serveur de jeu et joueurs connectés (HRT-268, ADR-0067)

L'agent parle à la console d'un serveur de jeu installé par Hearth quand sa fiche déclare `[remote_console.use]` : client RCON de l'agent vers l'adresse IPv4 PRIVÉE du conteneur, au port de la fiche, une connexion par échange. Le texte d'une commande et la réponse du jeu ne sont ni gardés, ni tracés, ni journalisés. Règles `BR-CONSOLE-001`, `003` à `009`, `011` ; capacité `game-console` de `/hello`.

## `GET /api/v1/services/{key}/console`

Accès : tout compte authentifié, sans journal. Rien n'est envoyé au jeu.

```json
{
  "available": true,
  "unavailable_reason": "adopted | no_console | protocol_unsupported",
  "state": "ready | starting | stopped | busy",
  "can_write": true,
  "policy": {
    "refused": [{ "verb": "stop", "reason": "use_hearth_stop" }],
    "sensitive": [{ "verb": "op", "title": "Donner les droits d'opérateur à {joueur} ?", "sentence": "...", "target": "first_arg" }]
  },
  "quick": [{ "id": "save", "label": "Enregistrer le monde", "text": "save-all", "mode": "send | fill" }],
  "limits": { "max_chars": 256, "per_minute": 20, "reply_timeout_s": 10 }
}
```

`unavailable_reason` n'est présent que si `available` est faux (alors `policy`, `quick` et `limits` sont vides ou absents). `can_write` : le compte a le droit de gérer les services. Erreurs : `404 SERVICE_NOT_FOUND`.

## `POST /api/v1/services/{key}/console`

Accès : droit « gérer les services » (niveau `Services`), acte `ConsoleCommand` (`0x30`) : clé du service, SHA-256 de la ligne normalisée, classe. Mot de passe : commande sensible toujours ; libre : grille du pilotage (BR-CONSOLE-005). Corps : `{ "command": "op Bram", "class": 2 }` (`class` : `1` libre, `2` sensible, celle que le client a jugée et signée) plus le membre `reauth`.

Réponse `200` : `{ "outcome": "answered | empty | no_answer", "reply": "Made Bram a server operator", "truncated": false }` (`reply` absent pour `empty` et `no_answer` ; `no_answer` : la commande a peut-être été exécutée, elle n'est jamais renvoyée).

Classe : un verbe à espace de noms inconnu (`mod:commande`, `minecraft:` retiré) est SENSIBLE par défaut ; `function`, `schedule` et `datapack` sont sensibles dans la fiche Minecraft (ADR-0067, points 11 et 12). Phase du jeu inconnue (agent redémarré) : la commande part, `CONSOLE_UNREACHABLE` est possible au début du démarrage.

Refus, dans cet ordre, rien n'est envoyé au jeu :

| HTTP | Code | Condition |
|---|---|---|
| 404 | `SERVICE_NOT_FOUND` | service inconnu |
| 409 | `SERVICE_ADOPTED` | service adopté |
| 409 | `CONSOLE_NOT_AVAILABLE` | fiche sans `[remote_console.use]` |
| 422 | `CONSOLE_COMMAND_INVALID` (`details.reason` : `empty`, `too_long`, `control_char`) | forme |
| 422 | `CONSOLE_COMMAND_REFUSED` (`details.reason` : `use_hearth_stop`, `use_hearth_restart`, `breaks_server`) | verbe refusé (inscrit ; un refus prend un jeton du débit AVANT d'être inscrit, `429` au-delà de la rafale) |
| 422 | `CONSOLE_CLASS_MISMATCH` | classe signée différente de la classe rejugée |
| 409 | `SERVICE_NOT_RUNNING` / `SERVICE_STARTING` | arrêté / pas encore prêt |
| 409 | `OPERATION_IN_PROGRESS` | verrou exclusif tenu |
| 429 | `CONSOLE_RATE_LIMITED` (`details.retry_after_s`) | débit |
| 403 / 409 | `FORBIDDEN_ROLE` (`details.missing`) / `POST_NOT_RECOGNIZED` (`details.reason`) | droit absent / confirmation ou mot de passe manquant |
| 502 | `CONSOLE_UNREACHABLE` | pas d'adresse privée, connexion ou authentification refusée (inscrit) |

Journal : `service.console` (BR-CONSOLE-009).

## `GET /api/v1/services/{key}/players`

Accès : tout compte authentifié, aucun journal. Cache de 10 secondes par service (une seule commande `list` pour tous les écrans).

```json
{ "state": "ok | stopped | starting | unavailable | no_console",
  "online": 3, "max": 20, "names": ["Alice_42", "Bram"], "hidden_names": 1, "more_names": 0,
  "read_at": "2026-11-18T20:45:12Z" }
```

`online`, `max`, `names` et `read_at` n'existent que pour `ok`. Une réponse illisible est `unavailable`, jamais un chiffre deviné. Pendant une opération exclusive : la dernière lecture, ou `unavailable`. Erreurs : `404 SERVICE_NOT_FOUND`.

## `GET /api/v1/operations/services/{id}/tail` (HRT-259)

Rend maintenant aussi la queue d'une opération `mods` revenue en arrière : `{ "lines": [...20 au plus], "reason": "missing_dependency", "sentence": "..." }` ; `reason` (`duplicate_mod`, `missing_dependency`, `incompatible`, `out_of_memory`) et `sentence` (phrase en clair) sont absents quand l'agent ne reconnaît pas la cause. Gardée 24 heures en mémoire.
