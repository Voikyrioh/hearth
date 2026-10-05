# GET /api/v1/stream (WebSocket)

Flux temps réel : l'identité et l'historique de la machine, puis un échantillon par seconde, sur la même adresse HTTPS que l'API (mise à niveau d'une requête `GET`). Types : `hearth-proto::stream` (messages JSON à champ `type`, un par trame texte). `snapshot` sert l'identité de la machine **en cache** (relue au plus toutes les 30 s, et aussitôt qu'une carte graphique nouvelle est vue). **Carte apparue après le `snapshot`** : l'agent renvoie un nouveau `snapshot` complet (identité à jour et historique, à vérifier toutes les 5 s) à l'abonné aux mesures ; le client remplace son identité et recolle l'historique comme après une reconnexion.

- **Authentification** : par le **premier message** (`auth`), pas par l'en-tête `Authorization` (un client web ne peut pas en poser sur un WebSocket). **Rôle requis** : tous ; le sujet `audit` est réservé aux administrateurs.
- **Version d'interface** : `X-Hearth-Api` est requis sur la requête d'ouverture (`422` absent, `426` hors plage), comme ailleurs.
- **Code** : `crates/hearth-agent/src/entrypoint/ws/` (`mod.rs` : mise à niveau et contexte ; `connection.rs` : une connexion), table `ENDPOINTS` (accès `FirstMessage`), `domain/stream.rs` (délais, `is_new`).

## Déroulement

1. Le client ouvre le WebSocket et envoie `{"type":"auth","token":"…"}` **dans les 5 s**. Sinon, ou jeton refusé : message `error` puis fermeture (code `1008`).
2. Le client envoie `{"type":"subscribe","topics":["metrics","session"]}` (`audit` en plus pour un administrateur). Chaque `subscribe` **remplace** les abonnements. S'abonner à `metrics` répond par un `snapshot` puis un `metrics` à chaque échantillon.
   Au plus un `subscribe` par seconde et par connexion : au-delà, message `error` `BUSY` sans fermeture (chacun coûte un `snapshot`).
3. Le client envoie `{"type":"ping","n":1}` (toutes les 2 s) ; l'agent répond `{"type":"pong","n":1}`. Sans aucun message du client pendant 30 s, l'agent ferme (`1008`).
4. La session est revérifiée toutes les 5 s. Si le rôle a changé (un administrateur rétrogradé), l'abonnement `audit` est retiré avec un message `error` `FORBIDDEN_ROLE`. Si la session est révoquée ou expire, l'agent envoie `{"type":"session","kind":"revoked"}` ou `{"type":"session","kind":"expired"}`, puis ferme (`1008`).
5. À l'arrêt de l'agent : fermeture propre, code `1001`.

## Sujet `update` (HRT-17)

Le sujet `update` (tout compte authentifié) diffuse la progression d'une mise à jour de l'agent à distance (BR-UPDATE-013). À l'abonnement, l'état courant s'il y a une mise à jour en cours ; puis `{"type":"update","version":"0.2.0","step":"download","percent":35,"outcome":null,"reason":null}` à chaque changement d'étape ou de pourcentage entier, et à la fin `step: "done"` avec `outcome` et `reason`. Ce type n'est pas un `ServerMessage` : un client qui ne le connaît pas l'ignore. Détails : [agent-update.md](./agent-update.md).

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
| `snapshot` | `machine` ([machine](./machine.md)), `history` (5 dernières minutes, 1 échantillon par seconde) | Après `subscribe` avec `metrics`, et de nouveau quand une carte graphique apparaît. |
| `metrics` | champs de l'échantillon à plat ([metrics](./metrics.md)) | Chaque seconde. |
| `audit` | `event` | Événement du journal d'activité, administrateurs abonnés à `audit`. Une entrée du journal, **dans la même forme que `GET /audit`** (`AuditEventItem` : `id`, `at`, `account`, `origin`, `action`, `action_label`, `target`, `outcome`, `reason`, `repeat_count`, voir [journal](./audit.md)), envoyée une fois la transaction de l'action validée (BR-AUDIT-010). Un abonné lent perd les plus anciennes entrées : il recharge `GET /audit`. Exemple : `{"type":"audit","event":{"id":42,"at":"2026-10-04T10:30:15.250Z","account":"marie","origin":{"kind":"client","name":"poste","addr":"10.0.0.7","text":"10.0.0.7 (poste)"},"action":"account.create","action_label":"Création de compte","target":"paul","outcome":"ok","reason":null,"repeat_count":0}}`. |
| `session` | `kind` : `revoked` ou `expired` | La session prend fin ; l'agent ferme ensuite. |
| `pong` | `n` | Réponse à `ping`. |
| `error` | `code`, `message`, `details` | Même format et mêmes codes que l'API HTTP. Fatal avant l'authentification (fermeture ensuite) ; ensuite le flux reste ouvert (`VALIDATION_ERROR` : message mal formé ; `FORBIDDEN_ROLE` : sujet interdit). |

## Reprise sans trou ni doublon (BR-DASH-011)

L'agent s'abonne aux échantillons **avant** de lire l'historique du `snapshot`, puis n'envoie que les échantillons plus récents que le dernier de l'historique, comparés sur l'horloge **monotone** de l'agent (l'horloge murale `at` peut reculer sans taire le flux) : un client qui se réabonne (après une reconnexion) recolle `history` puis `metrics` sans trou ni doublon. Un client qui revient après plus d'une heure reçoit un historique partiel (l'anneau ne garde qu'une heure).

## Charge et plafonds

Deux quotas distincts :

- **Connexions pas encore authentifiées** (en attente de leur `auth`) : au plus **2 par adresse** et **16 au total**. Au-delà : refus dès l'ouverture, `503 BUSY` au format d'erreur (avec `Retry-After`). La place est rendue dès que l'`auth` réussit, ou à la fermeture quelle qu'en soit la cause (délai de 5 s, coupure, erreur d'authentification).
- **Flux authentifiés** : au plus **32 au total** et **4 par compte**. La place n'est prise qu'après un `auth` réussi ; au-delà, message `error` `BUSY` puis fermeture (`1008`). Un flux fermé rend sa place.

Un anonyme muet ne prend donc jamais la place d'un flux authentifié ; il ne peut qu'occuper les places d'attente de **sa** propre adresse (2) et, avec des adresses multiples, jusqu'à 16 places d'attente pendant 5 s chacune.

Chaque connexion est une tâche indépendante. Les échantillons passent par un canal de diffusion borné : un abonné lent perd les plus anciens, il ne ralentit ni l'échantillonnage ni les autres. Un message qui ne part pas en 10 s fait abandonner le client.

## Codes de fermeture

`1001` arrêt de l'agent · `1008` règle du protocole (authentification absente ou refusée, trop de flux pour ce compte, session terminée, message refusé, silence du client).
