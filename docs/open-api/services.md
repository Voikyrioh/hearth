# Moteur de conteneurs et services (lecture seule)

Routes `GET /api/v1/services` et `POST /api/v1/engine/recheck`, et sujet `services` du flux `/stream`. Tranche 1 des déploiements (HRT-61) : l'agent détecte le moteur de conteneurs (Docker) et rend la liste de ses conteneurs, tenue à jour en direct. **Rien n'est écrit, ni dans le moteur, ni dans la base** : aucune route d'action, aucune migration.

- **Authentification** : session valable (Bearer), comme les autres lectures.
- **Rôle requis** : aucun rôle particulier. Les deux rôles (Administrateur, Lecture seule) reçoivent exactement la même chose (BR-ENGINE-016, BR-SVC-020).
- **Journal** : aucune entrée, ni pour la lecture ni pour la relance de la détection (BR-ENGINE-020, BR-SVC-024).
- **Capacité** : `GET /hello` annonce `capabilities: ["services"]`. Un client n'envoie le sujet `services` ni n'appelle ces routes que si la capacité est annoncée : un sujet inconnu fait refuser TOUT le message `subscribe`, `metrics` compris (voir [stream](./stream.md)). Un agent ancien n'a pas le champ (lu comme une liste vide), un client ancien l'ignore. `API_VERSION` reste à 1 (ADR-0033, ADR-0038).
- **Code** : `entrypoint/http/services.rs`, `entrypoint/services_wire.rs` (conversions), `application/services.rs` (registre), `domain/services/`. Types : `hearth-proto::api::services`, `hearth-proto::stream::ServicesMessage`.
- **Données absentes du fil** : aucune valeur de variable d'environnement, aucun nom de variable, aucune commande de lancement, pour aucun rôle (ADR-0038).

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
      "restart": "unless_stopped"
    }
  ]
}
```

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
| `key` | Identité stable, opaque : `c_<identifiant complet du conteneur>` dans cette tranche (ADR-0037). |
| `origin` | Toujours `external` dans cette tranche (BR-SVC-003). |
| `state` | `running`, `stopped`, `error`, `restart_loop`, `paused`, `stopping`, `unknown` produits ici ; `starting`, `deploying` réservés aux tranches suivantes (BR-SVC-004 à 006). |
| `restarts_10m` | Démarrages non demandés des 10 dernières minutes, comptés par l'agent (BR-SVC-006). |
| `image.ref` | Absent pour une image sans nom (BR-SVC-022). |
| `driver`, `unit`, `busy`, `last_action` | Déclarés facultatifs pour les tranches suivantes, absents ici. `driver` absent vaut `engine` (le moteur pilote le service). |
| `restart` | `unless_stopped`, `always`, `on_failure`, `none` produits ici ; `supervisor` (relancé par le système du serveur) est déclaré pour la tranche 2. |

**Valeurs inconnues.** Toute énumération que le client lit (`engine.state`, `host.init`, `state`, `origin`, `driver`, `protocol`, `scope`, `mounts[].kind`, `restart`) se lit avec une variante de repli (`unknown`, ou `other` pour `host.init`) : une valeur ajoutée par un agent plus récent ne fait pas perdre l'instantané.

**Bornes.** Textes nettoyés des caractères de contrôle et de formatage invisibles (renversement du sens d'écriture compris) et plafonnés, au plus 128 ports et 64 montages par service ; le surplus est compté dans `ports_omitted` et `mounts_omitted` (facultatifs, absents quand 0 : « et N autres ») (BR-SVC-021).

Erreurs : celles de toute route authentifiée (`401`, `426`).

## `POST /api/v1/engine/recheck`

Relance la détection du moteur tout de suite (« Vérifier à nouveau ») et rend `engine` (la forme de `EngineView` ci-dessus). C'est un `POST` qui **ne modifie rien** : il est dans `READ_ONLY_POSTS`, ni acte d'administration ni exception nommée, sans clé d'opération ni confirmation. Une demande trop rapprochée de la sonde précédente (moins d'1 seconde), ou pendant qu'une sonde est déjà en cours, rend l'état courant sans sonder. Ouvert aux deux rôles. Le résultat est publié tout de suite (hors de l'anti-clignotement).

## Sujet `services` du flux `/stream`

- À l'abonnement : `{"type":"services_snapshot","rev":7,"engine":{…},"services":[…]}` (le contenu de `GET /services`, champs à plat).
- Ensuite `{"type":"services_delta","rev":8,"engine":{…}?,"upserts":[…],"removed":["c_…"]}` : un lot par groupe d'événements du moteur (regroupés sur 250 ms), moins de 500 ms après l'événement ; `engine` n'est présent que si l'état, le nom ou la version du moteur ont changé.
- `service_action` est déclaré et jamais émis par cette tranche.
- Un abonné trop lent (file de 64 lots dépassée) reçoit un nouveau `services_snapshot` : le client n'a aucune logique de trou à écrire. Les lots plus anciens encore en file sont écartés par `rev`.
- Ouvert aux deux rôles. Un client qui ne connaît pas ces messages les ignore (ils ne sont pas des `ServerMessage`).

## Comportement du moteur et de l'agent

- Sonde : `GET /_ping` du socket (connexion, droits, 5 secondes), toutes les 2 secondes quand le moteur est prêt, toutes les 5 secondes sinon ; `/version` pour le nom, la version et l'API. Changement d'état publié au plus une fois par fenêtre de 5 secondes (BR-ENGINE-012, BR-ENGINE-013).
- Une relecture lente (jusqu'à 30 s) ne met pas l'agent en attente : la sonde garde sa cadence et l'anti-clignotement ses échéances pendant ce temps ; si le moteur disparaît, la lecture est abandonnée (BR-ENGINE-009, BR-SVC-001).
- Liste : tous les conteneurs lus à l'entrée dans l'état prêt, puis relecture du seul conteneur touché sur événement ; rupture du flux d'événements = sonde immédiate puis relecture complète (BR-SVC-015).
- Sans moteur (poste de développement, serveur sans Docker) : état `absent` ou `stopped`, aucune erreur dans le journal, sonde toutes les 5 secondes.
- Réglages : `[engine] enabled`, `[engine] socket` de `agent.toml`, `HEARTH_ENGINE`, `HEARTH_ENGINE_SOCKET` (BR-ENGINE-021, ADR-0036).
