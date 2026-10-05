---
id: BR-INSTALL-012
domaine: INSTALL
titre: L'installation refuse les architectures non prises en charge
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-installer-agent.md (BR-INSTALL-012), HRT-15
maj: 2026-10-05
---

# BR-INSTALL-012 : L'installation refuse les architectures non prises en charge

## Règle
x86_64 (amd64) et arm64 (aarch64) seulement. Autre architecture : « Cette architecture n'est pas prise en charge. Architectures supportées : x86_64, arm64. », sans rien écrire. Le script `deploy/install.sh` refuse aussi avant de télécharger quoi que ce soit.

## Application (code)
- `crates/hearth-agent/src/domain/install/platform.rs::parse_arch`.
- `crates/hearth-agent/src/domain/install/prerequisites.rs::check_prerequisites` (`Blocker::UnsupportedArch`).
- `deploy/install.sh` : `uname -m`.

## Vérification
- `domain::install::platform::tests`, `domain::install::prerequisites::tests`.
- `tests/install_flow.rs::an_unsupported_architecture_is_refused_and_nothing_is_written`.

## Cas limites
- Le binaire construit par `cargo xtask agent` est x86_64 ; le binaire arm64 viendra avec la publication des versions.

## Règles liées
- BR-INSTALL-006

## Historique
- 2026-10-05 : création (HRT-15, session 2026-10-04-hearth-creation).
