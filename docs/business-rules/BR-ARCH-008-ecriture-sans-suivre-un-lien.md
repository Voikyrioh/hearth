---
id: BR-ARCH-008
domaine: ARCH
titre: L'extraction écrit sous un dossier neuf sans jamais suivre un lien, puis bascule
statut: active
invariant: true
source: ADR-0055 (contexts/hearth/conceptions/2026-10-13-adr-lecture-d-une-archive-brouillon.md), contexts/hearth/tickets/hrt/HRT-130.md
maj: 2026-10-14
---

# BR-ARCH-008 — L'extraction écrit sous un dossier neuf sans jamais suivre un lien, puis bascule

## Règle
L'appelant donne une `Destination` : une racine de CONFIANCE absolue et un chemin relatif dont le dernier composant est le nom (qui ne doit pas exister). **Tout se fait ensuite par descripteur depuis le dossier PARENT** : le parent est ouvert une fois (`O_DIRECTORY | O_NOFOLLOW`, composant par composant depuis la racine), puis le dossier d'attente est créé (`mkdirat`, nom `.nom.staging-{ULID}` imprévisible, `0700`), ouvert, rempli, rendu durable (`fsync` de chaque fichier, de chaque dossier, du dossier d'attente), basculé par `renameat2(RENAME_NOREPLACE)` des deux noms relatifs au MÊME descripteur de parent, puis le parent est synchronisé. Aucun appel par chemin n'a lieu après l'ouverture du parent.

**Précondition du parent, vérifiée sur son descripteur (`fstat`) et refusée sinon (`UNSAFE_DESTINATION`) : il appartient à l'utilisateur effectif de l'agent et n'est inscriptible ni par le groupe ni par les autres (`mode & 0o022 == 0`).** Un lien en dernier composant de la racine ou en composant du chemin relatif, un composant absent ou qui n'est pas un dossier : `UNSAFE_DESTINATION`. Une destination qui existe déjà (fichier, dossier, lien) : `WRITE_FAILED`, rien n'est créé.

**Bascule sans remplacement** : si le noyau ou le système de fichiers ne prend pas en charge l'interdiction de remplacer (`EINVAL`, `ENOSYS`, `ENOTSUP`), le dossier d'attente est retiré et l'extraction est refusée (`ATOMIC_SWITCH_UNAVAILABLE`) ; jamais de repli par un `rename` ordinaire. Un objet apparu à la destination entre le contrôle et la bascule fait échouer la bascule (`WRITE_FAILED`).

Dans le dossier d'attente, chaque dossier traversé est ouvert par `openat(parent, composant, O_NOFOLLOW | O_DIRECTORY)` depuis le descripteur du précédent et chaque fichier est créé par `O_CREAT | O_EXCL | O_NOFOLLOW` : un lien rencontré est `Blocked`, un objet déjà présent `AlreadyExists`, rien n'est écrit au travers.

**Nettoyage** (refus, panne, annulation, échec après la bascule) : par descripteur (`unlinkat`), sans suivre de lien (un lien est retiré, jamais traversé), et SEULEMENT si le dossier trouvé sous le nom est celui que l'opération a créé (périphérique et inode comparés à ceux relevés à la création) ; sinon rien n'est touché. Hors Unix, l'écriture est refusée (`ATOMIC_SWITCH_UNAVAILABLE`) : aucun repli par chemin.

## Application (code)
- `crates/hearth-agent/src/infrastructure/archive_fs.rs::extract_file`
- `crates/hearth-agent/src/infrastructure/archive_fs.rs::Destination`
- `crates/hearth-agent/src/infrastructure/archive_fs/unix.rs::extract_into`
- `crates/hearth-agent/src/infrastructure/archive_fs/unix.rs::check_parent`
- `crates/hearth-agent/src/infrastructure/archive_fs/unix.rs::switch`
- `crates/hearth-agent/src/infrastructure/archive_fs/unix.rs::remove_created_dir`
- `crates/hearth-agent/src/infrastructure/archive_fs/unix.rs::DirectorySink`
- `crates/hearth-agent/src/domain/archive/sink.rs::ExtractSink`

## Vérification
- `crates/hearth-agent/tests/archive_fs.rs::unix::an_archive_is_unpacked_into_a_new_folder_by_a_switch_and_nothing_else_is_left (CI Linux)`
- `crates/hearth-agent/tests/archive_fs.rs::unix::a_destination_below_existing_folders_is_reached_one_component_at_a_time (CI Linux)`
- `crates/hearth-agent/tests/archive_fs.rs::a_refused_archive_creates_nothing_not_even_a_staging_folder`
- `crates/hearth-agent/tests/archive_fs.rs::a_destination_needs_an_absolute_trusted_root_and_plain_relative_names`
- `crates/hearth-agent/tests/archive_fs.rs::writing_is_refused_outside_unix_there_is_no_unsafe_fallback (hors Unix)`
- `crates/hearth-agent/tests/archive_fs.rs::unix::an_existing_destination_is_never_replaced (CI Linux)`
- `crates/hearth-agent/tests/archive_fs.rs::unix::a_parent_the_group_or_others_can_write_into_is_refused_and_nothing_is_created (CI Linux)`
- `crates/hearth-agent/tests/archive_fs.rs::unix::a_parent_owned_by_someone_else_is_refused (CI Linux, se saute en root)`
- `crates/hearth-agent/tests/archive_fs.rs::unix::a_link_on_the_way_to_the_destination_is_never_followed (CI Linux)`
- `crates/hearth-agent/tests/archive_fs.rs::unix::a_symlink_already_in_the_destination_is_never_followed (CI Linux)`
- `crates/hearth-agent/tests/archive_fs.rs::unix::an_existing_hard_link_is_never_written_through (CI Linux)`
- `crates/hearth-agent/tests/archive_fs.rs::unix::files_are_created_new_in_folders_created_on_the_way_and_flush_makes_them_durable (CI Linux)`
- `crates/hearth-agent/src/infrastructure/archive_fs/tests_unix.rs::the_switch_never_replaces_an_existing_object (CI Linux)`
- `crates/hearth-agent/src/infrastructure/archive_fs/tests_unix.rs::a_system_without_the_no_replace_option_is_a_clean_refusal_never_a_silent_replacement (CI Linux)`
- `crates/hearth-agent/src/infrastructure/archive_fs/tests_unix.rs::the_parent_must_be_the_agent_s_own_and_writable_by_nobody_else (CI Linux)`
- `crates/hearth-agent/src/infrastructure/archive_fs/tests_unix.rs::the_parent_is_opened_component_by_component_without_following_a_link (CI Linux)`
- `crates/hearth-agent/src/infrastructure/archive_fs/tests_unix.rs::cleanup_removes_what_the_staging_holds_and_never_follows_a_link (CI Linux)`
- `crates/hearth-agent/src/infrastructure/archive_fs/tests_unix.rs::cleanup_touches_nothing_when_the_folder_is_not_the_one_that_was_created (CI Linux)`
- `crates/hearth-agent/src/infrastructure/archive_fs/tests_unix.rs::cleanup_does_not_remove_a_link_standing_where_the_staging_was (CI Linux)`

## Cas limites
- La racine de confiance est celle de l'appelant : ses ancêtres ne sont pas vérifiés (seul le parent final l'est).
- Un arrêt brutal laisse un dossier d'attente que personne ne ramasse encore (ticket de suivi) ; le journal des opérations (inode du dossier d'attente) est l'affaire de l'appelant.
- La durabilité (`fsync`) dépend du système de fichiers ; aucun test ne coupe le courant.
- Non prouvé sur le poste de développement (Windows) : tout le code Unix est vérifié à la compilation pour la cible Linux (copie isolée, `clippy --all-targets -D warnings`) mais ne s'exécute qu'en CI Linux.

## Règles liées
- BR-ARCH-001, BR-ARCH-002, BR-ARCH-003, BR-ARCH-004, BR-ARCH-005, BR-ARCH-006, BR-ARCH-007, BR-ARCH-009, BR-ARCH-010, BR-ARCH-011

## Historique
- 2026-10-13 — création (HRT-130, ADR-0055, lecture d'une archive hostile).
- 2026-10-14 — parent ouvert une fois, tout par descripteur, précondition du parent, `RENAME_NOREPLACE`, nettoyage par identité, `fsync`, hors Unix refusé (revue de la PR 88).
