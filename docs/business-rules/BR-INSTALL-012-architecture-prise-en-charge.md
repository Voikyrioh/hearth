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
**x86_64 (amd64) seulement pour l'instant ; arm64 viendra** (aucun binaire arm64 n'est construit). Autre architecture, arm64 compris : « Cette architecture n'est pas prise en charge. Architecture supportée : x86_64 (arm64 viendra plus tard). », sans rien écrire (écart assumé avec le texte de la spécification, qui annonçait arm64). Le script `deploy/install.sh` refuse aussi avant de télécharger quoi que ce soit.

## Application (code)
- `crates/hearth-agent/src/domain/install/platform.rs::parse_arch`.
- `crates/hearth-agent/src/domain/install/prerequisites.rs::check_prerequisites` (`Blocker::UnsupportedArch`).
- `deploy/install.sh` : `uname -m`.

## Vérification
- `domain::install::platform::tests`, `domain::install::prerequisites::tests`.
- `tests/install_flow.rs::an_unsupported_architecture_is_refused_and_nothing_is_written`.

## Cas limites
- Le binaire construit par `cargo xtask agent` est x86_64 ; le binaire arm64, et l'acceptation de l'architecture, viendront ensemble.

## Règles liées
- BR-INSTALL-006

## Historique
- 2026-10-05 : création (HRT-15, session 2026-10-04-hearth-creation).
- 2026-10-05 : x86_64 seulement, arm64 refusé clairement.
