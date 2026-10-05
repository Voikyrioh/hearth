---
id: BR-INSTALL-008
domaine: INSTALL
titre: Une erreur ou une interruption laisse la machine dans l'état d'avant
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-installer-agent.md (BR-INSTALL-008), HRT-15
maj: 2026-10-05
---

# BR-INSTALL-008 : Une erreur ou une interruption laisse la machine dans l'état d'avant

## Règle
Une erreur à n'importe quelle étape, ou une interruption (Ctrl+C, SIGTERM), annonce ce qui s'est passé et défait ce que **cette exécution** a fait : binaire retiré (ou ancien binaire rétabli), configuration, dossier de données créé (avec la base, l'identité et le compte), unité et service retirés (ou ancien service relancé). Ce qui existait avant n'est jamais touché : seul ce qui n'existait pas du tout est noté « créé » (une identité, une base ou une configuration préexistantes, même partielles, ne sont jamais supprimées). Un service arrêté avant reste arrêté, l'unité d'avant est réécrite telle quelle. Les signaux SIGINT, SIGTERM et SIGHUP (session ssh qui tombe) déclenchent le retour en arrière. Ce qui ne peut pas être défait est listé pour un nettoyage manuel. Le script `deploy/install.sh` supprime son dossier temporaire à la sortie.

## Application (code)
- `crates/hearth-agent/src/domain/install/rollback.rs::undo_plan` : fonction pure, ordre inverse, rien de ce qui existait avant.
- `crates/hearth-agent/src/application/install.rs::Installer::{apply, rollback}` : chaque étape est notée **avant** son exécution, l'interruption est testée entre les étapes.
- `crates/hearth-agent/src/infrastructure/install/host.rs::SystemHost::install_binary` : écriture atomique, ancien binaire gardé sous un second nom (lien dur).

## Vérification
- `domain::install::rollback::tests`.
- `tests/install_flow.rs` : `a_failure_at_any_step_of_a_first_installation_leaves_the_machine_as_before`, `an_interruption_after_each_step_leaves_the_machine_as_before`, `a_failed_reinstallation_restores_the_binary_and_keeps_data_and_accounts`, `a_certificate_that_is_not_the_installed_identity_fails_and_is_undone`.
- `infrastructure::install::host::unix_tests` (copie atomique, restauration, aucun fichier temporaire).

## Cas limites
- Avant la première écriture (questions, prérequis), une interruption ne laisse rien à défaire : le processus s'arrête comme tout programme.
- L'observation ne modifie pas la base : le nombre d'administrateurs est lu en lecture seule, sans migration (`infrastructure/sqlite::count_admins_read_only`). Les migrations d'une base existante sont jouées par le service au démarrage ; l'installateur n'ouvre la base en écriture que pour y créer le premier compte (installation neuve ou base sans administrateur).

## Règles liées
- BR-INSTALL-006

## Historique
- 2026-10-05 : création (HRT-15, session 2026-10-04-hearth-creation).
- 2026-10-05 : état d'avant fidèle (unité, service arrêté), SIGHUP, jamais de suppression de l'existant.
