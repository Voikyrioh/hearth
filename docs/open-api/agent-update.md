# Mise à jour de l'agent : `/agent/update`, `/agent/update/last`

Mettre l'agent à jour à distance, avec retour arrière automatique (HRT-17). Types : `hearth-proto::api::update`. Code : `crates/hearth-agent/src/entrypoint/http/update.rs`, cas d'usage `application/update.rs`, superviseur `application/update_supervisor.rs`. Règles : `BR-UPDATE-011` à `BR-UPDATE-019`, `BR-UPDATE-024` et `BR-UPDATE-027` à `BR-UPDATE-029` (`docs/business-rules/`). Décisions : ADR-0008, ADR-0014. Procédure d'exploitation : `docs/runbooks/mettre-a-jour-agent.md`.

La mise à jour s'exécute **côté serveur** : `POST` répond `202` tout de suite, le client suit l'avancement par le flux (sujet `update`) ou par `GET /agent/update`, et lit le résultat par `GET /agent/update/last` quand il revient après une coupure ou un redémarrage de l'agent.

## `GET /api/v1/agent/update`

- **Rôle** : tous (lecture). Pas de clé d'opération.
- `200` :

```json
{
  "current": "0.1.0",
  "managed": false,
  "in_progress": true,
  "progress": { "version": "0.2.0", "step": "download", "percent": 35, "outcome": null, "reason": null },
  "last": { "version": "0.2.0", "previous": "0.1.0", "outcome": "succeeded", "reason": null, "at": "2026-10-05T10:00:00Z" }
}
```

- `current` : la version de l'agent qui répond. `managed` : `true` si l'installation est gérée par le système (`--managed`) **ou** si l'agent ne tourne pas sous systemd : pas de mise à jour à distance.
- `in_progress` : une mise à jour travaille (dans ce processus, ou un superviseur tient le verrou après un redémarrage). `progress` : où elle en est (`null` sinon). Après le redémarrage de l'agent, `progress.step` vaut `check`.
- `last` : le dernier résultat (`null` si aucune mise à jour n'a jamais eu lieu). Quand la version visée **ne se sait pas** (une trace de travail illisible a été conclue au démarrage), `version` est la chaîne vide et la clé `version_unknown: true` est présente : `last: { "version": "", "version_unknown": true, "previous": "0.1.0", "outcome": "failed", "reason": "interrupted", "at": "…" }`. **Ajout compatible** : pour un résultat ordinaire (version connue), la clé `version_unknown` est **absente** et la forme est celle d'avant ; un client qui l'ignore voit une version vide. Sur le flux, le message `done` d'une version inconnue porte aussi `version: ""` : relire `GET /agent/update/last`.

## `GET /api/v1/agent/update/last`

- **Rôle** : tous. `200` : `{ "last": { … } | null }`, le même objet `last` que ci-dessus. Pour le client qui revient après une coupure : « au retour du lien, afficher le résultat réel » (BR-UPDATE-017). Le résultat est un fichier de l'agent : il survit à son redémarrage.

## `POST /api/v1/agent/update` (administrateur)

- **Rôle** : administrateur seulement (BR-UPDATE-011). Suivie par clé d'opération (`Idempotency-Key`) : rejouer la même clé après une coupure renvoie le même `202` sans relancer.
- **Corps** : la cible telle que le flux de versions la publie (l'agent n'interroge jamais Internet de lui-même).

```json
{
  "version": "0.2.0",
  "url": "https://exemple.org/hearth-agent-linux-x86_64",
  "signature": "untrusted comment: signature from minisign secret key\nRUQ…\ntrusted comment: …\n…",
  "sha256": "ab12…(64 caractères hexadécimaux)"
}
```

  - `version` : `X.Y.Z`, **strictement plus récente** que `current`.
  - `url` : HTTPS seulement, sans identifiant dans l'adresse, 2048 caractères au plus. **Adresse publique** : bouclage, privées, lien-local et partagées refusées (`422 VALIDATION_ERROR`, `details.field = "url"`, « adresse locale ou privée refusée »), aussi après résolution du nom et à chaque redirection (BR-UPDATE-027). Les redirections sont suivies une à une et doivent rester en HTTPS. Le serveur télécharge **directement** : aucun proxy d'environnement (`HTTPS_PROXY`...) n'est utilisé, sinon le filtre des adresses serait contourné ; un serveur qui ne sort que par un proxy ne peut pas se mettre à jour à distance (`unreachable`).
  - `signature` : le contenu du fichier `.minisig` de minisign, ou son encodage base64 (le format de Tauri). Elle doit être faite par la clé publique **embarquée** dans l'agent.
  - `sha256` : somme du binaire, hexadécimale.
- `202` : `{ "version": "0.2.0", "step": "download" }`. L'avancement suit sur le flux.
- Le fichier est téléchargé **en mémoire** (128 Mio au plus). **Rien n'est écrit ni exécuté avant que la somme ET la signature soient vérifiées.**

### Erreurs

| Statut | Code | Quand | Message (diagnostic ; l'interface affiche les siens, indexés par `code`) |
|---|---|---|---|
| 401 | `UNAUTHENTICATED`, `SESSION_EXPIRED`, `SESSION_REVOKED` | pas de session | |
| 403 | `FORBIDDEN_ROLE` | compte en lecture seule (consigné au journal) | « Seul un administrateur peut mettre à jour l'agent » |
| 409 | `MANAGED_INSTALL` | installation gérée par le système, ou agent sans systemd | « Cette installation est gérée par le système : l'agent ne se met pas à jour à distance. Mets-le à jour par la configuration du système. » |
| 409 | `OPERATION_IN_PROGRESS` | une mise à jour est déjà en cours (BR-UPDATE-012) | « Une mise à jour de l'agent est déjà en cours. Réessaye plus tard. » |
| 422 | `BAD_SIGNATURE` | signature illisible ou d'une autre clé : refusée **avant tout téléchargement** | « La signature de la mise à jour est refusée. Rien n'a été téléchargé ni modifié. » |
| 422 | `VALIDATION_ERROR` | `details.field` : `version` (illisible, ou pas plus récente), `url`, `sha256`, `signature` | |

Toutes ces erreurs sont consignées au journal d'activité (action `agent.update`, BR-UPDATE-024).

## Étapes et résultat (BR-UPDATE-013, BR-UPDATE-015)

`step` : `download` (avec `percent`, 0 à 100, un message par pourcentage entier), `verify` (somme puis signature), `install` (dépôt du binaire, contrôle de la version annoncée), `restart` (le superviseur arrête l'agent : le lien tombe), `check` (le nouvel agent a 60 s pour répondre), `done`.

`done` porte `outcome` :

| `outcome` | Sens | `reason` possibles |
|---|---|---|
| `succeeded` | le nouvel agent répond avec la nouvelle version et le même certificat | |
| `rolled_back` | le nouvel agent n'a pas répondu en 60 s (ou a changé de certificat) : l'ancien binaire est revenu, identique octet pour octet | `no_answer`, `identity_changed` |
| `failed` | rien n'a changé sur le serveur | `unreachable` (pas d'accès à Internet, BR-UPDATE-019), `download_failed`, `bad_checksum`, `bad_signature`, `bad_binary`, `staging`, `swap` (aussi : pas assez de place pour copier la base, ou place impossible à mesurer, BR-UPDATE-029), `supervisor_launch`, `interrupted` (l'agent s'est arrêté pendant la mise à jour avant l'échange des binaires, ou une version tierce a été posée à la main par-dessus une mise à jour laissée en cours : conclue au démarrage suivant, BR-UPDATE-028), `rollback_failed` (le retour arrière lui-même a échoué, ou une reprise automatique n'a rien conclu : voir le runbook), `unknown` (côté client : une raison ajoutée par un agent plus récent est lue comme `unknown`, jamais comme une erreur de lecture) |

## Flux temps réel : sujet `update`

`{"type":"subscribe","topics":["update","metrics","session"]}` (le sujet `update` est ouvert à tout compte authentifié). À l'abonnement, l'agent envoie d'abord l'état courant s'il y a une mise à jour en cours, puis un message à chaque changement :

```json
{ "type": "update", "version": "0.2.0", "step": "download", "percent": 35, "outcome": null, "reason": null }
```

Un client qui ne connaît pas ce type de trame l'ignore. Le client actuel la lit comme `ServerMessage::Update` (même forme à plat que ci-dessus : ADR-0021) ; l'agent l'envoie encore sous le nom `UpdateMessage`, identique sur le fil. Pendant `restart`, la connexion se coupe ; à son retour, `GET /agent/update` et `GET /agent/update/last` disent où elle en est et comment elle s'est terminée. Détails du flux : [stream.md](./stream.md).

## Consommation par le client (HRT-17, lot interface, ADR-0021)

Ce que le client fait de ce contrat, pour qui écrit un autre client ou diagnostique :

- **La cible vient du client, jamais de l'interface** : le client lit la section `agent` du `latest.json` de la dernière release publiée de `Voikyrioh/hearth`, dans la MÊME requête que son propre flux de versions (une vérification = une requête, au plus une par 24 h, ADR-0017 et ADR-0021), et ne transmet à `POST /agent/update` que ce qu'il y a lu après l'avoir validé (version `X.Y.Z` plus récente que `current`, adresse HTTPS publique des releases du dépôt, signature, somme). Section du manifeste :

```json
{
  "version": "1.2.0",
  "platforms": { "windows-x86_64": { "url": "…", "signature": "…" } },
  "agent": {
    "version": "0.2.0",
    "platforms": {
      "linux-x86_64": {
        "url": "https://github.com/Voikyrioh/hearth/releases/download/v0.2.0/hearth-agent-linux-x86_64",
        "signature": "untrusted comment: …\nRUQ…\ntrusted comment: …\n…",
        "sha256": "ab12…(64 caractères hexadécimaux)"
      }
    }
  }
}
```

  Ajoutée par `cargo xtask agent-manifest` (qui vérifie la signature contre la clé embarquée dans l'agent). Absente, ou sans entrée `linux-x86_64` : rien n'est proposé.
- **Lectures** : `GET /agent/update` puis `GET /agent/update/last` à chaque connexion du lien (BR-UPDATE-017) ; `last.version_unknown` se lit « version inconnue » (aucun numéro affiché). Sujet `update` abonné à chaque connexion : l'état courant arrive d'abord.
- **Action** : `POST /agent/update` par l'action typée du client (clé d'opération `Idempotency-Key`, résultat inconnu à la coupure, jamais rejouée). Refus lus par leur code : `FORBIDDEN_ROLE` (échec « rôle »), `MANAGED_INSTALL`, `OPERATION_IN_PROGRESS`, `BAD_SIGNATURE`, `VALIDATION_ERROR` ; le texte du message de l'agent n'est jamais affiché.
- **Coupure attendue** : après l'étape `restart`, la fermeture du flux (code 1001) est une coupure attendue pendant 2 minutes (BR-UPDATE-014).
- **Versions incompatibles** : toute route sauf `/hello` répond `426` hors plage (BR-CONN-014) ; le client ne peut donc pas mettre à jour un agent trop ancien (ADR-0021).
