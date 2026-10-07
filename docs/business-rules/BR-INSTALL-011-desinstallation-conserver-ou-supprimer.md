---
id: BR-INSTALL-011
domaine: INSTALL
titre: La désinstallation permet de conserver ou de supprimer les données
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-installer-agent.md (BR-INSTALL-011), HRT-15
maj: 2026-10-05
---

# BR-INSTALL-011 : La désinstallation permet de conserver ou de supprimer les données

## Règle
`hearth-agent uninstall` arrête et retire le service et le binaire, puis, selon le choix, conserve (« conserver », `--keep-data`) ou supprime (« supprimer », `--purge`) les comptes, le journal, l'identité et la configuration. Interactif : la question est reposée tant que la réponse n'est pas valide. Sans terminal : `--keep-data`, `--purge` ou `--yes` (qui conserve) sont obligatoires. Un dossier de données partagé garde ses autres fichiers (listés, jamais touchés). Rien d'installé : « L'agent n'est pas installé sur ce serveur. Rien à désinstaller. ».

## Application (code)
- `crates/hearth-agent/src/domain/install/uninstall.rs::{parse_choice, uninstall_plan, is_installed}`.
- `crates/hearth-agent/src/application/install.rs::Installer::uninstall` : chaque étape est tentée même si une autre échoue ; ce qui reste est listé.
- `crates/hearth-agent/src/application/install.rs::Installer::remove_known_data` : la purge supprime la **liste exacte** des fichiers que Hearth connaît (identité, base et compagnons), puis le dossier seulement s'il est vide, et liste ce qui reste ; **aucune suppression récursive** d'un chemin venu de la configuration (`SystemHost` n'en a plus la capacité).

## Vérification
- `domain::install::uninstall::tests`.
- `tests/install_flow.rs` : `uninstalling_with_keep_stops_the_service_and_keeps_accounts_and_journal`, `uninstalling_with_purge_removes_everything`, `the_interactive_question_is_asked_until_the_answer_is_valid`.
- `cargo xtask e2e-install` : après `--purge`, aucun des chemins de l'agent n'existe.

## Cas limites
- Données laissées par une désinstallation précédente : `uninstall --purge` les retire même si l'agent n'est plus installé.
- Installation gérée : le binaire fourni par le système n'est pas retiré.

## Fichier du secret d'empreinte
`uninstall --purge` efface aussi `request_fingerprint.key` et ses temporaires (`DATA_FILES`, `is_fingerprint_secret_temporary`, BR-RESIL-021, HRT-32) ; `--keep-data` le conserve avec la base.

## Règles liées
- BR-INSTALL-003

## Historique
- 2026-10-05 : création (HRT-15, session 2026-10-04-hearth-creation).
- 2026-10-05 : purge par liste exacte de fichiers, dossier supprimé s'il est vide.
