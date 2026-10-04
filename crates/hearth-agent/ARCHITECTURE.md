# Architecture — hearth-agent

Service installé sur la machine pilotée. Hexagonal : les dépendances vont de `entrypoint` et `infrastructure` vers `application` puis `domain`, jamais l'inverse. `lib.rs` expose le tout (les tests d'intégration démarrent un vrai serveur) ; `main.rs` ne fait que parser la ligne de commande, initialiser les journaux et appeler `entrypoint::cli::run`.

```
src/
├── domain/          → Règles pures, sans E/S (seule dépendance : sha2). fingerprint.rs (SHA-256 du certificat, affichage 8 × 4), install_id.rs
├── application/     → hello.rs (cas d'usage `GET /hello`) ; ports/ : IdentityStore, MachineInfo
├── infrastructure/  → Adaptateurs des ports et services techniques
│   ├── tls/         → identity.rs (FileIdentityStore : cert.pem, key.pem, install_id ; rcgen), server_config.rs (rustls, TLS 1.3 seul, ring)
│   ├── config/      → agent.toml + variables HEARTH_* + options CLI, fusionnés par `load`
│   └── system/      → machine_info.rs (nom d'hôte, adresses MAC)
├── entrypoint/
│   ├── http/        → mod.rs (routeur /api/v1, 404 en ErrorBody), hello.rs, error.rs (ApiError), server.rs (axum-server + rustls, arrêt propre)
│   └── cli.rs       → clap : `serve` (défaut), `fingerprint` ; signaux Ctrl+C / SIGTERM
└── app.rs           → Racine de composition : load_identity, start
tests/hello.rs       → Intégration : serveur sur port libre, client rustls sans vérification, empreinte, TLS 1.2 refusé
```

## Pour ajouter une route

1. Types de corps dans `hearth-proto/src/api/`.
2. Cas d'usage dans `application/` (ports si besoin), règles dans `domain/`.
3. Handler dans `entrypoint/http/{route}.rs`, déclaré dans `router` (`mod.rs`) ; erreurs via `ApiError`.
4. Fiche `docs/open-api/{route}.md` et ligne d'index ; fiche `docs/business-rules/BR-*` si règle.
