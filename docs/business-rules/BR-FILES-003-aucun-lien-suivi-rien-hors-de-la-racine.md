---
id: BR-FILES-003
domaine: FILES
titre: Aucune opération de fichier ne sort de la racine du service ni ne suit un lien symbolique
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-fichiers-service.md (BR-FILES-003), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md §9.1, HRT-145
maj: 2026-10-17
---

# BR-FILES-003 : Aucune opération de fichier ne sort de la racine du service ni ne suit un lien symbolique

## Règle
Toute lecture, écriture, création de dossier, mesure ou suppression d'un fichier d'un service passe par `SafeFs` : (a) un chemin est un `RelPath` jugé par le domaine AVANT tout appel au système (vide, absolu, `..`, `.`, composant vide ou octet nul refusés, bornes de longueur et de profondeur) ; (b) chaque composant est ouvert par `openat(parent, nom, O_DIRECTORY | O_NOFOLLOW)` depuis le descripteur du composant précédent, sans aucun appel par chemin composé : un lien à un niveau quelconque du chemin donne `FsError::Blocked` et sa cible n'est pas touchée ; (c) un lien planté ou remplacé par le service PENDANT l'opération ne la détourne pas (chaque ouverture est relative au descripteur déjà tenu) ; (d) le périphérique de chaque dossier traversé est comparé à celui de la racine : un point de montage est refusé (`CrossesDevice`). Le fichier cible n'est jamais suivi non plus : lire ou écrire par un lien donne `Blocked`.

## Application (code)
- `crates/hearth-agent/src/domain/files/rel_path.rs::RelPath::{parse, check_component}`
- `crates/hearth-agent/src/infrastructure/fs/unix.rs::{open_dir_on, UnixRoot::{walk, parent_of, make_dirs, write_file, read_file}}`

## Vérification
- `crates/hearth-agent/src/domain/files/rel_path.rs` (tests) : `every_bad_shape_is_refused`, `bounds_are_enforced`
- `crates/hearth-agent/src/infrastructure/fs/tests_unix.rs` : `writing_through_a_planted_directory_link_leaves_the_target_alone`, `mkdir_through_a_planted_link_leaves_the_target_alone`, `a_link_in_place_of_the_file_is_refused_and_its_target_keeps_its_content`, `a_dangling_link_with_an_outside_target_is_never_created_through`, `measure_never_follows_a_link_out_of_the_root`, `a_mount_point_is_never_crossed`, `a_directory_swapped_for_a_link_during_writes_never_leaks_outside` (course)

## Cas limites
- Un lien symbolique qui pointe à l'intérieur de la racine est refusé aussi (aucun lien n'est suivi).
- Un montage « bind » du même système de fichiers garde le même périphérique et n'est pas vu ; `openat2` avec `RESOLVE_BENEATH` n'est pas utilisé (voir ADR-0046).
- L'affichage « Lien symbolique », les messages de l'onglet Fichiers et le journal des refus sont à HRT-93 ; ici seulement l'accès.

## Règles liées
- BR-FILES-004, BR-DEPLOY-010

## Historique
- 2026-10-17 : création (HRT-145).
