---
id: BR-FILES-004
domaine: FILES
titre: Un fichier spécial n'est ni lu ni écrit ; un fichier à plusieurs noms est lu mais jamais modifié
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-fichiers-service.md (BR-FILES-004), HRT-145
maj: 2026-10-17
---

# BR-FILES-004 : Un fichier spécial n'est ni lu ni écrit ; un fichier à plusieurs noms est lu mais jamais modifié

## Règle
Un tube nommé, un socket, un périphérique ou un lien n'est jamais ouvert ni remplacé (`NotRegular` ou `Blocked`) : il est désigné sans être ouvert (`O_PATH | O_NOFOLLOW`) et jugé par `fstat` de ce descripteur, ce qui ne bloque jamais sur un tube et ne déclenche jamais l'effet d'ouverture d'un périphérique ; seul un fichier ordinaire est rouvert en lecture (`/proc/self/fd/<n>`, même objet, identité revérifiée). Un fichier ordinaire dont le nombre de noms (`st_nlink`) dépasse un se lit et se supprime (cela ne retire que ce nom) mais `write_atomic` le refuse (`Hardlinked`). L'écriture est atomique : fichier d'attente `.hearth-tmp-<ULID>` du MÊME dossier (création exclusive, sans suivre de lien), propriétaire et droits posés sur le descripteur AVANT la première donnée (jamais de bit setuid, setgid ni sticky : `Perms` les refuse), `fsync`, `renameat`, `fsync` du dossier ; en cas d'échec le fichier d'attente est retiré.

## Application (code)
- `crates/hearth-agent/src/infrastructure/fs/unix.rs::{UnixRoot::{write_file, read_file}, fill_and_place, apply}`
- `crates/hearth-agent/src/application/ports/safe_fs.rs::{Perms::new, FsError}`

## Vérification
- `crates/hearth-agent/src/infrastructure/fs/tests_unix.rs` : `a_file_with_several_names_is_readable_but_never_modified`, `a_named_pipe_is_neither_read_nor_overwritten_and_nothing_blocks`, `a_device_node_is_judged_without_being_opened` (root), `a_directory_in_place_of_the_file_is_refused`, `write_then_read_round_trips_and_leaves_no_staging_file`, `a_written_file_never_carries_a_special_bit`, `a_read_over_the_limit_is_refused_and_exactly_the_limit_is_accepted`

## Cas limites
- Entre le contrôle et le renommage, le service peut encore écrire l'ancien fichier : la course est dite, pas fermée (le repère de version de l'éditeur est à HRT-93).
- Le propriétaire est posé par `fchown` : sans root, seul son propre identifiant est accepté.

## Règles liées
- BR-FILES-003

## Historique
- 2026-10-17 : création (HRT-145).
- 2026-10-17 : un fichier à lire est jugé AVANT d'être ouvert (`O_PATH`), review de la PR #101.
