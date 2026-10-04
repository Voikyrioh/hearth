---
id: ADR-0001
titre: Monorepo Cargo + app Tauri
type: architecture
statut: acceptée
date: 2026-10-04
portee: projet
remplace: —
liens: [conception technique 2026-10-04, section 1]
---

# ADR-0001 — Monorepo Cargo + app Tauri

## Contexte

Hearth regroupe une bibliothèque cliente réutilisable (`hearth-link`), un agent serveur autonome (`hearth-agent`), des types partagés (`hearth-proto`), une coquille desktop (`apps/desktop`), et un build system. Ces composants partagent une version, une politique de dépendances, et un workflow CI/CD. Trois stratégies : repos séparés (complexité de versionnage), monorepo traditionnel NPM/Python (pas adapté à du Rust multi-crate), workspace Cargo + sous-dossier `apps/` pour Tauri et le front.

## Décision

Un seul dépôt `voikyrioh/hearth` privé. Workspace Cargo à la racine : `crates/{hearth-proto, hearth-agent, hearth-link, xtask}`. App Tauri dans `apps/desktop/{src-tauri, src}`. Script d'installation dans `deploy/`.

## Comment l'appliquer

- Chaque crate `crates/*/Cargo.toml` déclare ses dépendances. `workspace.resolver = "2"` pour héritage des versions.
- App Tauri : `npm` + Cargo au niveau `apps/desktop/` ; `npm run tauri build` encapsule le build Rust.
- Build agent statique : `cargo xtask agent` (entrée `crates/xtask/src/main.rs`).
- CI/CD : un `.github/workflows/` avec jobs paralléles (tests Rust, build client, déploiement).

## Quand NE PAS l'appliquer / limites

- Ne pas créer de repos séparé pour `hearth-link` ou `hearth-agent` sans accord ; la réutilisation reste via publication crate sur crates.io si besoin.
- Monorepo Cargo augmente le temps de compilation global ; cibler les crates changeants dans les tests CI.

## Alternatives rejetées

- **Repos séparés** : versionnage compliqué, CI redondante, dépôt du client vide au bootstrap.
- **Cargo workspace + autre gestionnaire front (Yarn)** : complexité, duplication package manager.
- **Rust tout-en-un dans une seule crate** : mélange de responsabilités, pas de réutilisabilité pour `hearth-link` ailleurs.

## Conséquences

- Releases / versionnage unifiés : tag git `v*` appliqué au workspace entier.
- Dépendances compatibles à la source : pas de divergences npm/cargo.
- CI plus simple : un seul flux build/test, nécessite parfois de compiler même si seul le front change.

## Références

- Cargo Workspaces : https://doc.rust-lang.org/cargo/reference/workspaces.html
- Tauri + Cargo : https://tauri.app/en/v1/guides/getting-started/setup/integrate/
- ADR globale 0001 (stack) : `J:/Dev/Projects/orga/global/docs/adr/ADR-0001-*.md`
