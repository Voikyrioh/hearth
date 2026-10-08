# Sessions : `/sessions`, `/me`

Connexion, déconnexion, compte courant et défi de la clé d'appareil. Code : `crates/hearth-agent/src/entrypoint/http/sessions.rs` (handlers), `application/sessions.rs` (cas d'usage), `domain/{lockout,sessions,session_token}.rs` (règles). Types du fil : `hearth-proto::api::sessions`.

Toutes ces routes exigent l'en-tête `X-Hearth-Api: <n>` (BR-CONN-014) ; voir « Conventions » dans l'[index](./INDEX.md).

## `POST /api/v1/sessions` : se connecter

- **Authentification** : aucune. **Rôle** : aucun.
- **Suivi par clé** : non (la réponse contient un jeton, qu'on ne conserve pas en base).
- **En-têtes** : `X-Hearth-Api`, `X-Hearth-Client: poste/version` (nom du poste, 128 caractères au plus, `inconnu` si absent).
- **Corps** : `{ "username": "marie", "password": "…" }` (l'identifiant est insensible à la casse), plus, en option, la preuve de la clé d'appareil :

```json
{
  "username": "marie",
  "password": "…",
  "device": {
    "algorithm": "ed25519",
    "public_key": "base64 de 32 octets",
    "challenge": "le défi reçu de POST /sessions/challenge, tel quel",
    "signature": "base64 de 64 octets"
  }
}
```

  `device` est **optionnel** (HRT-22, ADR-0023, BR-TRUST-004 et 005). Absent, illisible (mauvaise forme), invalide (signature fausse, défi expiré, rejoué, forgé, d'un autre usage, d'un autre identifiant, d'une autre adresse, signé pour un autre serveur) : la connexion se déroule **exactement comme sans clé**, jamais une erreur propre à la clé. La clé ne change aucune décision d'accès : elle n'est prise en compte qu'**après** un mot de passe juste, dans la transaction de la connexion.

### Réponse `201`

```json
{
  "token": "9f2c…64 caractères hexadécimaux…",
  "expires_at": "2026-11-03T10:30:15.25Z",
  "account": { "id": "01J9ZY0G3Q8M2K6W4T7V5N1B9D", "username": "marie", "role": "admin" },
  "device": "enrolled"
}
```

- `device` (absent quand aucune clé n'a été prise en compte : pas de clé, preuve invalide, clé inscrite pour un autre compte) : `enrolled` (clé inscrite par cette connexion), `proven` (clé déjà inscrite, preuve valide), `limit` (le compte a déjà 8 postes : rien n'est inscrit, aucune éviction), `deferred` (preuve valide, inscription gelée pendant le mode attaque). Un client qui ne connaît pas le champ l'ignore.

- `token` : 32 octets aléatoires du système, en hexadécimal minuscule. Rendu une seule fois. L'agent n'en garde que le SHA-256 (BR-RESIL-012). À envoyer en `Authorization: Bearer <token>`.
- `expires_at` : fin de la session si elle reste inactive ; l'activité la repousse (30 jours glissants).
- `account.role` : `admin` ou `readonly`.

### Erreurs

| Statut | Code | Quand |
|---|---|---|
| 401 | `INVALID_CREDENTIALS` | Identifiant inconnu **ou** mot de passe faux, sans distinction (BR-CONN-013). |
| 429 | `TOO_MANY_ATTEMPTS` | 5 échecs pour ce couple identifiant + adresse, 20 échecs en 10 minutes pour cette adresse tous identifiants confondus (adresses exactes, IPv6 comprise), ou, plus de 10 échecs sur cet identifiant venus d'adresses inconnues (attente de 2 s doublée, au plus 2 minutes, refus après vérification du mot de passe) ; `details.retry_after_s` (BR-CONN-006, BR-CONN-007, BR-CONN-018). La réponse est la même quel que soit le compteur et que l'identifiant existe ou non. Même le bon mot de passe est refusé pendant l'attente, sauf depuis une adresse connue du compte pour le seul ralentissement par identifiant (BR-CONN-019, provisoire). |
| 422 | `VALIDATION_ERROR` | Corps illisible ou champ manquant, ou `X-Hearth-Api` absent. |
| 503 | `BUSY` | Trop de vérifications de mot de passe en cours : réessayer après `Retry-After` (1 s). |
| 429 | `TOO_MANY_ATTEMPTS` | Aussi quand plus de huit connexions attendent déjà pour la même adresse, ou quand l'agent a déjà 32 connexions en cours (24 pour une adresse inconnue du compte : 8 places sont réservées aux adresses connues, BR-CONN-020) : `details.retry_after_s = 1`. |
| 426 | `INCOMPATIBLE_VERSION` | Version d'interface hors plage ; `details.upgrade` : `client` ou `agent` (BR-CONN-014). |

## `POST /api/v1/sessions/challenge` : demander un défi pour la clé d'appareil

- **Authentification** : aucune. **Rôle** : aucun. **Suivi par clé** : non. **Journal** : non. En-tête `X-Hearth-Api` exigé.
- **Corps** : `{ "username": "marie", "purpose": "login" }`. `username` : la saisie, **existante ou non** ; `purpose` : `login` (connexion, octet `0x01`), `session` (ouverture du flux, `0x02`), `device_removal` (retrait d'un poste de confiance, `0x04` ; l'octet `0x03` (ancien mode attaque à plat) est retiré et ne se réutilise pas, voir [postes de confiance](./devices.md)), `admin_act` (un acte d'administration, `0x05`, HRT-28 : voir [sécurité](./security.md), « Confirmation des actes d'administration »). Un agent d'avant HRT-28 répond `422 VALIDATION_ERROR` à `admin_act` : le client ne le demande que si `admin_reauth` est annoncé.
- **Réponse `200`**, identique que l'identifiant existe ou non (même code, mêmes en-têtes, même forme et même taille) :

```json
{ "challenge": "base64 de 56 octets", "expires_in_s": 60 }
```

- **Sans état et sans lecture en base** : le défi est `nonce (16) || émission (8, millisecondes monotones) || HMAC-SHA256 (32)`, recalculé à la vérification. Il vaut 60 secondes, **pour l'adresse qui l'a demandé** et pour l'usage demandé, et ne sert qu'**une fois** (consommé quand sa preuve est validée). Un redémarrage de l'agent invalide les défis en cours.
- **Ce que le client signe** : le message de `hearth_proto::device_proof::signing_bytes` (préfixe `hearth-device-proof/1`, octet d'usage, empreinte SHA-256 du certificat du serveur épinglé, identifiant normalisé, défi, et pour les usages `session` et `attack_mode` l'empreinte du jeton). Voir [ADR-0023](../adr/ADR-0023-identite-d-appareil.md).

| Statut | Code | Quand |
|---|---|---|
| 422 | `VALIDATION_ERROR` | Corps illisible, `purpose` inconnu, ou `X-Hearth-Api` absent. |
| 426 | `INCOMPATIBLE_VERSION` | Version d'interface hors plage. |
| 404 | `NOT_FOUND` | Agent d'avant cette fonction : le client en déduit « clé non prise en charge » et se connecte sans clé. |

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

- BR-CONN-006, BR-CONN-007, BR-CONN-013, BR-CONN-014, BR-CONN-018, BR-CONN-019, BR-CONN-020, BR-RESIL-012, BR-RESIL-014. Décision : ADR-0022.
- L'adresse du client est celle de la connexion TCP ; `X-Forwarded-For` et `Forwarded` sont ignorés.
- Jamais de jeton, de mot de passe ni de haché dans un journal ou un message d'erreur.
