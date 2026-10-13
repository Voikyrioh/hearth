# Architecture — domain/services

Moteur de conteneurs et services (HRT-61, HRT-63, ADR-0036 à ADR-0038). Règles pures, sans E/S : ni `bollard`, ni SQLx, ni axum, ni tokio, ni lecture de `/proc`. Le moteur est vu par le port `application/ports/container_engine.rs`, le superviseur par `application/ports/service_supervisor.rs` ; ce dossier ne connaît que ses propres types. Aucun type d'ici ne porte une VALEUR de variable d'environnement ; les noms de variables et la commande ne s'y trouvent que dans `Hidden` (`Debug` masqué).

```
services/
├── engine.rs  → `EngineState` (les huit états plus `Checking`), `classify_probe` (sonde vers état : socket absent sans repère d'installation = absent, avec = arrêté, refusé = arrêté, EACCES = accès refusé, silence = ne répond pas, autre moteur = non pris en charge, API < 1.41 = version non prise en charge, désactivé sans sonde), `is_supported_engine`, `ApiVersion`, `Debounce` (au plus un changement publié par fenêtre, temps injecté) — BR-ENGINE-002, 005 à 013, 019, 021
├── state.rs   → `Observation` (un conteneur réduit à ce que la liste et le détail montrent), `derive_state` (état d'un service, fonction pure : code de sortie, `restarting`, pause, jamais démarré), `derive_supervised` et `derive_unit_state` (état d'un service supervisé : celui de l'unité quand le conteneur ne tourne pas), `stop_grace_s` (délai d'arrêt), `StartWindow` (démarrages des 10 dernières minutes, 3 = boucle), `compare_names` (tri sans la casse) — BR-SVC-004 à 007, 012
├── image.rs   → `image_name` : nom, version, identifiant court ; image sans nom — BR-SVC-022
├── key.rs     → `ServiceKey` : `c_` et `u_` produites, `h_` analysée seulement — ADR-0037, BR-SVC-016
├── unit.rs    → `UnitName` (nom d'unité validé : jeu de caractères de `systemd.unit(5)`), `UnitStatus`, `UnitLookup`, `parse_show` (sortie de `systemctl show`), `parse_timespan`, `unit_of_cgroup` (groupe de contrôle vers unité), `client_refs` (ligne de commande d'un client du moteur vers les conteneurs qu'elle nomme), `is_engine_client` — BR-SVC-016
├── hidden.rs  → `Hidden` : nom de variable ou argument de commande, `Debug` masqué, pas de `Display` — BR-SVC-008
└── limits.rs  → bornes de tout ce qui vient d'un conteneur : `bounded` (sans caractères de contrôle, coupé à une longueur), `MAX_PORTS`, `MAX_MOUNTS`, `MAX_ENV_NAMES`, `MAX_COMMAND_ARGS`… — BR-SVC-021
```

Les tranches suivantes ajoutent `lock.rs`, `intent.rs`, `spec.rs`, `folders.rs`, `ports.rs`, `command.rs`, `deploy.rs`, `logs.rs` (conception technique des déploiements, section 9).
