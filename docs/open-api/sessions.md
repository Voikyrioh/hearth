# Sessions : `/sessions`, `/me`

Connexion, déconnexion et compte courant. Code : `crates/hearth-agent/src/entrypoint/http/sessions.rs` (handlers), `application/sessions.rs` (cas d'usage), `domain/{lockout,sessions,session_token}.rs` (règles). Types du fil : `hearth-proto::api::sessions`.

Toutes ces routes exigent l'en-tête `X-Hearth-Api: <n>` (BR-CONN-014) ; voir « Conventions » dans l'[index](./INDEX.md).

## `POST /api/v1/sessions` : se connecter

- **Authentification** : aucune. **Rôle** : aucun.
- **Suivi par clé** : non (la réponse contient un jeton, qu'on ne conserve pas en base).
- **En-têtes** : `X-Hearth-Api`, `X-Hearth-Client: poste/version` (nom du poste, 128 caractères au plus, `inconnu` si absent).
- **Corps** : `{ "username": "marie", "password": "…" }` (l'identifiant est insensible à la casse).

### Réponse `201`

```json
{
  "token": "9f2c…64 caractères hexadécimaux…",
  "expires_at": "2026-11-03T10:30:15.25Z",
  "account": { "id": "01J9ZY0G3Q8M2K6W4T7V5N1B9D", "username": "marie", "role": "admin" }
}
```

- `token` : 32 octets aléatoires du système, en hexadécimal minuscule. Rendu une seule fois. L'agent n'en garde que le SHA-256 (BR-RESIL-012). À envoyer en `Authorization: Bearer <token>`.
- `expires_at` : fin de la session si elle reste inactive ; l'activité la repousse (30 jours glissants).
- `account.role` : `admin` ou `readonly`.

### Erreurs

| Statut | Code | Quand |
|---|---|---|
| 401 | `INVALID_CREDENTIALS` | Identifiant inconnu **ou** mot de passe faux, sans distinction (BR-CONN-013). |
| 429 | `TOO_MANY_ATTEMPTS` | 5 échecs pour ce couple identifiant + adresse ; `details.retry_after_s` (BR-CONN-006). Même le bon mot de passe est refusé pendant l'attente. |
| 422 | `VALIDATION_ERROR` | Corps illisible ou champ manquant, ou `X-Hearth-Api` absent. |
| 426 | `INCOMPATIBLE_VERSION` | Version d'interface hors plage ; `details.upgrade` : `client` ou `agent` (BR-CONN-014). |

## `DELETE /api/v1/sessions/current` : se déconnecter

- **Authentification** : jeton. **Rôle** : tous.
- **Réponse** : `204`, sans corps. La session est supprimée (le jeton ne vaut plus rien : `SESSION_EXPIRED`).
- **Erreurs** : celles de l'authentification (voir plus bas).

## `GET /api/v1/me` : le compte courant

- **Authentification** : jeton. **Rôle** : tous.

### Réponse `200`

```json
{
  "account": { "id": "01J9…", "username": "marie", "role": "admin" },
  "session_expires_at": "2026-11-03T10:30:15.25Z"
}
```

## Erreurs d'authentification (toutes les routes qui exigent un jeton)

| Statut | Code | Quand |
|---|---|---|
| 401 | `UNAUTHENTICATED` | Pas d'en-tête `Authorization: Bearer …`, ou jeton illisible (pas 64 chiffres hexadécimaux). |
| 401 | `SESSION_EXPIRED` | 30 jours sans activité, ou jeton inconnu (session purgée). Le client se reconnecte en silence s'il a le mot de passe (BR-RESIL-012, 013). |
| 401 | `SESSION_REVOKED` | Session fermée par un changement de mot de passe, une suppression de compte ou une révocation : « Accès révoqué » (BR-RESIL-014). |
| 403 | `FORBIDDEN_ROLE` | Rôle insuffisant (routes réservées aux administrateurs). |

## Règles

- BR-CONN-006, BR-CONN-007, BR-CONN-013, BR-CONN-014, BR-RESIL-012, BR-RESIL-014.
- L'adresse du client est celle de la connexion TCP ; `X-Forwarded-For` et `Forwarded` sont ignorés.
- Jamais de jeton, de mot de passe ni de haché dans un journal ou un message d'erreur.
