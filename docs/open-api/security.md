# Sécurité : `/security` et `/security/attack-mode`

L'état de sécurité du compte connecté (HRT-24, ADR-0024, BR-TRUST-008) et le mode attaque du serveur (HRT-25, ADR-0025, BR-TRUST-011 à 021, 028). Code : `crates/hearth-agent/src/entrypoint/http/security.rs` (handlers), `application/security.rs` et `application/attack_mode.rs` (cas d'usage), `domain/identifier_slowdown.rs` (règle de l'alerte), `domain/trust/{recognition,attack_mode}.rs` (règles du mode attaque). Types du fil : `hearth-proto::api::security`.

`GET /security` exige `X-Hearth-Api` et un jeton, et est ouverte à **tout rôle**. Elle ne modifie rien et n'est pas journalisée.

## `GET /api/v1/security`

### Réponse `200`

```json
{
  "alert": { "own": true, "since": "2026-10-07T01:04:11Z", "others": 1 },
  "attack_mode": { "state": "off" },
  "device": "proven",
  "admin_reauth": { "required": false, "factors": ["password", "device_key"], "password": "window", "elevated_for_s": 0 }
}
```

- `alert.own` : l'identifiant du compte connecté est visé en ce moment (plus de 10 échecs venus d'adresses inconnues, dernier échec de moins de 30 minutes, BR-CONN-018). `alert.since` : début de l'épisode, seulement quand `own` est vrai.
- `alert.others` : **administrateur seulement** : combien d'AUTRES comptes **existants** sont visés. Jamais leurs noms (ils se lisent dans le journal). Absent pour tout autre rôle : un compte ne peut jamais apprendre qu'un autre identifiant est visé. Un identifiant qui n'existe pas n'est jamais compté.
- `attack_mode` : le mode attaque du **serveur** (global, visible de tout compte authentifié : il n'est reconnu que s'il a deux critères en mode attaque, BR-TRUST-011).
  - `state` : `off` | `active` | `suspended` (la machine vient de démarrer : régime d'alerte pendant 30 minutes, BR-TRUST-020).
  - `since` : début de l'activation en cours (`active` et `suspended`).
  - `resumes_in_s` : seulement `suspended` : les secondes restantes de la fenêtre (arrondies à la seconde supérieure, mesurées sur le temps écoulé depuis le démarrage du noyau, jamais sur l'horloge murale).
  - `last_end` : seulement `off`, comment la dernière activation s'est terminée : `manual` (depuis le client), `auto` (30 minutes sans tentative refusée), `cli` (`hearth-agent attack-mode off`).
  - Exemples : `{"state":"off"}`, `{"state":"active","since":"2026-10-07T01:00:00Z"}`, `{"state":"suspended","since":"…","resumes_in_s":1200}`, `{"state":"off","last_end":"auto"}`.
- `device` : `proven` si la session a été ouverte ou prouvée par la clé d'un poste inscrit, `none` sinon. Le poste d'une session est posé à la connexion par mot de passe et n'est jamais réécrit par une preuve de session (BR-TRUST-048).
- `admin_reauth` (HRT-28) : la confirmation des actes d'administration. **Absent : agent d'avant ce ticket** (le client agit comme avant, sans rien confirmer). `required` : l'agent **exige** la confirmation (`true` depuis HRT-30 : le client qui confirme est livré, ADR-0033 ; `false` seulement pour un banc de test qui construit le service à la main). Un acte sans `reauth` reçoit alors `426 INCOMPATIBLE_VERSION` (`details.upgrade: "client"`, `details.reason: "reauth_required"`). `factors` : ce qu'il faut réunir (`password`, `device_key`). `password` : le réglage du compte, `window` (défaut, une saisie pour 5 minutes) ou `each` (à chaque action). `elevated_for_s` : secondes restantes de l'élévation de **cette session depuis cette adresse**, 0 sinon (indicatif : l'agent décide à l'acte).

## Message de flux `security`

Voir [stream](./stream.md) : `{"type":"security","alert":{…},"attack_mode":{…}}`, mêmes objets (sans `device`). **Toujours envoyé** après l'`auth` (sans abonnement), puis à chaque changement de l'état **de ce compte** (début ou fin d'une alerte, changement de rôle). Ce n'est pas un `ServerMessage` : un client qui ne le connaît pas l'ignore.

## `PUT /api/v1/security/attack-mode`

Active ou désactive le mode attaque. **Administrateur** (`Access::Admin`), suivie par clé d'opération. **Un acte d'administration** (Q14 point 3, Q16) : le corps porte le mot de passe actuel ET la preuve de possession d'une clé **inscrite pour le compte appelant** ; activer comme désactiver (BR-TRUST-018, 028).

```json
{ "active": true, "password": "…", "device": { "algorithm": "ed25519", "public_key": "…", "challenge": "…", "signature": "…" } }
```

La preuve : `POST /sessions/challenge` avec `purpose: "attack_mode"`, puis signature d'usage `0x03` liée à l'empreinte du certificat du serveur, à l'identifiant, au **hachage du jeton** de la session appelante et à la **valeur demandée** (`0x01` activer, `0x00` désactiver) : une preuve d'activation ne désactive pas, et inversement. Défi de 60 secondes, usage unique, consommé seulement si le changement a réussi (un mot de passe faux ne brûle pas la preuve). Détail des octets : `hearth_proto::device_proof`.

### Réponse `200`
L'objet `attack_mode` ci-dessus (état après le changement). Idempotente : activer un mode actif, désactiver un mode éteint ne change rien et n'écrit rien.

### Erreurs
| Code | Cas |
|---|---|
| `401 UNAUTHENTICATED` / `SESSION_EXPIRED` | pas de jeton ; en mode attaque, session présentée seule (BR-TRUST-013) |
| `403 FORBIDDEN_ROLE` | compte Lecture seule (« Tu n'as pas la permission d'activer le mode attaque. C'est réservé aux administrateurs. »), consigné « refusé » sous `attack_mode.enable` ou `attack_mode.disable` (le geste demandé) |
| `409 POST_NOT_RECOGNIZED` | pas de preuve valide d'une clé inscrite pour le compte appelant ; `details.field = "device"`, `details.reason` : `proof_missing` (aucune preuve, ou illisible) ou `proof_invalid` (périmée, rejouée, d'un autre usage ou de l'autre geste, d'un autre jeton, clé non inscrite ou d'un autre compte). Aucune écriture d'état ; **aucun mot de passe n'est essayé** |
| `422 WRONG_PASSWORD` | mot de passe actuel faux : compté comme un échec de connexion (mêmes compteurs, même ralentissement) |
| `429 TOO_MANY_ATTEMPTS` | attente du compteur du couple, de l'adresse ou de l'identifiant (`details.retry_after_s`) |
| `422 VALIDATION_ERROR` | corps illisible (`active` manquant) |

Un administrateur dont le client n'a pas de clé inscrite (client ancien, poste non inscrit) ne peut ni activer ni désactiver depuis le client : `hearth-agent attack-mode off` sur le serveur, ou le redémarrage physique de la machine (BR-TRUST-027). Il n'existe pas de sous-commande pour activer.

**Journal des refus de cette route** : le geste se lit dans `active` du corps, mais le corps n'est lu qu'une fois la session reconnue et le rôle admis. Un `401` (sans jeton ou jeton inconnu) ne lit aucun corps et n'est pas journalisé ; un `403` (rôle insuffisant) ne lit aucun corps non plus et est consigné « refusé » sous `attack_mode.enable` (geste par défaut) ; pour un administrateur authentifié dont le corps est illisible, absent ou sans `active`, le geste par défaut est l'activation (`attack_mode.enable`, « échoué », `422`).

`409 POST_NOT_RECOGNIZED` n'est rendu **que par cette route**, à un administrateur déjà authentifié. Une connexion bloquée par le mode attaque n'en reçoit jamais : elle reçoit le refus d'un mot de passe faux (le journal, lui, dit « mode attaque : poste non reconnu »).

Depuis HRT-28, cette route accepte **aussi** le contrat commun des actes (membre `reauth`, usage `0x05`, voir « Confirmation des actes d'administration » ci-dessous) : la clé exigée est la même (une clé inscrite du compte), les réponses sont celles ci-dessus. La forme à plat (usage `0x03`) reste acceptée pour les clients livrés.

## Confirmation des actes d'administration (HRT-28, ADR-0031, ADR-0032)

Tout acte d'administration (créer un compte, changer un rôle, le mot de passe d'un compte, supprimer un compte, fermer ses sessions, lancer la mise à jour de l'agent, activer ou désactiver le mode attaque, changer son propre mot de passe, régler la fréquence du mot de passe) accepte un membre `reauth` dans son corps :

```json
{ "reauth": { "password": "…", "device": { "algorithm": "ed25519", "public_key": "…", "challenge": "…", "signature": "…" } } }
```

- **Preuve** : `POST /sessions/challenge` avec `purpose: "admin_act"`, puis signature d'usage `0x05` liée à l'acte (code, cible = identifiant technique du compte visé, paramètres non secrets : compte et rôle d'une création, rôle d'un changement, version et somme d'une mise à jour, valeur du réglage), au compte, au hachage du jeton, au serveur et au défi. Octets : `hearth_proto::device_proof` et `hearth_proto::admin_act`. L'agent reconstruit l'acte depuis la requête.
- **Mot de passe** : par le chemin de la connexion (mêmes compteurs et ralentissement). Absent sous élévation pour un acte couvert ; **un mot de passe fourni pour un acte couvert sous élévation n'est pas vérifié** (la preuve de clé suffit) (créer un compte en lecture seule, passer un compte en lecture seule, supprimer un compte, fermer les sessions d'un compte).
- **Ordre** : session et rôle, preuve (aucun mot de passe n'est essayé tant qu'elle n'est pas valable), mot de passe ou élévation, acte, puis le défi est consommé avant l'effet de l'acte (un défi qui ne peut pas être retenu refuse l'acte : `proof_invalid`).
- **Tant que `admin_reauth.required` est faux**, un acte sans `reauth` passe comme avant ; un acte avec `reauth` est vérifié.

| Statut | Code | `details` | Quand |
|---|---|---|---|
| 409 | `POST_NOT_RECOGNIZED` | `field: "reauth.device"`, `reason: "proof_missing"` | `reauth` présent sans preuve lisible |
| 409 | `POST_NOT_RECOGNIZED` | `field: "reauth.device"`, `reason: "proof_invalid"` | preuve d'un autre acte, d'une autre cible, d'un autre compte, d'une autre session, périmée, rejouée, clé non inscrite pour le compte |
| 409 | `POST_NOT_RECOGNIZED` | `field: "reauth.password"`, `reason: "password_required"` | élévation absente ou fermée, ou acte non couvert, et mot de passe absent |
| 422 | `WRONG_PASSWORD` | | mot de passe faux (compté comme un échec de connexion) |
| 429 | `TOO_MANY_ATTEMPTS` | `retry_after_s` | attente du chemin de la connexion |
| 426 | `INCOMPATIBLE_VERSION` | `upgrade: "client"`, `reason: "reauth_required"` | acte sans `reauth` quand l'agent l'exige (HRT-30) ; toujours pour `PUT /me/reauth` |

Les codes propres à chaque acte (`USERNAME_TAKEN`, `LAST_ADMIN`, `WEAK_PASSWORD`, `BAD_SIGNATURE`, `MANAGED_INSTALL`, `OPERATION_IN_PROGRESS`, `NOT_FOUND`) ne sont rendus qu'après une confirmation réussie. Le retrait d'un poste garde son contrat et ses réponses (voir [postes de confiance](./devices.md)).

## `PUT /api/v1/me/reauth` : le réglage de fréquence du mot de passe

Tout rôle, suivie par clé d'opération. Corps : `{ "password": "window" | "each", "reauth": { … } }` (`password` porte le **réglage** ; le mot de passe de confirmation est dans `reauth`). **Toujours confirmé** (mot de passe et preuve de clé, jamais l'élévation) : acte `0x0B`. `200` : l'objet `admin_reauth`. Passer à `each` ferme les élévations du compte. Journal : `reauth.setting`.

## Ce que le mode attaque change pour les autres routes

Rien dans leur contrat. En mode attaque, une session valide présentée **seule** (adresse non retenue pour le compte, aucune preuve de clé) reçoit `401 SESSION_EXPIRED`, mot pour mot comme une session expirée, sur toutes les routes authentifiées (BR-TRUST-013) ; elle n'est pas détruite. Une connexion par mot de passe d'un poste non reconnu reçoit le refus d'un mot de passe faux (`401 INVALID_CREDENTIALS` ou `429`), jamais un code propre au mode (BR-TRUST-017).

## Journal

`security.alert` (« Attaque probable signalée ») : une entrée au début de l'épisode (cible « début de l'alerte », refusé « trop de tentatives, attente de N s », origine = la tentative qui l'a ouvert) et une à sa fin (cible « fin de l'alerte (levée par l'agent) », réussi, **origine « système »**). Le compte est celui de l'identifiant visé ; **rien** pour un identifiant inexistant.

Mode attaque (HRT-25), voir [journal](./audit.md) : `attack_mode.enable`, `attack_mode.disable`, `attack_mode.auto_disable`, `attack_mode.suspend`, `attack_mode.resume`, `attack_mode.trial`, `session.refused`.

## Côté client (HRT-26)

Le client lit le message de flux `security` (jamais un `ServerMessage`), relit `GET /security` à chaque retour du lien et au chargement de la page Sécurité, et appelle `PUT /security/attack-mode` par la commande typée `set_attack_mode(server_id, active, password)` : la liaison demande le défi `attack_mode`, signe avec la clé du coffre (usage `0x03`, liée au jeton et au geste) et joint le mot de passe. Sans clé au coffre, rien n'est envoyé (ni défi ni écriture). Le client ne déduit jamais le geste d'une entrée de journal. ADR-0029, BR-TRUST-009, 010, 029, 033.
