---
id: ADR-0004
titre: Protocole HTTP/WebSocket `/api/v1`, idempotency, versions
type: convention
statut: acceptée
date: 2026-10-04
portee: projet
remplace: —
liens: [conception technique 2026-10-04, section 7]
---

# ADR-0004 — Protocole HTTP/WebSocket `/api/v1`, idempotency, versions

## Contexte

Client et agent doivent négocier compatibilité, supporter la perte réseau (rejeu d'opérations), éviter les effets de bord (login en double ou création compte deux fois). Trois stratégies : gRPC (outillage lourd), REST (verbeux mais standard), **REST + WebSocket typé** sur même adresse.

## Décision

API versionnée `/api/v1`. Requêtes modifiantes portent clé idempotence `Idempotency-Key: <ULID>` (rejouer = même résultat). Négociation client/agent : `X-Hearth-Api: <n>` dans requête, agent répond `426 INCOMPATIBLE_VERSION` si hors `[min, max]`. WebSocket `/stream` pour flux temps réel (mesures, audit, événements session).

## Comment l'appliquer

### Routes HTTP

- **Commandes idempotentes** : `POST /api/v1/sessions`, `PUT /accounts/{id}/password`, `POST /agent/update`, etc.
  - Client génère `Idempotency-Key: <ULID>` persistent pour l'opération.
  - Serveur indexe par clé, rejeu renvoie le premier résultat sans rejouer.
- **Requêtes de lecture** : `GET` sans idempotency (indemne).
- **Authentification** : `Authorization: Bearer <token>` en en-tête ; `401 UNAUTHENTICATED` si absent, `401 SESSION_EXPIRED` si expiré.

### Codes d'erreur

Format : `{ error: { code: "CODE_SNAKE_CASE", message: "diagnostic", details: {} } }`

Codes transverses :
- `401 UNAUTHENTICATED` : pas de token
- `401 SESSION_EXPIRED` : token expiré
- `401 SESSION_REVOKED` : compte supprimé ou session révoquée
- `403 FORBIDDEN_ROLE` : rôle insuffisant
- `409 OPERATION_IN_PROGRESS` : une opération est en cours (ex. mise à jour)
- `422 VALIDATION_ERROR` : paramètre invalide (`details.field`)
- `426 INCOMPATIBLE_VERSION` : version client/agent incompatible (`details.upgrade: "client" | "agent"`)
- `429 TOO_MANY_ATTEMPTS` : trop de tentatives (login) ; `details.retry_after_s`
- `500 INTERNAL_ERROR` : erreur serveur

### WebSocket

Route : `GET /stream` (upgrade). Premier message client : `{ type: "auth", token: "…" }`. Souscriptions :
- `{ type: "subscribe", topics: ["metrics", "audit", "session"] }`
- Réponses serveur : `{ type: "snapshot", machine: {...}, history: {...} }`, `{ type: "metrics", at: "…", cpu: 0.5, ... }` (chaque seconde), `{ type: "audit", event: {...} }` (admin seulement), `{ type: "session", kind: "revoked" | "expired" }`.

### Versioning

- Client envoie `X-Hearth-Api: <n>` (numéro d'interface).
- Agent répond `X-Hearth-Api-Range: <min>-<max>` à chaque réponse.
- Hors plage → `426`.

## Quand NE PAS l'appliquer / limites

- Requêtes de lecture (GET) n'ont pas besoin d'idempotency (implicite).
- Dépréciation endpoint : versioning ne remplace pas la compatibilité backward ; préférer ajouter endpoint nouveau plutôt que changer l'existant.
- WebSocket n'a pas de HTTP status code natif ; fermeture avec `{ error: {...} }` final dans le message ou close code `1008` (policy violation) + texte.

## Alternatives rejetées

- **gRPC** : outillage (protobuf, grpcui), moins flexible pour client web.
- **SSE** (Server-Sent Events) : unidirectionnel agent → client, pas de client → agent (pas de console interactive pour l'épic 2).
- **Custom binary protocol** : surcharge dev, tests, debugging difficile.

## Conséquences

- Chaque opération nécessite un ULID (généré client avant envoi).
- Audit : routes modifiantes évaluées avec rôle et traçabilité.
- Tests : mocking d'idempotency dans les tests d'intégration.

## Références

- HTTP Idempotency (RFC 9110) : https://tools.ietf.org/html/rfc9110#section-9.2.2
- WebSocket : https://datatracker.ietf.org/doc/html/rfc6455
- Axum routing : https://docs.rs/axum/latest/axum/routing/index.html
- ADR-0004 globale (conventions) : `orga-global/docs/adr/ADR-0004-*.md`
