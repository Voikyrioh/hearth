---
id: BR-MOD-008
domaine: MOD
titre: Un pack refusé ne crée rien, est retiré de la zone d'attente aussitôt, et dit son motif par un code fixe
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-mods-et-packs.md (BR-MOD-008), HRT-124
maj: 2026-11-15
---

# BR-MOD-008 : Un pack refusé ne crée rien, est retiré de la zone d'attente aussitôt, et dit son motif par un code fixe

## Règle
Motifs : archive illisible ou hors bornes (`ARCHIVE_REFUSED`, code de l'ADR-0055) ; pack Modrinth dont un téléchargement n'est pas en `https` ou vise un hôte hors des quatre du format (`cdn.modrinth.com`, `github.com`, `raw.githubusercontent.com`, `gitlab.com`) : `PACK_HOST_FORBIDDEN` avec l'hôte (réduit à l'alphabet d'un nom d'hôte, sans chemin ni paramètre) ; chemin qui sort du dossier du jeu (`PACK_PATH_ESCAPES`) ; description illisible, d'un autre jeu ou d'un autre format (`PACK_INDEX_INVALID`) ; pack serveur rangé tout entier dans un dossier (`PACK_WRAPPED_IN_FOLDER`) ; genre non reconnu (`PACK_UNRECOGNIZED`) ; liste de téléchargements CurseForge (`PACK_IS_A_DOWNLOAD_LIST`, BR-MOD-016). Réponse `422 PACK_REFUSED`, `details.reason` ; un dépôt inconnu (ou d'un autre compte) est `404 PACK_UNKNOWN` et n'est jamais retiré. Aucun nom d'entrée de l'archive ne figure dans une réponse ni dans le journal. Un pack accepté mais une installation refusée pour une autre cause (nom pris, variante inadaptée) laisse le dépôt en place : l'utilisateur corrige sans redéposer.

## Application (code)
- `crates/hearth-agent/src/domain/mods/pack.rs::{judge_pack, PackRefusal::code}`
- `crates/hearth-agent/src/application/pack_intake.rs::PackError`
- `crates/hearth-agent/src/entrypoint/http/deployments.rs::deploy_error`

## Vérification
- crates/hearth-agent/src/domain/mods/pack.rs (tests) : `a_modrinth_download_outside_the_four_hosts_or_not_https_is_refused`, `a_modrinth_path_leaving_the_game_folder_is_refused`, `a_pack_wrapped_in_one_folder_is_refused_with_its_own_reason`
- crates/hearth-agent/src/application/pack_intake.rs (tests) : `a_hostile_archive_is_refused_whole_by_the_archive_reader_and_removed`
- crates/hearth-agent/tests/pack_install_https.rs : `a_hostile_archive_and_an_unlisted_modrinth_host_are_refused_with_a_fixed_reason`, `an_install_with_a_pack_is_refused_whole_when_the_pack_is_refused_or_unknown_or_not_offered`

## Cas limites
- Le message d'espace insuffisant est celui de l'installation (`NO_SPACE`).

## Règles liées
- BR-MOD-007, BR-MOD-016, BR-ARCH-003

## Historique
- 2026-11-15 : création (HRT-124).
