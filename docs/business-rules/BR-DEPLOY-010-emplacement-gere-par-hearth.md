---
id: BR-DEPLOY-010
domaine: DEPLOY
titre: Les données d'un service géré vivent sous `<données de l'agent>/services/<clé>/`, privé à l'agent, créé par `SafeFs`
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-deployer-image.md (BR-DEPLOY-010), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md §9.1, HRT-145
maj: 2026-10-17
---

# BR-DEPLOY-010 : Les données d'un service géré vivent sous `<données de l'agent>/services/<clé>/`, privé à l'agent, créé par `SafeFs`

## Règle
Le « dossier géré par Hearth » d'un service est `<dossier de données de l'agent>/services/<clé du service>/`. `SafeFs::ensure_service_root` crée `services/` puis `<clé>/` en `0700` (propriétaire : l'utilisateur effectif de l'agent, quel que soit le masque), ou les rouvre s'ils existent (un dossier du même nom gardé après une suppression est réutilisé tel quel). Le dossier de données, `services/` et `<clé>/` doivent appartenir à l'utilisateur effectif de l'agent et n'être inscriptibles ni par le groupe ni par les autres (sinon `UnsafeRoot`) ; aucun n'est un lien ni un point de montage. Les sous-dossiers de données (un par point de montage) sont créés par `ServiceRoot::mkdir` avec le propriétaire et les droits donnés par l'appelant.

## Application (code)
- `crates/hearth-agent/src/infrastructure/fs/unix.rs::{open_service_root, open_services_dir, check_trusted}`
- `crates/hearth-agent/src/application/ports/safe_fs.rs::SafeFs::{open_service_root, ensure_service_root}`

## Vérification
- `crates/hearth-agent/src/infrastructure/fs/tests_unix.rs` : `the_services_folder_and_the_root_are_created_private`, `opening_a_missing_root_without_creating_it_fails`, `a_root_another_account_can_write_is_refused`, `a_data_folder_that_is_a_link_is_refused`, `a_service_root_replaced_by_a_link_is_not_followed`, `mkdir_creates_parents_with_the_given_mode_and_is_idempotent`
- `crates/hearth-agent/tests/safe_fs.rs` (suite de contrat, faux et réel)

## Cas limites
- Cette fiche ne couvre que l'emplacement et sa création ; le choix de l'emplacement dans le formulaire, l'affichage du chemin réel et le nombre d'éléments d'un dossier réutilisé sont des tickets d'interface (HRT-68, HRT-70).
- Linux seulement : hors Linux, `FsError::Unsupported`.
- Le dossier de données est ouvert par son chemin (de confiance) ; seuls son appartenance et ses droits sont contrôlés.

## Règles liées
- BR-FILES-003, BR-DEL-007

## Historique
- 2026-10-17 : création (HRT-145).
