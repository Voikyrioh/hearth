# Moteur de conteneurs et services (services publiés), journaux en direct, démarrer, arrêter, redémarrer

Routes `GET /api/v1/services`, `POST /api/v1/engine/recheck`, `GET /api/v1/services/{key}/env`, `GET /api/v1/services/{key}/logs` (WebSocket des journaux, HRT-66), `GET /api/v1/services/external`, `POST /api/v1/services/adopt` et `POST /api/v1/services/{key}/release` (adopter et relâcher, HRT-80), `POST /api/v1/services/{key}/start`, `/stop` et `/restart` (démarrer, arrêter, redémarrer, HRT-64), et sujet `services` du flux `/stream`. Tranches 1 et 2 des déploiements (HRT-61, HRT-63) : l'agent détecte le moteur de conteneurs (Docker) et rend la liste de ses conteneurs, avec leur détail, tenue à jour en direct ; les services lancés par une unité systemd sont reconnus (clé `u_`). **Seuls trois gestes touchent un service** (HRT-64) : démarrer, arrêter, redémarrer (sections ci-dessous) ; l'agent ne crée, ne modifie ni ne supprime rien d'autre dans le moteur, ni dans une unité. Ses autres écritures sont sa propre mémoire : les services adoptés (`adopted_services`, migration `0011`, écrite par les deux actes d'adoption) et la mémoire des services supervisés adoptés (`supervised_services`, migration `0009`).

**Publication (HRT-79, ADR-0043)** : la liste, le flux et les routes par clé ne portent que les services CRÉÉS PAR HEARTH (clé `h_`) et les services ADOPTÉS (table `adopted_services`, migration `0011`). Les autres conteneurs du serveur sont comptés (`engine.external_count`), jamais détaillés. Sur un serveur dont tous les conteneurs sont hors Hearth, `services` est vide et `external_count` vaut leur nombre, jusqu'à ce qu'un compte qui a le droit adopte (HRT-80, sections ci-dessous : lire les candidats, adopter, relâcher).

- **Authentification** : session valable (Bearer), comme les autres lectures.
- **Rôle requis** : aucun rôle particulier pour `GET /services`, `POST /engine/recheck` et le flux : les deux rôles (Administrateur, Lecture seule) reçoivent exactement la même chose (BR-ENGINE-016, BR-SVC-020). `GET /services/{key}/env` exige le droit « gérer les services », quel que soit le rôle (BR-SVC-009, BR-GRANT-003, ADR-0041) : niveau d'accès `services`, qu'aucun rôle n'ouvre à lui seul. `GET /services/external`, `POST /services/adopt` et `POST /services/{key}/release` exigent le même droit (niveau `services`), plus la confirmation d'un acte d'administration pour les deux `POST` (voir leurs sections). `GET /services/{key}/logs` (socket) : aucun niveau d'accès de route, le droit et l'adoption sont jugés par `may_read` (voir sa section) ; le rôle n'y entre pas.
- **Journal** : aucune entrée pour la lecture de la liste ni pour la relance de la détection (BR-ENGINE-020, BR-SVC-024). `GET /services/{key}/env` : le refus d'un compte sans le droit (cause « droits insuffisants ») et chaque réussite sont consignés (`service.env.reveal`), sans nom ni valeur. Adopter, relâcher, le refus de lire les candidats et le refus de lire les journaux d'un adopté sont consignés (`service.adopt`, `service.release`, `service.external.read`, `service.logs.read`, voir [audit](./audit.md) et les sections ci-dessous) ; la lecture des candidats qui réussit l'est aussi (sans aucun nom).
- **Capacité** : `GET /hello` annonce `capabilities: ["services", "service-rights", "service-logs", "service-adoption", "service-ops"]` (`service-ops` : lire, décider, annuler et suivre les opérations de service, voir [service-ops](./service-ops.md), HRT-144 ; `service-logs` : journaux en direct, voir plus bas ; `service-adoption` : l'agent ne publie que les services créés par Hearth et les adoptés, et compte les autres, et il connaît les trois routes d'adoption (HRT-80) ; elle ne dit pas que l'adoption est possible sur ce serveur : `services.adoption = false` la refuse par `409`, voir plus bas). Un agent qui n'annonce pas `service-adoption` publie tous les conteneurs. Un client n'envoie le sujet `services` ni n'appelle ces routes que si la capacité est annoncée : un sujet inconnu fait refuser TOUT le message `subscribe`, `metrics` compris (voir [stream](./stream.md)). Un agent ancien n'a pas le champ (lu comme une liste vide), un client ancien l'ignore. `API_VERSION` reste à 1 (ADR-0033, ADR-0038).
- **Code** : `entrypoint/http/services.rs`, `entrypoint/services_wire.rs` (conversions), `application/services.rs` (registre), `domain/services/`. Types : `hearth-proto::api::services`, `hearth-proto::stream::ServicesMessage`.
- **Valeurs absentes du fil** : aucune VALEUR de variable d'environnement dans `GET /services` ni dans le flux, pour aucun rôle (ADR-0038). Les NOMS des variables et la commande de lancement sont dans le détail (`env_names`, `command`, BR-SVC-007, BR-SVC-008) pour les deux rôles quand le service est créé par Hearth (BR-ADOPT-016) ; pour un service ADOPTÉ ils ne sont donnés qu'aux comptes qui ont le droit « gérer les services » (BR-ADOPT-015) : aux autres, `env_names` et `command` sont absents et `details_reserved` vaut `true`. Le contenu d'un message dépend donc du droit du compte qui le lit (`GET /services` : le compte de la requête ; flux : le droit de la session, relu à chaque contrôle de session, et un instantané complet repart quand il change). Ils ne vont dans aucune trace de l'agent.

## `GET /api/v1/services`

Le même contenu que le `services_snapshot` du flux (repli et tests). `200` :

```json
{
  "rev": 7,
  "engine": {
    "state": "ready",
    "name": "Docker Engine - Community",
    "version": "24.0.7",
    "min_version": "20.10",
    "agent_account": "root",
    "agent_is_admin": true,
    "host": { "os_id": "debian", "os_like": [], "init": "systemd", "declarative": false },
    "last_ready_at": "2026-10-13T10:00:00Z",
    "checked_at": "2026-10-13T10:00:02Z",
    "list_read_at": "2026-10-13T10:00:00Z",
    "at": "2026-10-13T10:00:02Z",
    "list_current_at": "2026-10-13T10:00:02Z",
    "external_count": 2
  },
  "services": [
    {
      "key": "c_9f...64 caractères...",
      "name": "web",
      "origin": "external",
      "removed_on_stop": false,
      "compose_project": "shop",
      "state": "running",
      "since": "2026-10-13T10:00:00.5Z",
      "created_at": "2026-10-13T09:00:00.1Z",
      "never_started": false,
      "restarts_10m": 1,
      "image": { "ref": "nginx", "tag": "1.27", "short_id": "0123456789ab" },
      "ports": [{ "host_port": 8080, "container_port": 80, "protocol": "tcp", "scope": "all" }],
      "host_network": false,
      "mounts": [{ "kind": "folder", "source": "/srv/data", "target": "/data", "read_only": true }],
      "restart": "unless_stopped",
      "env_names": ["APP_MODE", "DB_PASSWORD"],
      "command": ["/entrypoint.sh", "--serve"],
      "grace_s": 30
    },
    {
      "key": "u_docker-terraria.service",
      "name": "terraria",
      "origin": "external",
      "driver": "supervisor",
      "unit": "docker-terraria.service",
      "removed_on_stop": true,
      "state": "stopped",
      "since": "2026-10-13T11:00:00Z",
      "never_started": false,
      "restarts_10m": 0,
      "image": { "ref": "ghcr.io/example/terraria", "tag": "1.4", "short_id": "89abcdef0123" },
      "ports": [{ "host_port": 7777, "container_port": 7777, "protocol": "tcp", "scope": "all" }],
      "host_network": false,
      "mounts": [],
      "restart": "supervisor",
      "grace_s": 120
    }
  ]
}
```

Le second service est lancé par l'unité `docker-terraria.service` : son conteneur est supprimé à l'arrêt (`removed_on_stop`), il reste listé d'après l'état de l'unité, avec ses derniers réglages, sans `env_names` ni `command` (jamais retenus).

| Champ | Description |
|---|---|
| `rev` | Croît à chaque changement ; repart de zéro au redémarrage de l'agent. Un `rev` plus petit que le dernier vu vaut remise à zéro. |
| `engine.state` | `checking`, `ready`, `stopped`, `access_denied`, `unresponsive`, `absent`, `unsupported_engine`, `unsupported_version`, `disabled` (BR-ENGINE-002, 019). |
| `engine.name`, `version` | Annoncés par le moteur, seulement s'il a répondu (BR-ENGINE-003). |
| `engine.min_version` | `20.10` (API 1.41), BR-ENGINE-011. |
| `engine.agent_account`, `agent_is_admin` | Compte de l'agent (BR-ENGINE-008). |
| `engine.host` | Distribution (`ID`, `ID_LIKE` de `os-release`), système d'init, serveur déclaratif (NixOS) : pour l'aide (BR-ENGINE-005). |
| `engine.checked_at` | Date de la dernière SONDE du moteur. Elle ne date pas la liste. |
| `engine.list_read_at` | Facultatif. Date de la dernière lecture COMPLÈTE réussie de la liste (aucun conteneur illisible). Information seulement : elle ne dit pas depuis quand la liste est périmée (une liste suivie en direct par les événements est à jour sans relecture). Absente tant qu'aucune n'a réussi depuis que la liste a été vidée. **Le client n'en tire pas l'âge** : voir `list_current_at`. |
| `engine.list_degraded` | Facultatif, absent vaut `false`. `true` quand la liste affichée est peut-être périmée alors que le moteur répond : la dernière relecture a échoué, des conteneurs sont illisibles (ils gardent leur dernière description, par exemple « en marche » alors qu'ils sont tombés), le suivi en direct est suspendu (flux d'événements qui casse en boucle, jusqu'à 60 s), ou la liste n'a pas encore été relue depuis que le moteur a démarré ou est revenu (jusqu'à 30 s sur un moteur lent ; l'ancienne liste d'un moteur absent n'est jamais renvoyée, elle repart vide, même si le moteur a disparu et reparu dans la fenêtre d'anti-clignotement). Une coupure retenue par l'anti-clignotement ne publie pas de marque. Le client dit alors « Impossible de lire les services pour le moment » et date la liste avec `list_current_at`. Sans sens dans les états sans liste (absent, non pris en charge, désactivé) ; jamais posé pour la seule raison « pas relue » dans un état inaccessible (la liste y est déjà grisée par l'état). Un agent plus ancien n'envoie ni l'un ni l'autre ; un client plus ancien les ignore (BR-SVC-001, BR-SVC-018). |
| `engine.at` | Facultatif (HRT-72). Heure de l'agent au moment où cette vue a été fabriquée. Sa présence dit que l'agent connaît `list_current_at`. |
| `engine.list_current_at` | Facultatif (HRT-72). Dernier moment où la liste était à jour : `at` quand elle l'est maintenant (sonde qui répond, liste relue et suivie sans réserve) ; sinon l'instant où l'agent a vu qu'elle cessait de l'être (lecture en échec, conteneur illisible, suivi suspendu, moteur plus joignable). Une liste suivie en direct reste « à jour » sans relecture. **Âge à afficher** (« Liste du moteur d'il y a {âge} ») : `max(0, at - list_current_at)`, deux dates de l'agent donc sans écart d'horloge avec le client, PLUS le temps écoulé chez le client depuis la réception du message, mesuré par l'horloge du client (même principe que « Vu il y a » : on part de l'instant de réception, jamais d'une date de l'agent comparée à l'horloge du client). Un message vient à chaque changement d'état ou de marque : l'âge ne se recalcule pas sans nouveau message. `at` n'est pas posé pour une liste non relue depuis que le moteur a été vu non prêt, quel que soit l'état publié (limite connue : BR-SVC-018). Lecture : `at` présent et `list_current_at` absent = l'agent n'a aucune liste (« jamais lue », ou vidée dans un état sans liste : le client qui garde sa copie la date depuis sa propre réception) ; `at` absent = agent plus ancien, date inconnue. Une horloge de l'agent qui recule peut donner une date dans le futur : l'âge est borné à zéro. |
| `engine.external_count` | Facultatif, absent vaut 0 (HRT-79, BR-ADOPT-002). Nombre de conteneurs que le moteur connaît, arrêtés compris, qui ne sont ni créés par Hearth ni adoptés : un nombre, aucun détail, le même pour tous les comptes. Vaut 0 quand le moteur est absent, non pris en charge ou désactivé. |
| `services[]` | Les services PUBLIÉS (créés par Hearth ou adoptés, BR-ADOPT-001), arrêtés compris, triés par nom sans tenir compte de la casse (BR-SVC-001, BR-SVC-012). Liste vide quand le moteur est absent, non pris en charge, de version non prise en charge ou désactivé (BR-SVC-018) ; gardée telle quelle quand le moteur est arrêté, refuse l'accès ou ne répond pas. |
| `key` | Identité stable, opaque : `c_<identifiant complet du conteneur>`, ou `u_<unité>` (suffixe `.service` compris) pour un service lancé par une unité systemd, le même quel que soit le conteneur que le système recrée (ADR-0037, BR-SVC-016). `h_<ULID>` est réservé à la tranche 5. |
| `origin` | `external` pour un service adopté ; `hearth` pour une clé `h_` (que l'agent ne produit pas encore, BR-SVC-003). |
| `adopted` | Facultatif (HRT-79). `{ "by": compte, "at": date RFC 3339 }` pour un service adopté ; absent pour un service créé par Hearth. |
| `details_reserved` | Facultatif, absent vaut `false` (HRT-79, BR-ADOPT-015). `true` : le service est adopté et le compte qui lit n'a pas le droit « gérer les services » ; `env_names`, `env_names_omitted` et `command` sont retirés du message et leur absence ne veut pas dire « aucun ». |
| `state` | `running`, `stopped`, `error`, `restart_loop`, `paused`, `stopping`, `unknown` ; `starting` pour une unité qui démarre (HRT-63) ; `deploying` réservé à la tranche 5 (BR-SVC-004 à 006). Un service supervisé dont le conteneur n'existe pas prend l'état de son unité (BR-SVC-005) : `stopped`, `error` (échec constaté par le système), `restart_loop` (en attente de relance), `starting`, `stopping` ; `unknown` si le superviseur ne répond pas. |
| `driver` | `supervisor` (service lancé par une unité systemd) ou `engine`. Absent d'un agent de la tranche 1 : lu comme `engine`. |
| `unit` | Nom de l'unité (`docker-terraria.service`), pour un service supervisé seulement (BR-SVC-003, BR-SVC-016). |
| `removed_on_stop` | Le moteur supprime le conteneur à l'arrêt (`--rm`). |
| `compose_project` | Projet Compose, information seulement. |
| `env_names`, `env_names_omitted` | Noms des variables d'environnement, y compris celles de l'image (BR-SVC-008) ; jamais une valeur. 255 caractères par nom, 256 noms au plus, le surplus est compté dans `env_names_omitted`. Absents quand vides ; absents d'un agent de la tranche 1. Type `Concealed` : un simple texte JSON, `Debug` masqué côté Rust. |
| `command` | La commande lancée (programme puis arguments), 1024 caractères par élément, 64 éléments au plus (le dernier est alors « … »). Absente quand le moteur n'en annonce aucune (BR-SVC-007). Même type. Peut contenir un secret passé en argument : voir `SECURITY.md`. |
| `grace_s` | Délai d'arrêt du service, en secondes : celui de l'unité pour un service supervisé, sinon celui du conteneur, sinon 10 (BR-SVC-007). |
| `restarts_10m` | Démarrages non demandés des 10 dernières minutes, comptés par l'agent (BR-SVC-006). |
| `image.ref` | Absent pour une image sans nom (BR-SVC-022). |
| `busy`, `last_action` | Déclarés facultatifs pour les tranches suivantes, absents ici. |
| `restart` | `unless_stopped`, `always`, `on_failure`, `none` ; `supervisor` (relancé par le système du serveur) pour un service supervisé (HRT-63). |

**Valeurs inconnues.** Toute énumération que le client lit (`engine.state`, `host.init`, `state`, `origin`, `driver`, `protocol`, `scope`, `mounts[].kind`, `restart`) se lit avec une variante de repli (`unknown`, ou `other` pour `host.init`) : une valeur ajoutée par un agent plus récent ne fait pas perdre l'instantané.

**Bornes.** Textes nettoyés des caractères de contrôle et de formatage invisibles (renversement du sens d'écriture compris) et plafonnés, au plus 128 ports et 64 montages par service ; le surplus est compté dans `ports_omitted` et `mounts_omitted` (facultatifs, absents quand 0 : « et N autres ») (BR-SVC-021).

**Tout ajout de la tranche 2 est facultatif et tolère l'absence** (`serde(default)`) : un client de la tranche 1 lit un agent de la tranche 2 (il ignore les champs qu'il ne connaît pas), un client de la tranche 2 lit un agent de la tranche 1 (listes vides, `driver` absent).

Erreurs : celles de toute route authentifiée (`401`, `426`).

## `GET /api/v1/services/{key}/env`

Révèle les variables d'environnement d'un service avec leurs VALEURS (BR-SVC-009). **Réservée aux comptes qui ont le droit « gérer les services »** (HRT-76, BR-GRANT-003), administrateurs ou non : la couche d'accès (`Access::Services`, `auth::allows`) relit le droit en base à chaque requête et refuse un compte qui ne l'a pas, quel que soit son rôle, `403 FORBIDDEN_ROLE` avec `details.missing: "services"` et « Tu n'as pas le droit de gérer les services de ce serveur. » (BR-GRANT-004), avant le handler, avant la lecture de `{key}` et avant toute confirmation, et consigne le refus ; une session absente ou inconnue répond `401` sans journal. Aucune clé d'opération, aucun mot de passe : ce n'est pas un acte d'administration du socle. `{key}` est la clé d'un service de la liste (`c_…` ou `u_…`).

`200`, avec `Cache-Control: no-store` et `Pragma: no-cache` :

```json
{ "variables": [ { "name": "APP_MODE", "value": "prod" }, { "name": "DB_PASSWORD", "value": "…" } ] }
```

- Variables dans l'ordre du moteur, au plus 256 (comme `env_names`), valeurs non modifiées (un certificat s'étend sur plusieurs lignes), une entrée sans `=` a une valeur vide.
- Les valeurs ne sont lues dans le moteur (`ContainerEngine::env`) qu'à cette demande ; aucune relecture de la liste, aucun événement, aucun abonnement ne les lit.
- **Journal** : une entrée `service.env.reveal` (« Valeurs des variables révélées », cible « service {nom} », résultat réussi) est écrite AVANT que les valeurs partent, une entrée par révélation (sans regroupement) ; si l'écriture échoue, aucune valeur n'est rendue : `503 BUSY`, texte fixe, sans détail (BR-SVC-009). Le refus faute du droit est consigné sous le même code (« Refusé », cause « droits insuffisants », cible : la route, jamais la clé demandée). Jamais un nom ni une valeur de variable.
- Un compte sans le droit reçoit EXACTEMENT la même réponse `403` pour un service connu, inconnu ou une clé mal formée : le droit est jugé avant la clé (aucun oracle d'existence). Le pilotage désactivé (`[engine] enabled = false`) l'emporte sur le droit : `404`, le moteur n'est pas ouvert (BR-GRANT-015). Aucune route de ce fichier ne donne ni ne retire le droit (BR-GRANT-005).
- Erreurs : `403 FORBIDDEN_ROLE` (`details.missing: "services"`) sans le droit ; `404 NOT_FOUND` clé mal formée, service inconnu OU conteneur non adopté (« Ce service n'existe pas. », une seule et même réponse pour les trois : code, corps, chemin de recherche, BR-ADOPT-004) ; `404` service supervisé listé d'après son unité mais sans conteneur (« Les variables de ce service ne sont pas lisibles : son conteneur n'existe pas. ») ; `503 BUSY` moteur muet ; `503 BUSY` journal non écrit (aucune valeur rendue). La clé n'est jamais répétée dans une erreur.
- Une réponse est un secret : jamais stockée, jamais tracée (test sentinelle au niveau de trace le plus bavard).

## `GET /api/v1/services/external` (HRT-80)

Les conteneurs que ce compte peut choisir d'adopter (BR-ADOPT-005, BR-ADOPT-006). **Réservée aux comptes qui ont le droit « gérer les services »**, quel que soit le rôle : même refus que `GET /services/{key}/env` (`403 FORBIDDEN_ROLE`, `details.missing: "services"`), consigné sous `service.external.read` avec pour cible la route. Une lecture qui réussit est consignée aussi (`service.external.read`, résultat « ok », cible : la route, aucun nom ni clé de conteneur), regroupée par compte par le regroupement des répétitions du journal. Elle n'est jamais diffusée par le flux, jamais gardée par l'agent (recalculée à chaque appel) et jamais mise en cache (`Cache-Control: no-store`). À appeler à l'ouverture de la fenêtre d'adoption, jamais en arrière-plan.

`200` :

```json
{
  "engine": "ready",
  "containers": [
    { "key": "c_9f...64 caractères...", "name": "Zeta", "image": { "ref": "busybox", "tag": "1.36", "short_id": "0123456789ab" }, "state": "stopped" },
    { "key": "u_docker-terraria.service", "name": "terraria", "image": { "ref": "ghcr.io/example/terraria", "tag": "1.4", "short_id": "89abcdef0123" }, "unit": "docker-terraria.service", "state": "running" }
  ]
}
```

- **Ce qui est exposé par conteneur, et pourquoi c'est le minimum** : `key` (ce que l'acte enverra et signera ; pour un conteneur SANS unité elle contient son identifiant complet, donné aux seuls comptes qui ont le droit), `name` et `image` (de quoi choisir), `unit` (absente sans unité : l'adoption ne tient alors que pour la vie du conteneur, BR-ADOPT-017), `state` (BR-SVC-004). Ni variable, ni commande, ni port, ni dossier monté, ni étiquette, ni date, ni journal : le détail d'un conteneur n'existe pour le client qu'une fois adopté, et alors réservé au droit (BR-ADOPT-015).
- Triés par nom sans tenir compte de la casse. Exclus : les services créés par Hearth, les adoptés, un service lancé par une unité dont le conteneur n'existe pas (il devient candidat dès qu'il a tourné). `engine` : l'état du moteur ; hors de `ready` la liste est vide et c'est l'état qui l'explique (BR-ADOPT-026).
- **Pas de trace, pas de réponse** (BR-ADOPT-005) : la lecture est consignée avant d'être rendue ; si l'entrée admise ne peut pas s'écrire, `503 BUSY` (« La lecture n'a pas pu être consignée au journal : aucun conteneur n'est rendu. Réessaie dans un instant. ») et aucun conteneur n'est rendu.
- Un candidat dont la clé est `u_<unité>` peut être une adoption « à confirmer » ou « pas encore relue depuis le démarrage » (BR-ADOPT-027) : la réponse ne la distingue pas, l'acte d'adoption sur sa clé la confirme. Un compte qui a le droit peut aussi la relâcher (`POST /services/{key}/release`).
- Erreurs : `403` sans le droit ; `401` ; `503 BUSY` (trace non écrite) ; `409 CONFLICT` avec `details.reason: "adoption_disabled"` quand le serveur a désactivé l'adoption (BR-ADOPT-020).

## `POST /api/v1/services/adopt` (HRT-80)

Adopte des conteneurs existants (BR-ADOPT-007 à BR-ADOPT-010). **Acte d'administration du socle**, code d'acte `0x11` : le droit « gérer les services » (niveau `services`, refus consigné), puis la confirmation complète : preuve de la clé d'un poste inscrit puis mot de passe, **toujours**, jamais couverte par l'élévation de cinq minutes (BR-ADOPT-008). La preuve signe le NOMBRE de clés et l'EMPREINTE SHA-256 de leurs clés triées, reconstruits par l'agent depuis le corps (`hearth_proto::admin_act::AdminAct::services_adopt`) : une liste différente de celle qui a été confirmée est refusée (`409 POST_NOT_RECOGNIZED`, `details.reason: "proof_invalid"`). Suivie par clé d'opération (`Idempotency-Key`) : l'acte va au bout si le lien coupe, et le rejouer avec la même clé rend le premier résultat (BR-ADOPT-025).

Corps : `{ "keys": ["c_…", "u_…"], "reauth": { … } }` — 1 à 50 clés (`MAX_ADOPTIONS`), sans doublon. `200` :

```json
{ "adopted": 1, "results": [
  { "key": "c_…", "result": "adopted" },
  { "key": "c_…", "result": "already_adopted", "by": "marie" },
  { "key": "c_…", "result": "not_found" } ] }
```

- `result` : `adopted`, `already_adopted` (avec `by`, déjà publié à tous), `not_found` (disparu depuis la liste, jamais existé, mal formé, `h_`, ou clé `c_` du conteneur d'un service lancé par une unité : l'agent ne distingue jamais ces cas, BR-ADOPT-004), `failed` (rien n'est adopté pour cette clé), et toute valeur inconnue se lit comme non adopté. Un résultat par clé, dans l'ordre de la demande.
- **Rien n'est écrit sur le service** (BR-ADOPT-009) : une ligne dans la base de l'agent, aucune étiquette, aucun appel d'écriture au moteur. Le service apparaît dans la liste de tous (`adopted: { by, at }`), son détail réservé au droit, et n'est plus compté.
- Identité : la clé est celle que la liste des candidats a donnée (`u_<unité>`, sinon `c_<identifiant>`), jamais un nom. Un conteneur sans unité recréé par un tiers redevient non géré (BR-ADOPT-017). Deux comptes qui adoptent la même clé en même temps : une seule adoption (BR-ADOPT-024).
- **Journal** : une entrée `service.adopt` par service (« Service adopté », cible « service {nom} ({unité}) », compte, origine), écrite dans la transaction de la ligne et avant la réponse ; les issues sans adoption sont consignées « Échoué » (« déjà adopté » avec le nom ; « service introuvable » avec pour cible la route `/services/adopt`, UNE entrée par acte avec le nombre des autres clés de même issue (`repeat_count`, rendu « N autres fois ») ; voir BR-ADOPT-019, la même entrée pour une clé cachée, disparue ou inventée). Un refus ne révèle jamais le nom d'un conteneur (BR-ADOPT-019).
- Erreurs : `403` sans le droit ; `426 INCOMPATIBLE_VERSION` sans `reauth` ; `409 POST_NOT_RECOGNIZED` preuve ou mot de passe (`details.reason` : `proof_missing`, `proof_invalid`, `password_required`) ; `422 VALIDATION_ERROR` (`details.field: "keys"` et `details.reason` : `keys_empty`, `keys_too_many`, `keys_duplicate` ; « Tu peux adopter 50 services à la fois. ») ; `409 CONFLICT` avec `details.reason: "adoption_disabled"` (texte de BR-ADOPT-020) ou `"engine_not_ready"` (avec `details.engine`, BR-ADOPT-026).

## `POST /api/v1/services/{key}/release` (HRT-80)

« Ne plus gérer ce service » (BR-ADOPT-013). **Acte d'administration du socle**, code d'acte `0x12`, cible signée : la clé du chemin. Le droit est exigé ; la confirmation est celle du socle, **couverte par l'élévation de cinq minutes** (il retire un accès, il n'en donne pas). Une `{key}` avec des caractères hors `A-Z a-z 0-9 _ - . : @` s'écrit encodée en pourcentage ; l'agent la décode comme le routeur avant de comparer la cible signée. Suivie par clé d'opération.

Clé du chemin : un segment `{key}` fait de lettres, chiffres et `_ - . : @`, avec l'encodage en pourcentage pour le reste ; une unité au nom échappé par le système porte une barre oblique inverse, qui voyage en `%5C` (jamais nue : les URL la lisent comme `/`). L'agent et la liaison cliente décodent le segment avec la même fonction (`hearth_proto::admin_act::decode_segment`, qui n'accepte après un `%` que DEUX chiffres hexadécimaux : `%+f`, `% 5` et `%%41` sont refusés des deux côtés) APRÈS l'avoir découpé : `/`, `..` ou `%2f` ne sortent pas du segment, ils donnent une clé invalide (`released: false` côté agent, pas un acte côté liaison). La preuve signe la clé décodée.

Corps : `{ "reauth": { … } }`. `200` : `{ "released": true }` ; `{ "released": false }` quand le service n'était pas adopté, **quelle qu'en soit la raison** (déjà relâché, jamais adopté, caché, inconnu, mal formé, `h_`) : la même réponse, sans effet, sans entrée de journal (aucun oracle sur ce que le serveur fait tourner).

- Le service sort de la liste de tous à la publication suivante et est de nouveau compté ; les lectures de journaux ouvertes sur lui se ferment (au plus 2 s) ; **le conteneur et son unité ne sont pas touchés**.
- **Journal** : `service.release` (« Adoption retirée », cible « service {nom} ({unité}) »), écrit dans la transaction de l'effacement de la ligne.
- Une adoption « à confirmer » ou « pas encore relue depuis le démarrage » (BR-ADOPT-027) se relâche comme toute autre : l'acte ne lit que la base, répond `released: true` et est consigné. C'est le geste qui retire des droits, il reste toujours possible au compte qui a le droit.
- Possible quand l'adoption est désactivée (BR-ADOPT-020) et quand le moteur n'est pas prêt (BR-ADOPT-026). Le refus tant qu'une action de service est en cours (règle d'une action déjà en cours sur le service, spec du cycle de vie) viendra avec les actions de service.
- Erreurs : `403` sans le droit ; `426`, `409 POST_NOT_RECOGNIZED` comme tout acte.

## `POST /api/v1/services/{key}/start`, `/stop` et `/restart` (HRT-64)

Démarrer, arrêter, redémarrer un service publié (créé par Hearth ou adopté ; un conteneur non adopté n'est pas pilotable, BR-LIFE-024). **Trois actes d'administration du socle**, codes d'acte `0x0C`, `0x0D`, `0x0E`, cible signée : la clé du chemin (décodée comme pour `release`), aucun paramètre ; **couverts par l'élévation de cinq minutes** (BR-LIFE-016). Le droit « gérer les services » est exigé par le niveau d'accès (`403 FORBIDDEN_ROLE`, `details.missing = "services"`, « Tu n'as pas le droit de gérer les services de ce serveur. », BR-LIFE-001). Suivies par clé d'opération. Corps : `{ "reauth": { … } }`.

- **Réponse `202`** : `{ "action_id": "…", "grace_s": 10 }`. `action_id` est l'identifiant de l'opération (`GET /operations/services/{id}`, voir [service-ops](./service-ops.md) ; `ServiceItem.busy` et `last_action` du service la projettent) ; `grace_s` est le délai d'arrêt du service en secondes (celui de l'unité pour un service supervisé, sinon celui du conteneur, sinon 10, BR-LIFE-009). **L'issue ne se lit jamais dans cette réponse** : l'ordre est donné à l'arrière-plan, l'attente est `max(grace_s + 30 s, 60 s)` (BR-LIFE-021).
- **Refus synchrones et typés** (rien n'est ordonné, rien n'est modifié ; chacun est au journal d'activité avec sa cause) : `503 ENGINE_UNAVAILABLE` (`details.engine`, moteur pas prêt, BR-LIFE-019) ; `404 SERVICE_NOT_FOUND` « Ce service n'existe plus. » (clé inconnue, mal formée ou non publiée : même réponse, BR-LIFE-015) ; `409 SERVICE_BUSY` (`details.by` le compte, `details.kind` le genre de l'action en cours, BR-LIFE-013) ; `409 SERVICE_ALREADY_IN_STATE` (`details.state`, démarrer un service qui tourne, arrêter un service arrêté, BR-LIFE-014) ; `409 SERVICE_NOT_DRIVABLE` (`details.state`, en pause, en arrêt, non reconnu, ou redémarrer un service qui ne tourne pas : jamais converti en démarrage). Ordre des contrôles : droit, confirmation, moteur, service, action en cours, état.
- **Pilote** : un service dont la clé est `u_<unité>` est piloté par le système (`systemctl start|stop|restart -- <unité>`), jamais par le moteur ; les autres par l'API du moteur (BR-LIFE-002, ADR-0037). Le nom d'unité est validé et passé en argument, jamais par un interpréteur.
- **Arrêter** : l'arrêt voulu est écrit en base AVANT l'ordre (BR-LIFE-010) ; l'ordre part avec le délai de grâce ; un arrêt forcé (code 137 après le délai) est réussi avec `forced = true` et la cause `forced_stop` (BR-LIFE-009). **Démarrer** : « En marche » vient du moteur (BR-LIFE-004). **Redémarrer** : un seul ordre ; un démarrage qui échoue après l'arrêt laisse le service « Arrêté » avec la cause (BR-LIFE-012). Pendant l'action, l'état publié est « Arrêt en cours » ou « Démarrage » (BR-SVC-004).
- **Issue** (opération) : `succeeded` ; `failed` avec `port_in_use`, `engine_refused`, `engine_unavailable` (pilote absent), `service_not_found`, `step_failed` ; `interrupted` avec `timeout` ou `engine_unavailable` (résultat inconnu, jamais rejoué, BR-LIFE-019, BR-LIFE-021) ou `interrupted` (l'agent a redémarré en cours d'action, BR-LIFE-020). Après coup, sur une réussite sans cause : `stopped_right_after_start` (le service s'est arrêté dans les 10 s qui ont suivi son lancement, BR-LIFE-005) et `restarted_by_host` (le service arrêté est reparti seul dans les 30 s, BR-LIFE-011) ; ces surveillances ne bloquent aucune autre action.
- **Journal d'activité** : `service.start`, `service.stop`, `service.restart` (« Démarrage d'un service », …), cible « service {nom} » (+ « (arrêt forcé) »), résultat et cause typée de la liste fermée (BR-LIFE-018) : « arrêt forcé », « port occupé », « arrêté juste après le démarrage », « moteur inaccessible », « service introuvable », « droits insuffisants », « action déjà en cours », « déjà dans l'état demandé », « état du service incompatible avec l'action », « repart tout seul », « délai dépassé », « interrompu par un redémarrage de l'agent », « refusé par le moteur ». Jamais un nom de variable, une commande, un texte du moteur.
- Erreurs communes à tout acte : `426 INCOMPATIBLE_VERSION` sans `reauth` ; `409 POST_NOT_RECOGNIZED` preuve ou mot de passe (`details.reason`).
- **Code** : `entrypoint/http/services.rs::{start, stop, restart}`, `application/service_lifecycle.rs`, `domain/ops/lifecycle.rs`. Types : `hearth_proto::api::services::ActionAccepted`.

## `POST /api/v1/engine/recheck`

Relance la détection du moteur tout de suite (« Vérifier à nouveau ») et rend `engine` (la forme de `EngineView` ci-dessus). C'est un `POST` qui **ne modifie rien** : il est dans `READ_ONLY_POSTS`, ni acte d'administration ni exception nommée, sans clé d'opération ni confirmation. Une demande trop rapprochée de la sonde précédente (moins d'1 seconde), ou pendant qu'une sonde est déjà en cours, rend l'état courant sans sonder. Ouvert aux deux rôles. Le résultat est publié tout de suite (hors de l'anti-clignotement).

## `GET /api/v1/services/{key}/logs` (WebSocket, HRT-66)

Les journaux d'un service en direct : **un WebSocket par lecture** (jamais le flux commun `/stream` : à 1 000 lignes par seconde, un lecteur lent ferait fermer tout le flux, donc le lien). Authentifié par son premier message comme `/stream` (`Access::FirstMessage`) ; **qui lit quoi** (HRT-79, `may_read`, BR-SLOG-011, BR-SLOG-018) : un service créé par Hearth, tous les comptes ; un service ADOPTÉ, les seuls comptes qui ont le droit de gérer les services (quel que soit le rôle) : les autres reçoivent un refus EXPLICITE, `error { MISSING_RIGHT }` puis la fermeture 1000, consigné au journal d'activité (`service.logs.read`, « Journaux lus », une fois par lecture refusée, cible le nom du service, déjà dans leur liste ; BR-SLOG-024) ; un conteneur ni créé par Hearth ni adopté, personne : sa clé reçoit exactement la réponse d'une clé inconnue (`error { SERVICE_NOT_FOUND }`, fermeture 1000, aucun appel au moteur), y compris à la limite de dix lectures (BR-ADOPT-004) ; `X-Hearth-Api` sur l'ouverture ; capacité `service-logs` annoncée par `/hello`. Lire n'est pas journalisé (BR-SLOG-022), le refus ci-dessus mis à part. Aucun oracle : `MISSING_RIGHT` n'est rendu que pour un service publié ; un conteneur caché, non adopté ou inconnu reçoit exactement la même séquence `SERVICE_NOT_FOUND` et n'écrit rien. Une clé qui n'a aucune forme de clé de service (`c_`, `h_`, `u_`) est refusée avant la mise à niveau : `404 NOT_FOUND`. Types : `hearth_proto::api::service_logs`. Client (HRT-67) : `hearth-link` ouvre ce socket (`Transport::open_service_logs`, `LinkManager::open_service_logs`), se reprend seul après une coupure par le curseur, traite tout refus comme une fin (jamais retenté) et rend `retry_after_s` à l'interface.

**Déroulement** : `auth` (même forme que `/stream`, jeton et preuve de clé d'appareil facultative) dans les 5 secondes, puis `open`, puis les messages ci-dessous. `ping { n }` toutes les 2 s (`pong { n }`) ; 30 s de silence ferment le socket. Fermer le socket (proprement ou non) arrête la lecture côté serveur et rend la place.

Client vers agent (après `auth`) :
- `open { tail?: 1000, since?: { ts, rank } }` : `tail` dernières lignes (plafonné à 1 000, défaut 1 000) ; `tail` compte des MESSAGES du moteur (une écriture du programme), pas des lignes : un message qui contient des retours chariot donne plusieurs lignes, la première page peut donc dépasser `tail` ; `since` : reprise après une coupure, à la place des dernières lignes (BR-SLOG-014), jamais crue sur parole. Un seul `open` par socket.
- `ping { n }`.

Agent vers client :
- `status { state: "live" | "ended" | "waiting", container?, mixed, exit_code?, ended_at?, not_kept? }` : à l'ouverture puis à chaque changement. `live` : le service tourne ; `ended` : arrêté (les dernières lignes que le moteur a gardées suivent, BR-SLOG-008) ; `waiting` : pas de conteneur pour l'instant, la lecture repart quand il revient ; `not_kept: true` : conteneur supprimé à chaque arrêt par le système, le moteur n'a gardé aucune ligne. `container` : douze premiers caractères de l'identifiant du conteneur lu. `mixed` : le service a un terminal, ses deux sorties sont fondues (BR-SLOG-012).
- `lines { seq, items: [{ t, s: "o" | "e", x, cut? }], cursor: { ts, rank } }` : `t` heure du moteur en **millisecondes** depuis l'époque (un entier que tout client lit sans perte ; la conception disait nanosecondes), `s` la sortie (`e` : erreur), `x` le texte déjà nettoyé et coupé à 10 000 caractères (BR-SLOG-013), `cut` le nombre total de caractères d'une ligne coupée. `cursor` : curseur de reprise de la dernière ligne du lot. Lots : toutes les 100 ms, ou dès 500 lignes, ou dès 256 Kio de texte.
- `gap { count?, reason: "slow" | "rotated" | "resume_lost" | "first_page" }` : des lignes manquent ici. `slow` : le lecteur du DIRECT était trop lent, `count` lignes perdues (BR-SLOG-007) ; `first_page` : la première page n'a pas pu être livrée en entier parce que le client n'a rien lu pendant 5 s pendant que l'agent la lisait (`count` lignes les plus anciennes de la page manquent ; ce n'est pas un lecteur lent, BR-SLOG-001) ; `resume_lost` : la reprise n'est pas acceptée ou la ligne de reprise n'existe plus (curseur trop ancien, dans le futur, illisible, horloge du moteur reculée, BR-SLOG-014), les dernières lignes suivent ; `rotated` est réservé. Une raison inconnue d'un agent plus récent se lit `Unknown` : un trou, sans plus.
- `marker { kind: "restarted", at }` : le service a redémarré, ce qui suit est la nouvelle exécution (BR-SLOG-009).
- `error { code, retry_after_s? }` puis fermeture. `retry_after_s` n'est présent que pour `LOGS_RATE_LIMITED` (secondes, au moins 1, avant qu'une ouverture soit rendue) ; un client qui ne le connaît pas l'ignore. Code de fermeture 1000 pour `LOGS_LIMIT`, `LOGS_ACCOUNT_LIMIT`, `LOGS_RATE_LIMITED`, `LOGS_UNREADABLE`, `SERVICE_NOT_FOUND`, `ENGINE_UNAVAILABLE` et `BAD_REQUEST` (sauf le délai d'`open`, ci-dessous) ; code 1008 (violation de politique) pour `UNAUTHENTICATED` (jeton refusé, premier message qui n'est pas `auth`, session terminée pendant la lecture), pour le délai d'`auth` dépassé (`error { UNAUTHENTICATED }` puis 1008) et pour le délai d'`open` dépassé (`error { BAD_REQUEST }` puis 1008) ; 1008 aussi, SANS `error`, pour le silence du client en cours de lecture ou un message refusé en cours de lecture ; code 1001 (départ) à l'arrêt de l'agent, à n'importe quelle étape (attente d'`auth`, attente d'`open`, lecture), à traiter comme une coupure à reprendre. Les codes : `LOGS_LIMIT` (dix lectures déjà ouvertes, BR-SLOG-020), `LOGS_ACCOUNT_LIMIT` (ce compte lit déjà les journaux de 3 services, tous postes confondus : fermer une lecture rend la place tout de suite, BR-SLOG-026), `LOGS_RATE_LIMITED` (ce compte ouvre plus de 12 lectures par minute, une rendue toutes les 5 secondes, la reprise après coupure comptant comme une ouverture : réessayer après `retry_after_s` secondes, BR-SLOG-027), `LOGS_UNREADABLE` (pilote de journaux `none`, BR-SLOG-017), `SERVICE_NOT_FOUND` (clé inconnue ou disparue, BR-SLOG-015), `ENGINE_UNAVAILABLE` (moteur pas prêt ou en panne, BR-SLOG-016), `UNAUTHENTICATED` (jeton refusé, premier message qui n'est pas `auth`, ou session terminée pendant la lecture), `BAD_REQUEST`. `SERVICE_NOT_FOUND` est aussi la réponse à une lecture que ce compte n'a (plus) le droit de faire (service adopté sans le droit) et à celle d'un conteneur non adopté : un droit retiré, une adoption relâchée ou un conteneur qui cesse d'être publié pendant la lecture la ferme (la décision est rejouée toutes les 2 s et à chaque changement du compte vu par la session). Un code, un état, un repère ou un type de message inconnu (agent plus récent) se lit `Unknown` : ignorer le message inconnu, traiter un code inconnu comme une erreur terminale générique.
- `pong { n }`.

Limites et garanties :
- **Anneau borné** par lecture : 5 000 lignes et 2 Mio de texte. Un client qui ne suit pas le direct perd les plus anciennes lignes, comptées dans un `gap { slow }` ; le moteur n'est jamais ralenti par le direct. La première page (un fichier déjà écrit) est lue au rythme du client : l'agent attend la place dans l'anneau (5 s au plus sans rien vider), donc 1 000 lignes de 4 Ko arrivent entières ; passé ce délai les lignes les plus anciennes de la page sont comptées dans `gap { first_page }`. Un client qui ne reçoit plus rien pendant 10 s perd sa lecture (le socket se ferme), jamais le lien.
- **Ouverture du socket** : avant la mise à niveau, l'agent peut refuser avec `503 BUSY` et `Retry-After` (au plus deux sockets en attente d'authentification par adresse, comptés avec `/stream` ; la place d'un socket de journaux est tenue jusqu'à la lecture d'`open`). Ce n'est pas une erreur à montrer : réessayer après un court délai. Un écran qui rouvre plusieurs lectures d'un coup (après une coupure, dont la mise à jour de l'agent) doit les ouvrir une par une, en attendant le premier `status` de l'une avant d'ouvrir la suivante, ou réessayer sur `503`.
- **Dix lectures** au plus pour tout l'agent, tous comptes confondus ; **trois par compte** (`LOGS_ACCOUNT_LIMIT`, BR-SLOG-026) et **douze ouvertures par minute par compte** (`LOGS_RATE_LIMITED` avec `retry_after_s`, BR-SLOG-027). Valeurs de départ, à ajuster après mesure sur forge. Les trois limites se vérifient dans cet ordre : le frein, le plafond du compte, le plafond du serveur ; chaque tentative coûte une ouverture. Un client qui rouvre plusieurs lectures après une coupure doit les rouvrir une par une et attendre `retry_after_s` sur un refus du frein.
- **Reprise** : le curseur est l'heure de la dernière ligne (ms) et son rang parmi les lignes de la même milliseconde ; relecture sans doublon ni perte tant que la ligne existe ET que l'heure du moteur n'a pas reculé de plus d'une seconde depuis la ligne du curseur. Bornes (BR-SLOG-014) : la relecture au moteur est bornée à une fenêtre de 25 000 MESSAGES du moteur (pas des lignes), donc un lecteur en retard de plus de 25 000 messages reçoit `resume_lost` puis les 1 000 dernières lignes, même avant 5 minutes (25 000 messages couvrent la coupure de 2 minutes d'une mise à jour de l'agent sous 200 messages par seconde environ, et 5 minutes sous 80 environ ; au-delà, c'est la fenêtre qui décide) ; un curseur de plus de 5 minutes, dans le futur (plus d'une seconde), négatif ou au rang supérieur à 20 000 n'est pas repris (`gap { resume_lost }` puis les dernières lignes) ; la relecture est plafonnée à 20 000 lignes sautées ; un conteneur recréé est lu depuis son début sur 5 000 lignes au plus. Limite exacte : si l'horloge du serveur recule de plus d'une seconde, les curseurs suivants sont ramenés à l'heure haute déjà vue, et la reprise par le client ne retrouve plus les lignes écrites tant que l'heure n'a pas rattrapé ce maximum ; l'agent le détecte pour ses propres relectures (trou puis dernières lignes), pas pour un curseur déjà remis au client.
- **Aucune ligne** n'entre dans une trace, la base, le journal d'activité ni la file des opérations (test sentinelle, BR-SLOG-019).

## Sujet `services` du flux `/stream`

- À l'abonnement : `{"type":"services_snapshot","rev":7,"engine":{…},"services":[…]}` (le contenu de `GET /services`, champs à plat).
- Ensuite `{"type":"services_delta","rev":8,"engine":{…}?,"upserts":[…],"removed":["c_…"]}` : un lot par groupe d'événements du moteur (regroupés sur 250 ms), moins de 500 ms après l'événement ; `engine` n'est présent que si l'état, le nom ou la version du moteur ont changé.
- `service_action` est déclaré et jamais émis par ces tranches.
- Les messages du flux portent `env_names` et `command` : la crate `tungstenite` n'écrit jamais ses messages sous le niveau `warn` (garde de `infrastructure/logging.rs`).
- Un abonné trop lent (file de 64 lots dépassée) reçoit un nouveau `services_snapshot` : le client n'a aucune logique de trou à écrire. Les lots plus anciens encore en file sont écartés par `rev`.
- Ouvert aux deux rôles. Un client qui ne connaît pas ces messages les ignore (ils ne sont pas des `ServerMessage`).

## Types déclarés pour les opérations de service (HRT-83, aucune route)

Le socle des services créés par Hearth déclare des types du fil **sans servir aucune route** : `API_VERSION` reste à `1`, `GET /hello` n'annonce encore aucune des nouvelles capacités (`service-ops`, `service-resources`, `service-files`, `service-transfers`, `service-backups`, `catalog`, `game-services`, constantes de `hearth_proto::api::service_ops`), le sujet `services` ne porte pas encore le message `operation`.

- `OperationView` (`api/service_ops.rs`) : `id`, `key`, `kind` (`start`, `stop`, `restart`, `deploy`, `update`, `delete`, `resources`, `backup`, `restore`, `world_import`), `status` (`accepted`, `running`, `awaiting_decision`, `cancelling`, `cancelled`, `rolling_back`, `rolled_back`, `succeeded`, `failed`, `interrupted`, `needs_attention`), `step`, `steps`, `percent`, `items_done`, `bytes_done`, `bytes_total`, `eta_s`, `cancellable`, `decision` (`choices`, `expires_at`), `cause`. Toute énumération a une variante `Unknown` (valeur inconnue tolérée) ; les champs facultatifs sont `serde(default)`. **Aucune valeur, aucun nom de variable, aucune commande.**
- `DriftView` (`api/service_spec.rs`) : `none`, `modified_outside`, `container_missing`.
- `ErrorCode` : les 24 codes du lot (`SPEC_CONFLICT`, `SERVICE_ADOPTED`, `PLAN_SETTING_REFUSED`, `NAME_BEING_DELETED`, `LIMIT_REFUSED_LIVE`, `LIMITS_CHANGED`, `OVERCOMMIT_CHANGED`, `RESOURCE_NOT_LIMITABLE`, `PATH_REFUSED`, `FILE_CONFLICT`, `FILE_NOT_EDITABLE`, `HARDLINKED`, `NO_SPACE` (507), `WRITE_DENIED` (403), `HEAVY_IO_BUSY`, `BACKUP_CORRUPT`, `BACKUP_IN_USE`, `RESTORE_PLAN_CHANGED`, `TRANSFER_EXPIRED` (410), `TRANSFER_LIMIT` (429), `FICHE_UNKNOWN` (404), `HOST_TOO_SMALL`, `WORLD_GONE`, `TOO_LATE_TO_CANCEL`), déclarés d'un coup ; les autres sont 409 ou 422 (voir `ErrorCode::http_status`). Un client ancien lit un code inconnu comme une erreur générique.
- Les actes `OperationCancel` et `OperationDecide` (codes `0x10` et `0x1D`) sont déclarés et servis par les routes des opérations de service ([service-ops](./service-ops.md), HRT-144).

## Comportement du moteur et de l'agent

- Sonde : `GET /_ping` du socket (connexion, droits, 5 secondes), toutes les 2 secondes quand le moteur est prêt, toutes les 5 secondes sinon ; `/version` pour le nom, la version et l'API. Changement d'état publié au plus une fois par fenêtre de 5 secondes (BR-ENGINE-012, BR-ENGINE-013).
- Une relecture lente (jusqu'à 30 s) ne met pas l'agent en attente : la sonde garde sa cadence et l'anti-clignotement ses échéances pendant ce temps ; si le moteur disparaît, la lecture est abandonnée (BR-ENGINE-009, BR-SVC-001).
- Liste : tous les conteneurs lus à l'entrée dans l'état prêt, puis relecture du seul conteneur touché sur événement ; rupture du flux d'événements = sonde immédiate puis relecture complète (BR-SVC-015).
- Sans moteur (poste de développement, serveur sans Docker) : état `absent` ou `stopped`, aucune erreur dans le journal, sonde toutes les 5 secondes.
- Réglages : `[engine] enabled`, `[engine] socket` de `agent.toml`, `HEARTH_ENGINE`, `HEARTH_ENGINE_SOCKET` (BR-ENGINE-021, ADR-0036).
- **Services supervisés (HRT-63)** : le propriétaire de chaque NOUVEAU conteneur est cherché une fois (`/proc`, groupe de contrôle d'un client `docker` ou `podman`), l'état des unités connues est relu toutes les 4 secondes (`systemctl show`) ; un service supervisé dont le conteneur disparaît reste listé tant que le système connaît l'unité. `systemctl` absent ou muet : aucun service n'est supervisé, aucune erreur (ADR-0037).
- **Récapitulatif des ports et détection des conflits** (règle de la spec fonctionnelle, sa fiche viendra avec l'écran) : côté CLIENT. L'agent ne calcule rien : `ports` (avec `scope`) et `host_network` de chaque service suffisent, et un service arrêté garde ses ports réservés.
