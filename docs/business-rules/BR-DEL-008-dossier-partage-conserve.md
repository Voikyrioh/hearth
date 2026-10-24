---
id: BR-DEL-008
domaine: DEL
titre: Un dossier géré qu'un autre conteneur utilise n'est pas effacé
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-supprimer-service.md (BR-DEL-008), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md §5 et §9, HRT-85
maj: 2026-10-24
---

# BR-DEL-008 : Un dossier géré qu'un autre conteneur utilise n'est pas effacé

## Règle
Un dossier géré qu'un autre conteneur monte (le même chemin, un parent ou un enfant), en marche ou arrêté, créé par Hearth, adopté ou non, est CONSERVÉ avec le nom des services concernés ; un conteneur créé hors de Hearth est dit « un conteneur créé hors de Hearth », sans nom (BR-ADOPT-002). Ce qui est couvert, exactement : (1) le même dossier, un parent ou un enfant, par le NOM, sous la forme lexicale (`//`, `.`, `..`) ET canonique des deux côtés (dossier de données canonique lu au démarrage, source du moteur canonisée quand elle existe) ; (2) par l'IDENTITÉ (périphérique, inode), dans les deux sens : le dossier à effacer est comparé, avec TOUS ses parents jusqu'à la racine, à chaque dossier monté par un autre conteneur et à tous les parents de celui-ci. Un lien, ou un montage « bind » du dossier de données (`/persist/var/lib/hearth` pour `/var/lib/hearth`), ne cache donc ni le dossier lui-même, ni un parent monté sous un autre nom, ni un enfant monté par son vrai chemin. Les parents communs (la racine, le dossier de données) ne comptent pas : seule l'identité propre de chaque côté est cherchée dans la lignée de l'autre. L'identité de montage (`mnt_id`) n'est volontairement PAS comparée : deux montages « bind » du même dossier ont deux `mnt_id` et sont le même dossier. Ce qui n'est PAS couvert : un montage fait dans le conteneur après sa création, ou que le moteur ne rapporte pas ; un dossier qui n'existe pas (comparé par le nom et par la lignée de son parent). Un chemin qu'on ne peut pas lire (partage réseau mort, droit refusé) ou une lecture qui dépasse le budget (10 s en tout, 3 s par chemin) vaut `Unverifiable`. La liste des conteneurs doit être FRAÎCHE (`list_fresh` : moteur vu prêt, liste relue depuis son retour, suivi sans réserve, aucun conteneur illisible) ; sinon (« Prêt » publié mais liste pas encore relue, suivi perdu, lecture en échec, conteneur illisible) elle vaut `Unverifiable` : un dossier jamais effacé sur une liste qu'on n'a pas, et un conteneur illisible n'est jamais « complet ». Un dossier devenu partagé entre la fenêtre et l'effacement n'est pas effacé et compte dans `data_not_erased`. Si un autre conteneur a plus de montages que l'agent n'en lit, l'agent ne peut pas prouver qu'il n'utilise pas le dossier : conservé (« non vérifiable »). Le sort est jugé à la fenêtre, à l'acceptation ET juste avant d'effacer chaque dossier (un conteneur créé entre-temps compte). Ce cas ne bloque pas la suppression : les autres dossiers partent.

## Application (code)
- `crates/hearth-agent/src/domain/services/delete.rs::{fate_of, normalize, FolderUse, OwnFolder, Fate}`
- `crates/hearth-agent/src/application/ports/host_paths.rs::HostPaths`, `infrastructure/host/paths.rs::SystemHostPaths` (`resolve` : chemin canonique et lignée, borné dans le temps)
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::folder_users`
- `crates/hearth-agent/src/application/delete.rs::ServiceDeletion::{judge_folders, erase}`

## Vérification
- `crates/hearth-agent/src/domain/services/delete.rs::tests` (partage, parent, enfant, préfixe voisin, sans nom, doute, liste absente, chemins non canoniques, identité, parent et enfant monté sous un autre nom)
- `crates/hearth-agent/src/application/services.rs::tests::the_folders_others_mount_are_only_known_from_a_fresh_list`
- `crates/hearth-agent/src/infrastructure/host/paths.rs::tests` (lignée d'un lien, lecture abandonnée au bout du délai)
- `crates/hearth-agent/tests/service_delete_https.rs::{a_folder_another_container_mounts_is_kept_even_when_the_data_box_is_ticked, the_preview_says_who_else_uses_a_folder_and_a_container_made_outside_hearth_without_a_name, a_parent_of_the_folder_mounted_under_another_name_keeps_it, a_child_of_the_folder_mounted_under_another_name_keeps_it, parents_in_common_are_not_a_sharing, a_mount_that_cannot_be_read_keeps_the_folder_as_unverifiable, a_dead_network_share_never_hangs_the_request_the_folder_is_kept, a_container_list_that_cannot_be_trusted_keeps_the_folder}`

## Cas limites
- Choix conservateur : un doute vaut « conservé » (la spec ne traite pas le cas des montages non lus).

## Règles liées
- BR-DEL-006
- BR-ADOPT-002

## Historique
- 2026-10-24 : création (HRT-85, côté agent).
- 2026-10-24 : revue de la PR 125 (round 2) : liste fraîche exigée, parents et enfants par identité dans les deux sens, lectures bornées dans le temps.
