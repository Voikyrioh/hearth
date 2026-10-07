# Comptes : `/accounts`, `/me/password`

Gestion des comptes. Code : `crates/hearth-agent/src/entrypoint/http/accounts.rs` (handlers), `application/accounts.rs` (cas d'usage), `domain/accounts/` (règles). Types du fil : `hearth-proto::api::accounts`.

Toutes ces routes exigent `X-Hearth-Api` et un jeton (`Authorization: Bearer …`). Sauf `PUT /me/password`, elles sont **réservées aux administrateurs** : un compte lecture seule reçoit `403 FORBIDDEN_ROLE` (BR-ACCT-013, BR-ACCT-014), avant même la lecture du corps. Le contrôle est fait par l'agent, par une couche unique (`auth::guard`, posée par le routeur d'après la table `ENDPOINTS`), jamais par le client.

**Confirmation (HRT-28, ADR-0031).** Chaque route qui modifie (création, rôle, mot de passe d'un compte, suppression, fermeture des sessions, `PUT /me/password`) accepte un membre `reauth` (mot de passe + preuve de clé d'usage `0x05` liée à l'acte) : voir [sécurité](./security.md), « Confirmation des actes d'administration ». L'agent l'accepte sans encore l'exiger. **`PUT /me/password`** : avec `reauth`, `reauth.password` est l'ancien mot de passe (il doit être égal à `current`, sinon `422 VALIDATION_ERROR`, `details.field = "current"`) ; **sans `reauth` aussi, l'ancien mot de passe passe par les compteurs de la connexion** (`422 WRONG_PASSWORD` compté comme un échec de connexion, `429 TOO_MANY_ATTEMPTS` à partir du cinquième : BR-TRUST-040).

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

Corps : `{ "current": "…", "password": "…", "keep_address": false }`. `keep_address` est **facultatif** (Q15, BR-CONN-019) : absent ou faux, toutes les adresses retenues du compte sans clé sont oubliées, celle d'où part la requête comprise ; vrai, l'adresse d'où part la requête (la connexion TCP, jamais une valeur du corps) est gardée si elle est retenue, les autres oubliées. Les adresses liées à un poste à clé restent dans les deux cas (BR-TRUST-023). `200` : `{ "sessions_closed": n }` : les **autres** sessions sont fermées, la session courante est gardée (BR-ACCT-009).

| Statut | Code | Quand |
|---|---|---|
| 422 | `WRONG_PASSWORD` | Le mot de passe actuel est incorrect. |
| 422 | `WEAK_PASSWORD` | Le nouveau mot de passe ne respecte pas les règles. |
| 409 | `CONFLICT` | Le mot de passe a changé entre la vérification et l'écriture : réessayer. |

Les routes qui changent un mot de passe ou créent un compte répondent aussi `503 BUSY` (avec `Retry-After`) quand l'agent est saturé de calculs de mots de passe.

## Journal

Chaque changement de compte réussi est consigné au journal d'activité, dans la transaction de l'action (BR-ACCT-016) ; un refus faute de droits et un échec de la requête le sont aussi (BR-AUDIT-003). Voir [journal](./audit.md).

## Côté client (HRT-13)

Le client Windows n'écrit jamais ces routes depuis la WebView : chaque action a SA commande Tauri typée (ADR-0016), la méthode et le chemin sont construits côté Rust (`apps/desktop/src-tauri/src/accounts/wire.rs`).

| Commande | Route | Clé d'opération |
|---|---|---|
| `list_accounts(server_id)` | `GET /accounts` puis `GET /me` (l'identifiant de l'agent du compte de la session, rendu avec la liste) | non (lectures typées, `LinkManager::accounts_list`, sans suivi) |
| `create_account(server_id, username, password, role)` | `POST /accounts` | oui |
| `change_account_role(server_id, account_id, role)` | `PATCH /accounts/{id}` | oui |
| `set_account_password(server_id, account_id, password)` | `PUT /accounts/{id}/password` | oui |
| `change_own_password(server_id, current, password)` | `PUT /me/password` | oui |
| `close_account_sessions(server_id, account_id)` | `DELETE /accounts/{id}/sessions` | oui |
| `delete_account(server_id, account_id, confirmation)` | `DELETE /accounts/{id}` | oui |
| `check_account_input(username, password)` | aucune (règles de `hearth-proto`, sans réseau) | — |

Un identifiant de compte (`account_id`) ne peut contenir que des lettres et des chiffres (jamais `/`, `..`, `?`). Les refus de l'agent sont rendus par code stable (`AccountRefusal`), jamais par texte. Une action coupée avant sa réponse rend `unknown` (jamais rejouée) ; l'issue arrive par `link://operation` et la liste se relit au retour du lien.
