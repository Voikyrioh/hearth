# Comptes : `/accounts`, `/me/password`

Gestion des comptes. Code : `crates/hearth-agent/src/entrypoint/http/accounts.rs` (handlers), `application/accounts.rs` (cas d'usage), `domain/accounts/` (règles). Types du fil : `hearth-proto::api::accounts`.

Toutes ces routes exigent `X-Hearth-Api` et un jeton (`Authorization: Bearer …`). Sauf `PUT /me/password`, elles sont **réservées aux administrateurs** : un compte lecture seule reçoit `403 FORBIDDEN_ROLE` (BR-ACCT-013, BR-ACCT-014), avant même la lecture du corps. Le contrôle est fait par l'agent, par une couche unique (`auth::guard`, posée par le routeur d'après la table `ENDPOINTS`), jamais par le client.

Toutes les requêtes qui modifient acceptent `Idempotency-Key` : rejouer la clé rend le premier résultat sans ré-exécuter (BR-RESIL-010).

## `GET /api/v1/accounts` (admin)

`200` : `{ "accounts": [ { "id", "username", "role", "created_at", "last_login_at", "sessions_open" } ] }`, du plus ancien au plus récent. `last_login_at` est `null` tant que le compte ne s'est jamais connecté ; `sessions_open` compte les sessions dont l'expiration est dans le futur.

## `POST /api/v1/accounts` (admin)

Corps : `{ "username", "password", "role": "admin" | "readonly" }`. `201` : le compte créé (même forme qu'un élément de la liste).

| Statut | Code | Quand |
|---|---|---|
| 422 | `VALIDATION_ERROR` | Format d'identifiant invalide (`details.field = "username"`, BR-ACCT-002). |
| 422 | `WEAK_PASSWORD` | `details.rules` : toutes les règles non respectées, parmi `required`, `min_length`, `digit`, `lowercase`, `uppercase`, `contains_username` (BR-ACCT-004, 005). |
| 409 | `USERNAME_TAKEN` | Identifiant déjà pris, sans distinction de casse (BR-ACCT-003). |

## `PATCH /api/v1/accounts/{id}` (admin)

Corps : `{ "role": "admin" | "readonly" }`. `204`.

| Statut | Code | Quand |
|---|---|---|
| 409 | `LAST_ADMIN` | Rétrograder le dernier administrateur (BR-ACCT-007). |
| 404 | `NOT_FOUND` | Compte inconnu. |

## `DELETE /api/v1/accounts/{id}` (admin)

Corps facultatif : `{ "confirmation": "marie" }`. Qui supprime **son propre** compte doit retaper son identifiant (BR-ACCT-012). `200` : `{ "sessions_closed": n }` (les sessions du compte sont fermées, BR-ACCT-010).

| Statut | Code | Quand |
|---|---|---|
| 422 | `VALIDATION_ERROR` | `details.field = "confirmation"` : identifiant retapé absent ou différent. |
| 409 | `LAST_ADMIN` | Supprimer le dernier administrateur. |
| 404 | `NOT_FOUND` | Compte inconnu. |

## `PUT /api/v1/accounts/{id}/password` (admin)

Corps : `{ "password": "…" }`. `200` : `{ "sessions_closed": n }`. Toutes les sessions du compte sont fermées : son jeton reçoit `SESSION_REVOKED` (BR-ACCT-008). Erreurs : `WEAK_PASSWORD`, `NOT_FOUND`.

## `DELETE /api/v1/accounts/{id}/sessions` (admin)

Ferme toutes les sessions du compte sans toucher à son mot de passe (BR-ACCT-011). `200` : `{ "sessions_closed": n }`. Erreur : `NOT_FOUND`.

## `PUT /api/v1/me/password` (tout rôle)

Corps : `{ "current": "…", "password": "…" }`. `200` : `{ "sessions_closed": n }` : les **autres** sessions sont fermées, la session courante est gardée (BR-ACCT-009).

| Statut | Code | Quand |
|---|---|---|
| 422 | `WRONG_PASSWORD` | Le mot de passe actuel est incorrect. |
| 422 | `WEAK_PASSWORD` | Le nouveau mot de passe ne respecte pas les règles. |
| 409 | `CONFLICT` | Le mot de passe a changé entre la vérification et l'écriture : réessayer. |

Les routes qui changent un mot de passe ou créent un compte répondent aussi `503 BUSY` (avec `Retry-After`) quand l'agent est saturé de calculs de mots de passe.

## Journal

Les changements de compte seront consignés au journal d'activité avec HRT-05 (BR-ACCT-016) ; cette tâche n'écrit pas encore dans le journal.
