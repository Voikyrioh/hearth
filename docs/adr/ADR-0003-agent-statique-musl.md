---
id: ADR-0003
titre: Agent binaire statique x86_64-unknown-linux-musl
type: architecture
statut: acceptée
date: 2026-10-04
portee: projet
remplace: —
liens: [conception technique 2026-10-04, section 3]
---

# ADR-0003 — Agent binaire statique x86_64-unknown-linux-musl

## Contexte

Hearth-agent s'installe sur des machines Linux hétérogènes (distributions variables, versions glibc inconnues, NixOS). Binaire dynamique lié à glibc ne démarre pas sous NixOS sans configuration ; statique tourne partout. Trade-off : pas de chargement dynamique de bibliothèque (NVML pour GPU NVIDIA), obligatoire d'utiliser les sous-processus (nvidia-smi).

## Décision

Agent compilé en cible Rust `x86_64-unknown-linux-musl`, produit un exécutable statique autonome. Build en conteneur `rust:alpine` piloté par `cargo xtask agent`. Mesures GPU : NVIDIA par `nvidia-smi` (stdout parsée), AMD/Intel par fichiers noyau.

## Comment l'appliquer

- `crates/xtask/src/main.rs` : implémenter `fn agent()` appelant `cargo build --target x86_64-unknown-linux-musl --release` en conteneur.
- `crates/hearth-agent/Cargo.toml` : ajouter `[lib]` et `[[bin]]` pour être compilable.
- Sonde GPU : trait `GpuProbe` avec impl `NvidiaSmiProbe { fn sample() -> Result<GpuSample> }` (spawn `nvidia-smi --query-gpu=...`).
- Mode dev (Windows/local) : utiliser sonde `sysinfo::Systems::Gpu` pour tests sans nvidia-smi.

## Quand NE PAS l'appliquer / limites

- musl a quelques différences avec glibc (localisation, signaux) ; testé avant déploiement.
- Aucune bibliothèque non-standard dynamique : si futur besoin d'une lib GPU propriétaire, revoir cette décision.
- Compilation musl dure 2-3 fois plus longtemps que glibc ; CI accepte ce coût.

## Alternatives rejetées

- **Binaire glibc dynamique** : impossible sous NixOS sans configuration de l'admin.
- **Conteneur embarqué** : ajoute la taille, complexité pour déploiement bare-metal.
- **NVML lié statiquement** : propriétaire NVIDIA, et problèmes licence, évité.

## Conséquences

- Déploiement : un binaire unique, copie dans `/usr/local/bin/hearth-agent`, pas de runtime Rust requis.
- Mise à jour : superviseur (ancien binaire) télécharge nouveau, échange, redémarrage, vérification.
- Stockage : binaire ~15-20 Mo (Rust release), décompressé à l'installation.

## Références

- musl target Rust : https://doc.rust-lang.org/rustc/platform-support/x86_64-unknown-linux-musl.html
- Conteneur build `rust:alpine` : https://hub.docker.com/_/rust
- ADR-0003 globale (hexagonal Rust) : `orga-global/docs/adr/ADR-0003-*.md`
- ADR-0008 (mises à jour) : `./ADR-0008-mises-a-jour-signees.md`
