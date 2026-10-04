# GET /api/v1/stream (WebSocket)

Flux temps réel : l'identité et l'historique de la machine, puis un échantillon par seconde, sur la même adresse HTTPS que l'API (mise à niveau d'une requête `GET`). Types : `hearth-proto::stream` (messages JSON à champ `type`, un par trame texte).

- **Authentification** : par le **premier message** (`auth`), pas par l'en-tête `Authorization` (un client web ne peut pas en poser sur un WebSocket). **Rôle requis** : tous ; le sujet `audit` est réservé aux administrateurs.
- **Version d'interface** : `X-Hearth-Api` est requis sur la requête d'ouverture (`422` absent, `426` hors plage), comme ailleurs.
- **Code** : `crates/hearth-agent/src/entrypoint/ws/` (`mod.rs` : mise à niveau et contexte ; `connection.rs` : une connexion), table `ENDPOINTS` (accès `FirstMessage`), `domain/stream.rs` (délais, `is_new`).

## Déroulement

1. Le client ouvre le WebSocket et envoie `{"type":"auth","token":"…"}` **dans les 5 s**. Sinon, ou jeton refusé : message `error` puis fermeture (code `1008`).
2. Le client envoie `{"type":"subscribe","topics":["metrics","session"]}` (`audit` en plus pour un administrateur). Chaque `subscribe` **remplace** les abonnements. S'abonner à `metrics` répond par un `snapshot` puis un `metrics` à chaque échantillon.
3. Le client envoie `{"type":"ping","n":1}` (toutes les 2 s) ; l'agent répond `{"type":"pong","n":1}`. Sans aucun message du client pendant 30 s, l'agent ferme (`1008`).
4. Si la session est révoquée ou expire pendant le flux (vérifiée toutes les 5 s), l'agent envoie `{"type":"session","kind":"revoked"}` ou `{"type":"session","kind":"expired"}`, puis ferme (`1008`).
5. À l'arrêt de l'agent : fermeture propre, code `1001`.

## Messages client → agent

| `type` | Champs | Rôle |
|---|---|---|
| `auth` | `token` | Jeton de session, **premier message obligatoire**. Jamais journalisé. |
| `subscribe` | `topics[]` (`metrics`, `audit`, `session`) | Remplace les abonnements. `audit` pour un compte lecture seule : message `error` `FORBIDDEN_ROLE`, le reste est pris. `session` est toujours reçu : s'y abonner est sans effet. |
| `ping` | `n` | Battement. |

Taille maximale d'un message du client : 4 096 octets (au-delà, l'agent ferme).

## Messages agent → client

| `type` | Champs | Quand |
|---|---|---|
| `snapshot` | `machine` ([machine](./machine.md)), `history` (5 dernières minutes, 1 échantillon par seconde) | Après `subscribe` avec `metrics`. |
| `metrics` | champs de l'échantillon à plat ([metrics](./metrics.md)) | Chaque seconde. |
| `audit` | `event` | Événement du journal d'activité, administrateurs abonnés à `audit`. Forme fixée par le journal (HRT-05) ; silencieux tant qu'aucun journal n'est branché. |
| `session` | `kind` : `revoked` ou `expired` | La session prend fin ; l'agent ferme ensuite. |
| `pong` | `n` | Réponse à `ping`. |
| `error` | `code`, `message`, `details` | Même format et mêmes codes que l'API HTTP. Fatal avant l'authentification (fermeture ensuite) ; ensuite le flux reste ouvert (`VALIDATION_ERROR` : message mal formé ; `FORBIDDEN_ROLE` : sujet interdit). |

## Reprise sans trou ni doublon (BR-DASH-011)

L'agent s'abonne aux échantillons **avant** de lire l'historique du `snapshot`, puis n'envoie que les échantillons plus récents que le dernier de l'historique : un client qui se réabonne (après une reconnexion) recolle `history` puis `metrics` sans trou ni doublon. Un client qui revient après plus d'une heure reçoit un historique partiel (l'anneau ne garde qu'une heure).

## Charge

Chaque connexion est une tâche indépendante. Les échantillons passent par un canal de diffusion borné : un abonné lent perd les plus anciens, il ne ralentit ni l'échantillonnage ni les autres. Un message qui ne part pas en 10 s fait abandonner le client.

## Codes de fermeture

`1001` arrêt de l'agent · `1008` règle du protocole (authentification absente ou refusée, session terminée, message refusé, silence du client).
