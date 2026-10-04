# API — Hearth

Endpoints `/api/v1` (JSON, HTTPS). Authentification par Bearer token en en-tête `Authorization: Bearer …`.

23 endpoints : session, comptes, mesures, audit, mises à jour.

| Groupe | Méthode | Route | Rôle | Description |
|---|---|---|---|---|
| **Session** | GET | [`/hello`](./hello.md) | — | Identité de l'agent, plage de versions (l'empreinte se lit sur le certificat TLS) |
| **Session** | POST | [`/sessions`](./sessions.md) | — | Login (username + password) → token |
| **Session** | DELETE | [`/sessions/current`](./sessions.md) | user+ | Logout |
| **Session** | GET | [`/me`](./sessions.md) | user+ | Compte courant + rôle |
| **Session** | GET | [`/operations/{id}`](./operations.md) | user+ | Statut opération idempotente |
| **Mesures** | GET | `/machine` | user+ | Identité machine, specs hardware |
| **Mesures** | GET | `/stream` | user+ | WebSocket flux temps réel (snapshot + metrics chaque seconde) |
| **Mesures** | GET | `/metrics/history?window=1m\|5m\|1h` | user+ | Historique rééchantillonné |
| **Comptes** | GET | [`/accounts`](./accounts.md) | admin | Liste comptes, sessions ouvertes, dernier login |
| **Comptes** | POST | [`/accounts`](./accounts.md) | admin | Créer compte (username, password, role) |
| **Comptes** | PATCH | [`/accounts/{id}`](./accounts.md) | admin | Modifier rôle |
| **Comptes** | PUT | [`/accounts/{id}/password`](./accounts.md) | admin | Changer mot de passe (ferme sessions compte) |
| **Comptes** | PUT | [`/me/password`](./accounts.md) | user+ | Changer son mot de passe (ferme autres sessions) |
| **Comptes** | DELETE | [`/accounts/{id}`](./accounts.md) | admin | Supprimer compte |
| **Comptes** | DELETE | [`/accounts/{id}/sessions`](./accounts.md) | admin | Fermer toutes les sessions du compte |
| **Audit** | GET | `/audit?filters…` | admin | Journal d'activité (pagination curseur) |
| **Audit** | GET | `/audit/export?filters…` | admin | Journal CSV (UTF-8 BOM, `;` séparateur) |
| **Mise à jour** | GET | `/agent/update` | user+ | Statut mise à jour (courant, en cours, dernier résultat) |
| **Mise à jour** | POST | `/agent/update` | admin | Lancer mise à jour (URL, signature minisign) |
| **CLI** | `hearth-agent install` | — | — | Installation interactive (env var override) |
| **CLI** | `hearth-agent uninstall` | — | — | Désinstallation |
| **CLI** | `hearth-agent account` | — | — | Sous-cmds : add, list, passwd, role, remove, revoke |
| **CLI** | `hearth-agent fingerprint` | — | — | Affiche empreinte certificat |

## Codes d'erreur transverses

- `401 UNAUTHENTICATED` : pas de token, ou token illisible
- `401 INVALID_CREDENTIALS` : identifiant ou mot de passe refusé, sans préciser lequel
- `401 SESSION_EXPIRED` : token expiré
- `401 SESSION_REVOKED` : session révoquée
- `403 FORBIDDEN_ROLE` : rôle insuffisant
- `409 OPERATION_IN_PROGRESS` : opération en cours (ex. mise à jour)
- `422 VALIDATION_ERROR` : paramètre invalide (détails champ)
- `426 INCOMPATIBLE_VERSION` : client/agent incompatibles
- `429 TOO_MANY_ATTEMPTS` : trop de tentatives (login verrouillé), `details.retry_after_s`
- `409 USERNAME_TAKEN`, `409 LAST_ADMIN`, `409 CONFLICT` ; `422 WEAK_PASSWORD` (`details.rules`), `422 WRONG_PASSWORD` : voir [comptes](./accounts.md)
- `500 INTERNAL_ERROR` : erreur serveur

## Conventions

- Idempotency : requêtes modifiantes portent `Idempotency-Key: <ULID>` (rejouer = même résultat, sans ré-exécuter ; en cours = `409`) : voir [opérations](./operations.md). `POST /sessions` n'est pas suivi (son résultat contient un jeton).
- Versioning : client envoie `X-Hearth-Api: <n>` (obligatoire sauf sur `/hello`) ; hors plage `426 INCOMPATIBLE_VERSION` avec `details.upgrade` ; l'agent répond `X-Hearth-Api-Range: min-max`.
- Client : `X-Hearth-Client: poste/version` (nom du poste, retenu avec la session).
- Accès : chaque route est déclarée dans `ENDPOINTS` (`entrypoint/http/mod.rs`) avec son niveau (public, authentifié, administrateur) ; un test de balayage vérifie que les routes réservées refusent l'appelant sans droit.
- Réponses : succès = corps JSON propre à la route, sans enveloppe (ex. `/hello`) ; échec = `{ error: { code, message, details } }` (`hearth-proto::error::ErrorBody`).
- Fiches de route détaillées : [hello](./hello.md), [sessions](./sessions.md), [comptes](./accounts.md), [opérations](./operations.md).
