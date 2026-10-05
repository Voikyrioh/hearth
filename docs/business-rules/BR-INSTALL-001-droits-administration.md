---
id: BR-INSTALL-001
domaine: INSTALL
titre: Seuls les utilisateurs avec les droits d'administration installent
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-installer-agent.md (BR-INSTALL-001), HRT-15
maj: 2026-10-05
---

# BR-INSTALL-001 : Seuls les utilisateurs avec les droits d'administration installent

## Règle
Seuls les utilisateurs qui ont les droits d'administration de la machine peuvent exécuter `hearth-agent install` et `uninstall`. Sans eux, l'agent n'observe rien, n'écrit rien, affiche « Droits d'administration requis. Relance cette commande avec les droits d'administration. » et la commande exacte à relancer (`sudo …`).

## Application (code)
- `crates/hearth-agent/src/domain/install/prerequisites.rs::check_rights` et `check_prerequisites` : fonctions pures (premier contrôle, avant le système, l'architecture, le port, l'espace).
- `crates/hearth-agent/src/infrastructure/install/host.rs::SystemHost::is_privileged` : lit l'utilisateur effectif dans `/proc/self/status` (aucune bibliothèque C, aucun `unsafe`).
- `crates/hearth-agent/src/entrypoint/install.rs::install` et `uninstall` : le contrôle vient avant le verrou et avant toute observation.

## Vérification
- `domain::install::prerequisites::tests` (`the_rights_are_checked_on_their_own`, `without_administration_rights_nothing_else_is_looked_at`).
- `tests/install_flow.rs::without_administration_rights_it_stops_with_the_exact_command_and_writes_nothing`, `uninstalling_without_administration_rights_is_refused`.
- `cargo xtask e2e-install` : lancement sans droits dans le conteneur, rien d'écrit.

## Cas limites
- Windows (mode dev) : le système n'est pas Linux, l'installation répond « prise en charge sous Linux seulement » après le contrôle des droits.

## Historique
- 2026-10-05 : création (HRT-15, session 2026-10-04-hearth-creation).
