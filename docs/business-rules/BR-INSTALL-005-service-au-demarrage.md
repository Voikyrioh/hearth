---
id: BR-INSTALL-005
domaine: INSTALL
titre: Le service démarre automatiquement au démarrage du système
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-installer-agent.md (BR-INSTALL-005), HRT-15
maj: 2026-10-05
---

# BR-INSTALL-005 : Le service démarre automatiquement au démarrage du système

## Règle
À la fin de l'installation, l'agent tourne et redémarre tout seul au démarrage du serveur : unité systemd `hearth-agent.service` activée (`WantedBy=multi-user.target`) et démarrée (`enable --now`), `Restart=always`. En installation gérée par le système (`--managed`, NixOS), l'agent n'écrit aucune unité et le dit : le système déclare et démarre le service.

## Application (code)
- `crates/hearth-agent/src/infrastructure/service/systemd.rs::render_unit` et `Systemd::install` (`daemon-reload`, `enable --now`).
- `crates/hearth-agent/src/infrastructure/service/none.rs::Unmanaged` : installation gérée, rien d'écrit.
- `crates/hearth-agent/src/domain/install/plan.rs::ServiceAction` : démarrer, redémarrer ou laisser.
- `crates/hearth-agent/src/application/install.rs::Installer::run` (étape 6) : démarrage, puis attente de `/api/v1/hello` et comparaison de l'empreinte du certificat servi.

## Vérification
- `infrastructure::service::systemd::tests` (faux `systemctl` : appels et ordre, unité, durcissement, aucun secret).
- `tests/install_flow.rs::a_managed_installation_writes_no_unit_copies_no_binary_and_says_so`.
- `cargo xtask e2e-install` : systemd réellement démarré dans le conteneur, `systemctl is-enabled` et `is-active`, `/hello` répond.

## Cas limites
- Aucun redémarrage de la machine : le service est lancé à la fin de l'installation.

## Règles liées
- ADR-0012

## Historique
- 2026-10-05 : création (HRT-15, session 2026-10-04-hearth-creation).
