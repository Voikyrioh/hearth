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
Un dossier géré qu'un autre conteneur monte (le même chemin, un parent ou un enfant), en marche ou arrêté, créé par Hearth, adopté ou non, est CONSERVÉ avec le nom des services concernés ; un conteneur créé hors de Hearth est dit « un conteneur créé hors de Hearth », sans nom (BR-ADOPT-002). Les chemins sont comparés sous leur forme lexicale (`//`, `.`, `..`) ET canonique des deux côtés (dossier de données canonique lu au démarrage, source du moteur canonisée quand elle existe), et par identité (périphérique, inode) quand le dossier existe : un lien ou un montage « bind » ne cache pas le partage. La liste des conteneurs absente ou pas prête (moteur qui n'est plus « Prêt ») vaut `Unverifiable` : un dossier jamais effacé sur une liste qu'on n'a pas. Un dossier devenu partagé entre la fenêtre et l'effacement n'est pas effacé et compte dans `data_not_erased`. Si un autre conteneur a plus de montages que l'agent n'en lit, l'agent ne peut pas prouver qu'il n'utilise pas le dossier : conservé (« non vérifiable »). Le sort est jugé à la fenêtre, à l'acceptation ET juste avant d'effacer chaque dossier (un conteneur créé entre-temps compte). Ce cas ne bloque pas la suppression : les autres dossiers partent.

## Application (code)
- `crates/hearth-agent/src/domain/services/delete.rs::{fate_of, normalize, FolderUse, OwnFolder, Fate}`
- `crates/hearth-agent/src/application/ports/host_paths.rs::HostPaths`, `infrastructure/host/paths.rs::SystemHostPaths`
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::folder_users`
- `crates/hearth-agent/src/application/delete.rs::ServiceDeletion::{judge_folders, erase}`

## Vérification
- `crates/hearth-agent/src/domain/services/delete.rs::tests` (partage, parent, enfant, préfixe voisin, sans nom, doute, liste absente, chemins non canoniques, identité)
- `crates/hearth-agent/tests/service_delete_https.rs::{a_folder_another_container_mounts_is_kept_even_when_the_data_box_is_ticked, the_preview_says_who_else_uses_a_folder_and_a_container_made_outside_hearth_without_a_name}`

## Cas limites
- Choix conservateur : un doute vaut « conservé » (la spec ne traite pas le cas des montages non lus).

## Règles liées
- BR-DEL-006
- BR-ADOPT-002

## Historique
- 2026-10-24 : création (HRT-85, côté agent).
