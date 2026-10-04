# Opérations : `Idempotency-Key`, `GET /operations/{id}`

Suivi par clé des requêtes qui modifient, pour qu'un client qui perd le lien avant la réponse sache ensuite ce qui s'est passé (BR-RESIL-010). Code : `crates/hearth-agent/src/entrypoint/http/operations.rs` (couche et route), `application/operations.rs` (cas d'usage), `domain/operations.rs` (règle). Types du fil : `hearth-proto::api::operations`.

## En-tête `Idempotency-Key: <clé>`

- À mettre sur toute requête qui modifie faite par un compte connecté (`POST`, `PUT`, `PATCH`, `DELETE`). Absente : la requête s'exécute sans suivi.
- Forme : 1 à 64 caractères, lettres, chiffres, tiret, souligné (un ULID en pratique). Sinon `422 VALIDATION_ERROR` (`details.field = "idempotency-key"`).
- La clé est celle d'un compte (deux comptes peuvent choisir la même) et **liée à la requête** : méthode, chemin et SHA-256 du corps. Elle est enregistrée **avant** l'exécution, puis le résultat (statut et corps de la réponse) :
  - clé inconnue : la requête s'exécute ;
  - même clé, même requête, terminée : le premier résultat est rendu tel quel (même statut, même corps), sans ré-exécuter, avec l'en-tête `Idempotent-Replayed: true` ;
  - clé reçue dont l'exécution n'est pas finie : `409 OPERATION_IN_PROGRESS` ;
  - même clé, autre requête (autre méthode, chemin ou corps) : `422 IDEMPOTENCY_KEY_REUSED`, sans exécuter ;
  - exécution interrompue par un arrêt de l'agent : `409 CONFLICT` (résultat inconnu : vérifier l'état avant de relancer).
- La requête s'exécute dans une tâche détachée : si le client coupe la connexion avant la réponse, le résultat est tout de même retenu et relisible. Au démarrage de l'agent, une opération restée « en cours » passe à `interrupted`.
- Un résultat `5xx` n'est pas retenu (la clé est oubliée : le client peut relancer). Les résultats `4xx` le sont (rejouer un refus donne le même refus).
- `POST /sessions` n'est pas suivi : sa réponse contient un jeton, qui n'est jamais conservé en base.
- Les opérations sont conservées 24 heures, puis purgées. Seules les routes marquées « suivie » dans `ENDPOINTS` (authentifiées, qui modifient) suivent la clé ; ailleurs l'en-tête est ignoré.

## `GET /api/v1/operations/{id}`

- **Authentification** : jeton. **Rôle** : tous. Lecture seule, sans effet de bord.
- `{id}` est la clé d'opération envoyée par le client. Les clés sont propres à chaque compte : celle d'un autre compte répond `404`.

### Réponse `200`

```json
{
  "id": "01J9ZY0G3Q8M2K6W4T7V5N1B9D",
  "kind": "PUT /me/password",
  "status": "succeeded",
  "result": { "sessions_closed": 1 }
}
```

| Champ | Description |
|---|---|
| `status` | `running` (en cours), `succeeded` (réponse 2xx), `failed` (réponse 4xx), `interrupted` (l'agent s'est arrêté pendant l'exécution : résultat inconnu). |
| `kind` | `MÉTHODE /chemin` de la requête d'origine. |
| `result` | Corps de la réponse de l'opération terminée ; `null` tant qu'elle est en cours ou si la réponse n'avait pas de corps. |

### Erreurs

- `404 NOT_FOUND` : l'agent n'a jamais reçu cette clé (l'action n'a pas été exécutée : « Non exécuté, tu peux relancer »).
- Erreurs d'authentification : voir [sessions](./sessions.md).
