# Sécurité : `/security`

L'état de sécurité du compte connecté (HRT-24, ADR-0024, BR-TRUST-008). Code : `crates/hearth-agent/src/entrypoint/http/security.rs` (handler), `application/security.rs` (cas d'usage), `domain/identifier_slowdown.rs` (règle de l'alerte). Types du fil : `hearth-proto::api::security`.

La route exige `X-Hearth-Api` et un jeton, et est ouverte à **tout rôle**. Elle ne modifie rien et n'est pas journalisée.

## `GET /api/v1/security`

### Réponse `200`

```json
{
  "alert": { "own": true, "since": "2026-10-07T01:04:11Z", "others": 1 },
  "attack_mode": { "state": "off" },
  "device": "proven"
}
```

- `alert.own` : l'identifiant du compte connecté est visé en ce moment (plus de 10 échecs venus d'adresses inconnues, dernier échec de moins de 30 minutes, BR-CONN-018). `alert.since` : début de l'épisode, seulement quand `own` est vrai.
- `alert.others` : **administrateur seulement** : combien d'AUTRES comptes **existants** sont visés. Jamais leurs noms (ils se lisent dans le journal). Absent pour tout autre rôle : un compte ne peut jamais apprendre qu'un autre identifiant est visé. Un identifiant qui n'existe pas n'est jamais compté.
- `attack_mode.state` : `off` | `active` | `suspended` ; **toujours `off` jusqu'à HRT-25** (`since`, `resumes_in_s`, `last_end` apparaîtront avec lui).
- `device` : `proven` si la session a été ouverte ou prouvée par la clé d'un poste inscrit, `none` sinon.

## Message de flux `security`

Voir [stream](./stream.md) : `{"type":"security","alert":{…},"attack_mode":{…}}`, mêmes objets (sans `device`). **Toujours envoyé** après l'`auth` (sans abonnement), puis à chaque changement de l'état **de ce compte** (début ou fin d'une alerte, changement de rôle). Ce n'est pas un `ServerMessage` : un client qui ne le connaît pas l'ignore.

## Journal

`security.alert` (« Attaque probable signalée ») : une entrée au début de l'épisode (cible « début de l'alerte », refusé « trop de tentatives, attente de N s », origine = la tentative qui l'a ouvert) et une à sa fin (cible « fin de l'alerte (levée par l'agent) », réussi). Le compte est celui de l'identifiant visé ; **rien** pour un identifiant inexistant.
