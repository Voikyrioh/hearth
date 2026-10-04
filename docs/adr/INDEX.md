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
| [ADR-0009](./ADR-0009-dependances-agent.md) | librairie | Dépendances de l'agent : axum, rustls + ring, rcgen, clap, toml, tower-http (pas d'aws-lc ni d'OpenSSL) | acceptée | 2026-10-04 |
