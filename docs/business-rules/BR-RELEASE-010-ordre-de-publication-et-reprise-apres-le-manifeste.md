---
id: BR-RELEASE-010
domaine: RELEASE
titre: Ordre de publication (fichiers, script, manifeste, étiquette, release) et reprise après le manifeste
statut: active
invariant: true
source: HRT-174, contexts/hearth/conceptions/2026-10-16-fonctionnelle-publier-une-version.md (BR-RELEASE-010), 2026-10-16-technique-publier-une-version.md (7.2, 9.5)
maj: 2026-10-17
---

# BR-RELEASE-010 : Ordre de publication et reprise après le manifeste

## Règle
Ordre : fichiers de la version, script d'installation, manifeste (BR-RELEASE-002), PUIS étiquette et release GitHub. Un échec après le manifeste se reprend en ne créant que ce qui manque : `cargo xtask release-github --notes-file F [--stage-dir D]` (env `HEARTH_SHA`, `GH_TOKEN`) lit `COMMIT` et le manifeste assemblés, vérifie que le commit est `HEARTH_SHA`, puis lit l'étiquette `vX.Y.Z` d'`origin` (`git ls-remote --exit-code`, avec la ligne dépouillée d'une étiquette annotée) et la release (`GET /repos/Voikyrioh/hearth/releases/tags/vX.Y.Z`, code HTTP exact) :

| Étiquette | Release | Action |
|---|---|---|
| absente | absente | `gh release create vX.Y.Z --target <commit> --title "Hearth X.Y.Z" --notes-file F SHA256SUMS` |
| sur le commit | absente | `gh release create vX.Y.Z --verify-tag …` |
| sur le commit | présente, `SHA256SUMS` joint | rien |
| sur le commit | présente, `SHA256SUMS` absent | `gh release upload vX.Y.Z SHA256SUMS` |
| sur un AUTRE commit | — | arrêt, « L'étiquette vX.Y.Z pointe sur le commit … et non sur … : à examiner à la main. », rien de modifié |
| absente | présente | arrêt, à examiner à la main |
| lecture en échec | — | arrêt |

Toute erreur de lecture ou de création (hors étiquette sur un autre commit) donne, en première ligne, « Version déposée et annoncée, mais la release GitHub n'a pas pu être créée : relancez l'étape en échec. ». La création est TOUJOURS relue (release présente, `SHA256SUMS` joint), puis le manifeste public est relu : il doit annoncer X.Y.Z. Fin : « Version X.Y.Z publiée : manifeste à jour, release vX.Y.Z créée. », avec au résumé le commit, les fichiers déposés et leur somme, l'adresse du manifeste et le lien de la release.

## Application (code)
- `xtask/src/release_github.rs::{read_staged, tag_commit, decide, ensure_release, success_summary}`, `xtask/src/release_preflight.rs::fetch_release` (lecture partagée avec `lookup_release`).

## Vérification
- `xtask/src/release_github.rs` : `nothing_there_creates_the_tag_and_the_release_on_the_commit_then_rereads`, `a_complete_release_is_recognised_and_nothing_is_created`, `a_release_without_its_sums_gets_them_uploaded`, `a_tag_on_the_commit_without_release_is_verified_not_moved`, `a_tag_on_another_commit_stops_and_touches_nothing`, `a_release_without_a_readable_tag_stops`, `a_failed_reading_stops_with_the_message_of_the_spec`, `a_failing_creation_says_to_rerun_the_failed_step`, `a_creation_that_cannot_be_read_back_is_not_believed`, `the_public_manifest_must_announce_the_version`, `a_second_run_after_a_full_success_creates_nothing_more`, `the_staged_commit_must_be_the_launched_one`, `the_annotated_tag_is_read_through_its_peeled_line`, `empty_notes_are_refused_before_anything_is_read`, `the_success_summary_lists_the_commit_the_files_and_the_links`.

## Cas limites
- Une release en brouillon n'est pas vue par l'API des étiquettes (comme BR-RELEASE-008).
- `gh` lit `GH_TOKEN` dans l'environnement ; le jeton ne passe jamais en argument ni dans un message (la lecture HTTP le passe par l'entrée standard de `curl`).
- Le job de workflow qui l'appelle (droit d'écriture `contents`, publication réelle seulement) vient avec HRT-175.

## Règles liées
- BR-RELEASE-002, BR-RELEASE-007, BR-RELEASE-008, BR-RELEASE-009, BR-RELEASE-011

## Historique
- 2026-10-17 : création (HRT-174).
