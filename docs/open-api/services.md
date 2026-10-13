# Moteur de conteneurs et services (lecture seule)

Routes `GET /api/v1/services`, `POST /api/v1/engine/recheck` et `GET /api/v1/services/{key}/env`, et sujet `services` du flux `/stream`. Tranches 1 et 2 des déploiements (HRT-61, HRT-63) : l'agent détecte le moteur de conteneurs (Docker) et rend la liste de ses conteneurs, avec leur détail, tenue à jour en direct ; les services lancés par une unité systemd sont reconnus (clé `u_`). **Rien n'est écrit dans le moteur, ni dans un service, ni dans une unité** : aucune route d'action. La seule écriture de l'agent est la mémoire des services supervisés (`supervised_services`, migration `0009`).

- **Authentification** : session valable (Bearer), comme les autres lectures.
- **Rôle requis** : aucun rôle particulier pour `GET /services`, `POST /engine/recheck` et le flux : les deux rôles (Administrateur, Lecture seule) reçoivent exactement la même chose (BR-ENGINE-016, BR-SVC-020). `GET /services/{key}/env` exige le droit « gérer les services », quel que soit le rôle (BR-SVC-009, BR-GRANT-003, ADR-0041) : niveau d'accès `services`, qu'aucun rôle n'ouvre à lui seul.
- **Journal** : aucune entrée pour la lecture ni pour la relance de la détection (BR-ENGINE-020, BR-SVC-024). `GET /services/{key}/env` : le refus d'un compte sans le droit (cause « droits insuffisants ») et chaque réussite sont consignés (`service.env.reveal`), sans nom ni valeur.
- **Capacité** : `GET /hello` annonce `capabilities: ["services"]`. Un client n'envoie le sujet `services` ni n'appelle ces routes que si la capacité est annoncée : un sujet inconnu fait refuser TOUT le message `subscribe`, `metrics` compris (voir [stream](./stream.md)). Un agent ancien n'a pas le champ (lu comme une liste vide), un client ancien l'ignore. `API_VERSION` reste à 1 (ADR-0033, ADR-0038).
- **Code** : `entrypoint/http/services.rs`, `entrypoint/services_wire.rs` (conversions), `application/services.rs` (registre), `domain/services/`. Types : `hearth-proto::api::services`, `hearth-proto::stream::ServicesMessage`.
- **Valeurs absentes du fil** : aucune VALEUR de variable d'environnement dans `GET /services` ni dans le flux, pour aucun rôle (ADR-0038). Les NOMS des variables et la commande de lancement sont dans le détail pour les deux rôles (`env_names`, `command`, BR-SVC-007, BR-SVC-008) ; ils ne vont dans aucune trace de l'agent.

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
    "list_current_at": "2026-10-13T10:00:02Z"
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
| `services[]` | Tous les conteneurs, arrêtés compris, triés par nom sans tenir compte de la casse (BR-SVC-001, BR-SVC-012). Liste vide quand le moteur est absent, non pris en charge, de version non prise en charge ou désactivé (BR-SVC-018) ; gardée telle quelle quand le moteur est arrêté, refuse l'accès ou ne répond pas. |
| `key` | Identité stable, opaque : `c_<identifiant complet du conteneur>`, ou `u_<unité>` (suffixe `.service` compris) pour un service lancé par une unité systemd, le même quel que soit le conteneur que le système recrée (ADR-0037, BR-SVC-016). `h_<ULID>` est réservé à la tranche 5. |
| `origin` | Toujours `external` jusqu'à la tranche 5 (BR-SVC-003). |
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
- Erreurs : `403 FORBIDDEN_ROLE` (`details.missing: "services"`) sans le droit ; `404 NOT_FOUND` clé mal formée ou service inconnu (« Ce service n'existe pas. ») ; `404` service supervisé listé d'après son unité mais sans conteneur (« Les variables de ce service ne sont pas lisibles : son conteneur n'existe pas. ») ; `503 BUSY` moteur muet ; `503 BUSY` journal non écrit (aucune valeur rendue). La clé n'est jamais répétée dans une erreur.
- Une réponse est un secret : jamais stockée, jamais tracée (test sentinelle au niveau de trace le plus bavard).

## `POST /api/v1/engine/recheck`

Relance la détection du moteur tout de suite (« Vérifier à nouveau ») et rend `engine` (la forme de `EngineView` ci-dessus). C'est un `POST` qui **ne modifie rien** : il est dans `READ_ONLY_POSTS`, ni acte d'administration ni exception nommée, sans clé d'opération ni confirmation. Une demande trop rapprochée de la sonde précédente (moins d'1 seconde), ou pendant qu'une sonde est déjà en cours, rend l'état courant sans sonder. Ouvert aux deux rôles. Le résultat est publié tout de suite (hors de l'anti-clignotement).

## Sujet `services` du flux `/stream`

- À l'abonnement : `{"type":"services_snapshot","rev":7,"engine":{…},"services":[…]}` (le contenu de `GET /services`, champs à plat).
- Ensuite `{"type":"services_delta","rev":8,"engine":{…}?,"upserts":[…],"removed":["c_…"]}` : un lot par groupe d'événements du moteur (regroupés sur 250 ms), moins de 500 ms après l'événement ; `engine` n'est présent que si l'état, le nom ou la version du moteur ont changé.
- `service_action` est déclaré et jamais émis par ces tranches.
- Les messages du flux portent `env_names` et `command` : la crate `tungstenite` n'écrit jamais ses messages sous le niveau `warn` (garde de `infrastructure/logging.rs`).
- Un abonné trop lent (file de 64 lots dépassée) reçoit un nouveau `services_snapshot` : le client n'a aucune logique de trou à écrire. Les lots plus anciens encore en file sont écartés par `rev`.
- Ouvert aux deux rôles. Un client qui ne connaît pas ces messages les ignore (ils ne sont pas des `ServerMessage`).

## Comportement du moteur et de l'agent

- Sonde : `GET /_ping` du socket (connexion, droits, 5 secondes), toutes les 2 secondes quand le moteur est prêt, toutes les 5 secondes sinon ; `/version` pour le nom, la version et l'API. Changement d'état publié au plus une fois par fenêtre de 5 secondes (BR-ENGINE-012, BR-ENGINE-013).
- Une relecture lente (jusqu'à 30 s) ne met pas l'agent en attente : la sonde garde sa cadence et l'anti-clignotement ses échéances pendant ce temps ; si le moteur disparaît, la lecture est abandonnée (BR-ENGINE-009, BR-SVC-001).
- Liste : tous les conteneurs lus à l'entrée dans l'état prêt, puis relecture du seul conteneur touché sur événement ; rupture du flux d'événements = sonde immédiate puis relecture complète (BR-SVC-015).
- Sans moteur (poste de développement, serveur sans Docker) : état `absent` ou `stopped`, aucune erreur dans le journal, sonde toutes les 5 secondes.
- Réglages : `[engine] enabled`, `[engine] socket` de `agent.toml`, `HEARTH_ENGINE`, `HEARTH_ENGINE_SOCKET` (BR-ENGINE-021, ADR-0036).
- **Services supervisés (HRT-63)** : le propriétaire de chaque NOUVEAU conteneur est cherché une fois (`/proc`, groupe de contrôle d'un client `docker` ou `podman`), l'état des unités connues est relu toutes les 4 secondes (`systemctl show`) ; un service supervisé dont le conteneur disparaît reste listé tant que le système connaît l'unité. `systemctl` absent ou muet : aucun service n'est supervisé, aucune erreur (ADR-0037).
- **Récapitulatif des ports et détection des conflits** (règle de la spec fonctionnelle, sa fiche viendra avec l'écran) : côté CLIENT. L'agent ne calcule rien : `ports` (avec `scope`) et `host_network` de chaque service suffisent, et un service arrêté garde ses ports réservés.
