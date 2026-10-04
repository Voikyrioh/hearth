# Hearth

Piloter un serveur maison depuis une app Windows : état de la machine, services compartimentés (jeux, apps, modèles d'IA), alimentation et réveil réseau, éclairage, écran du boîtier.

Deux morceaux :
- **agent** : un binaire autonome installé sur le serveur Linux ;
- **client** : une app Windows avec installateur.

État : socle en cours de développement. Rien n'est encore installable.

## Dépôt

| Dossier | Rôle |
|---|---|
| `crates/hearth-proto` | Types partagés du protocole |
| `crates/hearth-agent` | Agent serveur |
| `crates/hearth-link` | Liaison cliente (épinglage, session, reconnexion) |
| `apps/desktop` | Client Windows (à venir) |
| `deploy/` | Script d'installation (à venir) |
| `xtask/` | Tâches de build |
| `docs/` | ADR, règles métier, contrats, composants |

## Développer

Prérequis : Rust 1.95 (installé par `rust-toolchain.toml`).

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Licence

Hearth est distribué sous licence [GNU AGPL version 3 ou ultérieure](./LICENSE) : tu peux l'utiliser, le modifier et le redistribuer, y compris commercialement, à condition de publier tes modifications sous la même licence, même si tu ne fais que le faire tourner comme service.

Copyright © 2026 Voikyrioh. Pour un usage hors des conditions de l'AGPL, une licence commerciale peut être accordée par l'auteur.

Les contributions externes ne sont pas acceptées pour l'instant.
