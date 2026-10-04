# Architecture — hearth-agent

Service installé sur la machine pilotée. Hexagonal : les dépendances vont de `entrypoint` et `infrastructure` vers `application` puis `domain`, jamais l'inverse ; `entrypoint` et `infrastructure` ne se connaissent pas, seul `app.rs` (racine de composition) les assemble. `lib.rs` expose le tout (les tests d'intégration démarrent un vrai serveur) ; `main.rs` initialise les journaux, appelle `app::run` et journalise toute erreur fatale.

```
src/
├── domain/          → Règles pures, sans E/S. identity_policy.rs (créer / réutiliser / refuser / nettoyer l'identité), install_id.rs. L'empreinte (`Fingerprint`) vit dans `hearth-proto`.
├── application/     → hello.rs (cas d'usage `GET /hello`, rend `AgentDescription`) ; ports/ : IdentityStore (partie publique seulement), MachineInfo
├── infrastructure/  → Adaptateurs des ports et services techniques
│   ├── tls/         → identity.rs (FileIdentityStore : cert.pem, key.pem, install_id, verrou identity.lock ; rcgen), server_config.rs (rustls, TLS 1.3 seul, ring ; seul lecteur de la clé privée)
│   ├── config/      → agent.toml + variables HEARTH_* + options CLI, fusionnés par `load`
│   ├── logging.rs   → tracing : texte en terminal, JSON sinon (HEARTH_LOG_FORMAT)
│   └── system/      → machine_info.rs (nom d'hôte, adresses MAC)
├── entrypoint/
│   ├── http/        → mod.rs (routeur /api/v1, journal des requêtes, erreurs de routage en ErrorBody), hello.rs, error.rs (ApiError), server.rs (axum-server + rustls, arrêt propre, poignées de main refusées en debug)
│   ├── cli.rs       → clap : options et sous-commandes `serve` (défaut), `fingerprint`
│   └── signal.rs    → Ctrl+C / SIGTERM
└── app.rs           → Racine de composition : load_config, load_identity, start, run
tests/hello.rs       → Intégration : serveur sur port libre, client rustls sans vérification, empreinte, TLS 1.2 refusé
```

## Conventions

- **Le cas d'usage rend une structure applicative ; `entrypoint/http` la convertit en type du fil de `hearth-proto`.** `application` ne connaît jamais le contrat JSON.
- La clé privée ne sort pas de `infrastructure/tls` : les ports n'exposent que du public.
- Les décisions métier sont des fonctions pures de `domain/` ; les adaptateurs observent et exécutent.

## Pour ajouter une route

1. Types de corps dans `hearth-proto/src/api/`.
2. Cas d'usage dans `application/` (ports si besoin), règles dans `domain/`.
3. Handler dans `entrypoint/http/{route}.rs` (conversion vers le type du fil), déclaré dans `router` (`mod.rs`) ; erreurs via `ApiError`.
4. Fiche `docs/open-api/{route}.md` et ligne d'index ; fiche `docs/business-rules/BR-*` si règle.
