---
id: BR-RELEASE-011
domaine: RELEASE
titre: L'essai à blanc montre ce qui serait déposé et n'écrit rien
statut: active
invariant: true
source: HRT-174, contexts/hearth/conceptions/2026-10-16-fonctionnelle-publier-une-version.md (BR-RELEASE-011), 2026-10-16-technique-publier-une-version.md (7.2, 9.5)
maj: 2026-10-17
---

# BR-RELEASE-011 : Essai à blanc

## Règle
`cargo xtask release-upload` SANS `--confirm` est l'essai à blanc : il montre, sans rien écrire dans le bucket ni sur GitHub et sans lancer `aws` ni `curl` (seul `git`, en lecture, vérifie l'arbre et le commit), chaque fichier qui serait déposé (clé, somme, taille, type), le script et le manifeste. Première ligne exacte : « Essai à blanc : rien n'a été déposé. Fichiers et manifeste qui l'auraient été ci-dessous. ». En mode GitHub, le compte rendu entier va au résumé de l'exécution (« Essai à blanc du dépôt »). La publication réelle (`--confirm`) lance en plus `aws --version` en première ligne (BR-RELEASE-009).

Le reste de l'essai à blanc d'un flux (construire, signer, assembler sans artefact signé, sans clé du bucket, sans `release-github`) est dans le workflow (HRT-175).

## Application (code)
- `xtask/src/release_publish.rs::{dry_run_report, publish, run_upload}`.

## Vérification
- `xtask/src/release_publish.rs` : `the_dry_run_shows_every_write_and_calls_nothing`, `the_dry_run_lists_the_files_and_the_manifest_and_writes_nothing`, `the_dry_run_says_when_the_rollback_option_is_armed`, `the_upload_guard_refuses_before_any_aws_or_curl_with_and_without_confirm`.

## Cas limites
- La relecture publique de `release-upload` n'est exercée qu'en réel ; l'essai à blanc ne dit rien de la chaîne Cloudflare (le manifeste public est lu dès `release-preflight`).

## Règles liées
- BR-RELEASE-001, BR-RELEASE-002, BR-RELEASE-009, BR-RELEASE-010

## Historique
- 2026-10-17 : création (HRT-174).
