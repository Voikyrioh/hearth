# Hearth

Piloter un serveur maison depuis une app Windows : état de la machine, services compartimentés (jeux, apps, modèles d'IA), alimentation et réveil réseau, éclairage, écran du boîtier.

Deux morceaux :
- **agent** : un binaire autonome installé sur le serveur Linux ;
- **client** : une app Windows avec installateur.

État : socle en cours de développement. L'agent s'installe en une commande sur un serveur Linux ; le client Windows est en construction. La publication des versions n'existe pas encore : l'agent se construit et s'installe depuis ce dépôt.

## Installer l'agent

Sur le serveur Linux (x86_64 ou arm64, systemd), avec les droits d'administration :

```sh
cargo xtask agent                                   # binaire statique dans target/dist/hearth-agent (Docker requis)
sudo sh deploy/install.sh --binary ./hearth-agent   # questions : port, premier compte, mot de passe
```

Sans question : `sudo HEARTH_ADMIN_USER=marie HEARTH_ADMIN_PASSWORD='…' sh deploy/install.sh --binary ./hearth-agent --yes`. À la fin, l'agent tourne, démarre avec le serveur, et affiche son **empreinte** : à comparer avec celle du client à la première connexion. Désinstaller : `sudo hearth-agent uninstall [--keep-data|--purge]`. Détails, réinstallation, installation gérée (NixOS), dépannage : [`docs/runbooks/installer-agent.md`](./docs/runbooks/installer-agent.md).

## Dépôt

| Dossier | Rôle |
|---|---|
| `crates/hearth-proto` | Types partagés du protocole |
| `crates/hearth-agent` | Agent serveur |
| `crates/hearth-link` | Liaison cliente (épinglage, session, reconnexion) |
| `apps/desktop` | Client Windows (à venir) |
| `deploy/` | Script d'installation (`install.sh`) et scénario de bout en bout (`e2e/`) |
| `xtask/` | Tâches de build : `cargo xtask agent`, `e2e-install`, `shellcheck` |
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
