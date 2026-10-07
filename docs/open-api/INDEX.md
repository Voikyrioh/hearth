# API — Hearth

Endpoints `/api/v1` (JSON, HTTPS). Authentification par Bearer token en en-tête `Authorization: Bearer …`.

**Actes d'administration** (HRT-28) : les routes qui modifient (hors déconnexion et hors retrait d'un poste, qui garde son contrat) acceptent un membre `reauth` (mot de passe + preuve de clé d'usage `0x05`) ; l'agent l'EXIGE depuis HRT-30 (`GET /security` : `admin_reauth.required: true`) ; sans `reauth`, `426 INCOMPATIBLE_VERSION` « client trop ancien ». Voir [sécurité](./security.md).

30 endpoints : session, postes de confiance, sécurité, comptes, mesures, audit, mises à jour.

| Groupe | Méthode | Route | Rôle | Description |
|---|---|---|---|---|
| **Session** | GET | [`/hello`](./hello.md) | — | Identité de l'agent, plage de versions (l'empreinte se lit sur le certificat TLS) |
| **Session** | POST | [`/sessions`](./sessions.md) | — | Login (username + password, preuve de clé d'appareil optionnelle) → token |
| **Session** | POST | [`/sessions/challenge`](./sessions.md) | — | Défi pour la clé d'appareil (sans état, 60 s, identique pour tout identifiant) |
| **Session** | DELETE | [`/sessions/current`](./sessions.md) | user+ | Logout |
| **Session** | GET | [`/me`](./sessions.md) | user+ | Compte courant + rôle |
| **Session** | GET | [`/operations/{id}`](./operations.md) | user+ | Statut opération idempotente |
| **Postes** | GET | [`/me/devices`](./devices.md) | user+ | Mes postes de confiance (nom, dates, dernière adresse, poste courant) |
| **Postes** | DELETE | [`/me/devices/{id}`](./devices.md) | user+ | Retirer un poste (sa clé, son adresse retenue, ses sessions) |
| **Sécurité** | GET | [`/security`](./security.md) | user+ | État de sécurité du compte : alerte « attaque probable » (`own`, `since`, `others` pour un administrateur), mode attaque (`off`, `active`, `suspended` avec `resumes_in_s`, `last_end`) |
| **Sécurité** | PUT | [`/security/attack-mode`](./security.md) | admin | Activer ou désactiver le mode attaque : mot de passe actuel et preuve d'une clé inscrite du compte (usage `0x03`, liée au jeton et au geste) ; `409 POST_NOT_RECOGNIZED` sans preuve valide |
| **Sécurité** | PUT | [`/me/reauth`](./security.md) | user+ | Réglage de la fréquence du mot de passe en administration (`window` : 5 minutes, ou `each`) ; toujours avec mot de passe et clé (HRT-28) |
| **Mesures** | GET | [`/machine`](./machine.md) | user+ | Identité machine, specs hardware |
| **Mesures** | GET | [`/stream`](./stream.md) | user+ (jeton dans le premier message) | WebSocket flux temps réel (snapshot + metrics chaque seconde) |
| **Mesures** | GET | [`/metrics/history?window=1m\|5m\|1h`](./metrics.md) | user+ | Historique rééchantillonné |
| **Comptes** | GET | [`/accounts`](./accounts.md) | admin | Liste comptes, sessions ouvertes, dernier login |
| **Comptes** | POST | [`/accounts`](./accounts.md) | admin | Créer compte (username, password, role) |
| **Comptes** | PATCH | [`/accounts/{id}`](./accounts.md) | admin | Modifier rôle |
| **Comptes** | PUT | [`/accounts/{id}/password`](./accounts.md) | admin | Changer mot de passe (ferme sessions compte) |
| **Comptes** | PUT | [`/me/password`](./accounts.md) | user+ | Changer son mot de passe (ferme autres sessions) |
| **Comptes** | DELETE | [`/accounts/{id}`](./accounts.md) | admin | Supprimer compte |
| **Comptes** | DELETE | [`/accounts/{id}/sessions`](./accounts.md) | admin | Fermer toutes les sessions du compte |
| **Audit** | GET | [`/audit?filters…`](./audit.md) | admin | Journal d'activité (filtres, recherche plein texte, pagination curseur) |
| **Audit** | GET | [`/audit/export?filters…`](./audit.md) | admin | Journal CSV (UTF-8 BOM, `;` séparateur) |
| **Mise à jour** | GET | [`/agent/update`](./agent-update.md) | user+ | Statut mise à jour (version, installation gérée, en cours, dernier résultat) |
| **Mise à jour** | GET | [`/agent/update/last`](./agent-update.md) | user+ | Dernier résultat (survit au redémarrage de l'agent) |
| **Mise à jour** | POST | [`/agent/update`](./agent-update.md) | admin | Lancer la mise à jour (version, URL HTTPS, signature minisign, somme SHA-256) : `202`, suivi par le flux |
| **CLI** | `hearth-agent install` | — | — | Installation interactive (env var override) |
| **CLI** | `hearth-agent uninstall` | — | — | Désinstallation |
| **CLI** | `hearth-agent account` | — | — | Sous-cmds : add, list, passwd, role, remove, revoke |
| **CLI** | `hearth-agent attack-mode` | — | — | Sous-cmds : status, off (voie de secours sans réseau, root) ; pas d'activation |
| **CLI** | `hearth-agent fingerprint` | — | — | Affiche empreinte certificat |

## Codes d'erreur transverses

- `401 UNAUTHENTICATED` : pas de token, ou token illisible
- `401 INVALID_CREDENTIALS` : identifiant ou mot de passe refusé, sans préciser lequel
- `401 SESSION_EXPIRED` : token expiré
- `401 SESSION_REVOKED` : session révoquée
- `403 FORBIDDEN_ROLE` : rôle insuffisant
- `409 OPERATION_IN_PROGRESS` : opération en cours (ex. mise à jour)
- `409 MANAGED_INSTALL` : installation gérée par le système, pas de mise à jour à distance ; `422 BAD_SIGNATURE` : signature de mise à jour refusée : voir [mise à jour de l'agent](./agent-update.md)
- `422 VALIDATION_ERROR` : paramètre invalide (détails champ)
- `426 INCOMPATIBLE_VERSION` : client/agent incompatibles
- `429 TOO_MANY_ATTEMPTS` : trop de tentatives (login verrouillé), `details.retry_after_s`
- `409 USERNAME_TAKEN`, `409 LAST_ADMIN`, `409 CONFLICT` ; `422 WEAK_PASSWORD` (`details.rules`), `422 WRONG_PASSWORD` : voir [comptes](./accounts.md)
- `409 POST_NOT_RECOGNIZED` : activer ou désactiver le mode attaque sans preuve valide d'une clé inscrite pour le compte appelant (`details.reason`) : voir [sécurité](./security.md)
- `422 IDEMPOTENCY_KEY_REUSED` : la clé d'opération a déjà servi pour une autre requête
- `413 PAYLOAD_TOO_LARGE` : corps d'une requête suivie au-delà de 1 Mio
- `503 BUSY` : agent saturé (calculs de mots de passe), en-tête `Retry-After`
- `500 INTERNAL_ERROR` : erreur serveur

## Conventions

- Idempotency : requêtes modifiantes portent `Idempotency-Key: <ULID>` (rejouer = même résultat, sans ré-exécuter ; en cours = `409` ; autre requête = `422 IDEMPOTENCY_KEY_REUSED`) : voir [opérations](./operations.md). `POST /sessions` n'est pas suivi (son résultat contient un jeton).
- Versioning : client envoie `X-Hearth-Api: <n>` (obligatoire sauf sur `/hello`) ; hors plage `426 INCOMPATIBLE_VERSION` avec `details.upgrade` ; l'agent répond `X-Hearth-Api-Range: min-max`.
- Client : `X-Hearth-Client: poste/version` (nom du poste, retenu avec la session).
- Accès : chaque route est déclarée dans `ENDPOINTS` (`entrypoint/http/mod.rs`) avec son niveau (public, authentifié, administrateur, ou « premier message » pour le flux, qui s'authentifie lui-même) son suivi par clé et son action de journal (les refus et les échecs sont consignés par la couche d'accès) ; le routeur pose la couche d'accès depuis la table et un test de balayage vérifie que les routes réservées refusent l'appelant sans droit.
- Réponses : succès = corps JSON propre à la route, sans enveloppe (ex. `/hello`) ; échec = `{ error: { code, message, details } }` (`hearth-proto::error::ErrorBody`).
- Fiches de route détaillées : [mise à jour de l'agent](./agent-update.md), [hello](./hello.md), [sessions](./sessions.md), [postes de confiance](./devices.md), [sécurité](./security.md), [comptes](./accounts.md), [opérations](./operations.md), [machine](./machine.md), [historique des mesures](./metrics.md), [flux temps réel](./stream.md), [journal](./audit.md).
