---
id: BR-DEL-006
domaine: DEL
titre: « Supprimer aussi les données » n'efface que les dossiers gérés par Hearth, jamais un dossier du serveur
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-supprimer-service.md (BR-DEL-006), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md §5 et §9, HRT-85
maj: 2026-10-24
---

# BR-DEL-006 : « Supprimer aussi les données » n'efface que les dossiers gérés par Hearth, jamais un dossier du serveur

## Règle
La case « données » efface le contenu des dossiers gérés par Hearth de ce service (les sous-dossiers de `<données>/services/<clé>/`, un composant chacun), et d'eux seuls. Un dossier du serveur monté dans le service n'est jamais effacé, même choisi au déploiement ; un dossier « géré » dont le nom n'est pas un composant sûr est conservé et signalé ; un volume du moteur est conservé (la suppression du conteneur ne les prend pas : `ContainerEngine::remove_keeping_volumes`). La racine du service ne part que si elle est VIDE après l'effacement (`remove_empty_service_root`).

## Application (code)
- `crates/hearth-agent/src/domain/services/delete.rs::{split_mounts, MountSplit}`
- `crates/hearth-agent/src/application/ports/container_engine.rs::ContainerEngine::remove_keeping_volumes`
- `crates/hearth-agent/src/application/delete.rs::ServiceDeletion::erase`

## Vérification
- `crates/hearth-agent/src/domain/services/delete.rs::tests` (`managed_folders_are_split_from_server_folders_and_never_mixed`, `a_managed_name_that_climbs_or_nests_is_kept_as_unmanaged_never_erased`)
- `crates/hearth-agent/tests/service_delete_https.rs::volumes_are_counted_and_kept_the_removal_never_asks_the_engine_to_take_them`
- `crates/hearth-agent/tests/engine_fake_daemon.rs::removing_for_a_deletion_keeps_the_volumes_and_never_forces`

## Cas limites
- Cette version ne monte aucun dossier du serveur (FOLDER_REFUSED, HRT-70) : le tri est prêt pour quand elles existeront.

## Règles liées
- BR-DEL-007
- BR-DEL-008
- BR-DEPLOY-010

## Historique
- 2026-10-24 : création (HRT-85, côté agent).
