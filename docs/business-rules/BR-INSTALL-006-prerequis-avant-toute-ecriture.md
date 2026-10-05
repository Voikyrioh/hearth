---
id: BR-INSTALL-006
domaine: INSTALL
titre: L'installation s'interrompt sans modification si un prérequis manque
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-installer-agent.md (BR-INSTALL-006), HRT-15
maj: 2026-10-05
---

# BR-INSTALL-006 : L'installation s'interrompt sans modification si un prérequis manque

## Règle
Les prérequis sont contrôlés dans l'ordre : droits, système (Linux), architecture, port libre, espace disque (256 Mio libres), et systemd présent (sinon `--managed`). Le premier qui manque arrête l'installation **avant la moindre écriture**, avec le message de la spécification et, pour un port occupé, la commande à relancer avec un autre port. Un port occupé par l'agent déjà installé et en marche n'est pas un obstacle à sa réinstallation.

## Application (code)
- `crates/hearth-agent/src/domain/install/prerequisites.rs::check_prerequisites`.
- `crates/hearth-agent/src/domain/install/port.rs::parse_port` : 1 à 65535, vide = 7341.
- `crates/hearth-agent/src/application/install.rs::Installer::check_prerequisites` : observe (port, espace) puis décide.
- `deploy/install.sh` : le téléchargement échoue proprement (message, rien d'installé) quand le réseau ou la version manquent.

## Vérification
- `domain::install::prerequisites::tests`, `domain::install::port::tests`.
- `tests/install_flow.rs` : `a_taken_port_is_refused_with_the_command_to_relaunch_and_nothing_is_written`, `an_unsupported_architecture_is_refused_and_nothing_is_written`, `systemd_missing_without_the_managed_flag_is_refused_before_any_write`, `a_bad_port_is_refused_before_any_write`.
- `cargo xtask e2e-install` : port occupé et droits manquants, rien d'écrit (liste des chemins vérifiée).

## Cas limites
- Espace disque illisible (pas de `df`) : le contrôle est ignoré plutôt que de bloquer l'installation.

## Règles liées
- BR-INSTALL-001, BR-INSTALL-012

## Historique
- 2026-10-05 : création (HRT-15, session 2026-10-04-hearth-creation).
