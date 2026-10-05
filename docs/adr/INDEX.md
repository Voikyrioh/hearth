# ADR — Hearth

Décisions d'architecture et conventions. Chaque ADR référence les ADR globales pertinentes (orga-global/docs/adr/).

| ADR | Type | Titre | Statut | Date |
|---|---|---|---|---|
| [ADR-0001](./ADR-0001-monorepo-workspace.md) | architecture | Monorepo Cargo + app Tauri | acceptée | 2026-10-04 |
| [ADR-0002](./ADR-0002-client-tauri-vue.md) | librairie | Client Tauri 2 + Vue 3, réseau dans Rust | acceptée | 2026-10-04 |
| [ADR-0003](./ADR-0003-agent-statique-musl.md) | architecture | Agent binaire statique x86_64-unknown-linux-musl | acceptée | 2026-10-04 |
| [ADR-0004](./ADR-0004-protocole-https-websocket.md) | convention | Protocole HTTP/WebSocket `/api/v1`, idempotency, versions | acceptée | 2026-10-04 |
| [ADR-0005](./ADR-0005-tls-epingle.md) | securite | TLS 1.3 auto-signé épinglé à la première connexion | acceptée | 2026-10-04 |
| [ADR-0006](./ADR-0006-sqlite-sqlx.md) | librairie | Persistance agent : SQLx + SQLite | acceptée | 2026-10-04 |
| [ADR-0007](./ADR-0007-machine-a-etats-du-lien.md) | architecture | Machine à états du lien : seuils 3s/30s, reconnexion sans fin | acceptée | 2026-10-04 |
| [ADR-0008](./ADR-0008-mises-a-jour-signees.md) | securite | Mises à jour agent signées (minisign), superviseur et retour arrière | acceptée | 2026-10-04 |
| [ADR-0009](./ADR-0009-dependances-agent.md) | librairie | Dépendances de l'agent : axum, rustls + ring, rcgen, clap, toml, tower-http, sqlx, argon2 (pas d'aws-lc ni d'OpenSSL) | acceptée | 2026-10-04 |
| [ADR-0010](./ADR-0010-dependances-client.md) | librairie | Dépendances du client : Tauri et greffons, Vue, Pinia, Vite, Biome, Vitest, tauri-specta, polices embarquées | acceptée | 2026-10-04 |
| [ADR-0011](./ADR-0011-dependances-liaison.md) | librairie | Dépendances de la bibliothèque de liaison : reqwest, rustls + ring, tokio-rustls, tokio-tungstenite, futures-util, if-addrs (pas d'aws-lc ni d'OpenSSL) | acceptée | 2026-10-05 |
| [ADR-0012](./ADR-0012-service-systeme.md) | securite | Service système : root, unité systemd durcie, installation gérée sans unité | acceptée | 2026-10-05 |
