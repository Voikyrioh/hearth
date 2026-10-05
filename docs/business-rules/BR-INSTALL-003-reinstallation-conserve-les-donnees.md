---
id: BR-INSTALL-003
domaine: INSTALL
titre: Une réinstallation détecte et conserve comptes, journal, empreinte et configuration
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-installer-agent.md (BR-INSTALL-003), HRT-15
maj: 2026-10-05
---

# BR-INSTALL-003 : Une réinstallation détecte et conserve comptes, journal, empreinte et configuration

## Règle
Une réinstallation (même version, mise à niveau, ou réparation d'une installation abîmée) détecte les données présentes et les conserve : comptes, journal, identité (donc l'empreinte) et configuration. Le fichier de configuration n'est jamais écrasé ; le premier compte n'est pas redemandé.

## Application (code)
- `crates/hearth-agent/src/domain/install/plan.rs::plan_install` : `InstallKind` (Fresh, Reinstall, Upgrade, Repair) et `keeps` (`Kept`).
- `crates/hearth-agent/src/infrastructure/install/host.rs::SystemHost::write_config_new` : création exclusive (`create_new`), jamais d'écrasement.
- `crates/hearth-agent/src/application/install.rs::Installer::run` : l'identité passe par `IdentityStore::load_or_create` (BR-INSTALL-004), la base n'est jamais recréée.

## Vérification
- `domain::install::plan::tests` (réinstallation, mise à niveau, réparation, données laissées par une désinstallation).
- `tests/install_flow.rs::a_reinstallation_asks_nothing_keeps_everything_and_restarts_on_the_new_binary`, `data_kept_by_an_uninstall_are_found_again_by_the_next_installation`.
- `cargo xtask e2e-install` : mêmes comptes et même empreinte après réinstallation, et après désinstallation avec conservation puis réinstallation.

## Cas limites
- Une réinstallation avec un autre port : le port de l'installation existante est conservé et le message le dit ; changer de port exige une modification manuelle de `agent.toml` (hors installation automatique).
- Mot de passe oublié : la réinstallation ne le réinitialise pas (`hearth-agent account passwd`, voir le runbook d'accès administrateur).

## Règles liées
- BR-INSTALL-004, BR-INSTALL-007

## Historique
- 2026-10-05 : création (HRT-15, session 2026-10-04-hearth-creation).
