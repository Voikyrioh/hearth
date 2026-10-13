# Architecture — domain/services

Moteur de conteneurs et services (HRT-61, ADR-0036 à ADR-0038). Règles pures, sans E/S : ni `bollard`, ni SQLx, ni axum, ni tokio. Le moteur est vu par le port `application/ports/container_engine.rs` ; ce dossier ne connaît que ses propres types. Aucun type d'ici ne porte une valeur ou un nom de variable d'environnement, ni une commande.

```
services/
├── engine.rs  → `EngineState` (les huit états plus `Checking`), `classify_probe` (sonde vers état : socket absent sans repère d'installation = absent, avec = arrêté, refusé = arrêté, EACCES = accès refusé, silence = ne répond pas, autre moteur = non pris en charge, API < 1.41 = version non prise en charge, désactivé sans sonde), `is_supported_engine`, `ApiVersion`, `Debounce` (au plus un changement publié par fenêtre, temps injecté) — BR-ENGINE-002, 005 à 013, 019, 021
├── state.rs   → `Observation` (un conteneur réduit à ce que la liste montre), `derive_state` (état d'un service, fonction pure : code de sortie, `restarting`, pause, jamais démarré), `StartWindow` (démarrages des 10 dernières minutes, 3 = boucle), `compare_names` (tri sans la casse) — BR-SVC-004 à 006, 012
├── image.rs   → `image_name` : nom, version, identifiant court ; image sans nom — BR-SVC-022
├── key.rs     → `ServiceKey` : `c_` produite, `h_` et `u_` analysées seulement — ADR-0037
└── limits.rs  → bornes de tout ce qui vient d'un conteneur : `bounded` (sans caractères de contrôle, coupé à une longueur), `MAX_PORTS`, `MAX_MOUNTS`… — BR-SVC-021
```

Les tranches suivantes ajoutent `lock.rs`, `intent.rs`, `spec.rs`, `folders.rs`, `ports.rs`, `command.rs`, `deploy.rs`, `logs.rs` (conception technique des déploiements, section 9).
