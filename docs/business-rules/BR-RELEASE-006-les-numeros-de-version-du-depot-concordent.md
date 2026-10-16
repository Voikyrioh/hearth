---
id: BR-RELEASE-006
domaine: RELEASE
titre: Le numéro de version est lu dans le dépôt, jamais saisi, et tous les fichiers de version le portent
statut: active
invariant: true
source: HRT-172, contexts/hearth/conceptions/2026-10-16-fonctionnelle-publier-une-version.md (BR-RELEASE-006), 2026-10-16-technique-publier-une-version.md (0, 9.1)
maj: 2026-10-16
---

# BR-RELEASE-006 : Numéros concordants dans les cinq fichiers

## Règle
Le numéro publié est celui de `[workspace.package] version` de `Cargo.toml` (source unique) ; aucun numéro n'est saisi au lancement. `cargo xtask release-preflight` refuse, avant toute construction, si un fichier ne porte pas le même numéro :
- `Cargo.toml` : `[workspace.package].version` (la source) ;
- `Cargo.lock` : la version des cinq paquets `hearth-agent`, `hearth-desktop`, `hearth-link`, `hearth-proto`, `xtask` ;
- `apps/desktop/package.json` : `version` ;
- `apps/desktop/package-lock.json` : `version` de la racine ET `packages[""].version` ;
- `apps/desktop/src-tauri/tauri.conf.json` : AUCUN champ `version` (Tauri prend alors celui du crate ; un champ l'emporterait sur Cargo, même s'il vaut le bon numéro, donc le refus).

Un fichier manquant, illisible ou sans numéro est une discordance, jamais un succès. Le numéro commun doit être `X.Y.Z` strict (`release_core::check_version` : pas de préversion, pas de zéro en tête). Message exact : « Les numéros de version du dépôt ne concordent pas : {fichier → numéro, …}. », tous les fichiers listés dans l'ordre ci-dessus (un fichier qui porte plusieurs numéros divergents est listé numéro par numéro : `Cargo.lock (xtask) → 0.3.0`, `package-lock.json (packages[""]) → …`) ; « aucun numéro » quand rien n'est lu.

La même vérification tourne à chaque PR (`ci.yml`, job `rust` : `cargo xtask release-preflight --versions-only`, sans réseau) : une montée de version incomplète est vue à la PR, pas au lancement du flux.

## Application (code)
- `xtask/src/release_preflight.rs::version_files` : lit les cinq fichiers (`locked_versions`, `grouped`).
- `xtask/src/release_preflight.rs::check_versions` : concordance, absence de numéro dans `tauri.conf.json`, forme `X.Y.Z`, message.
- `xtask/src/release_core.rs::{workspace_version, check_version}` : lecture de la source et forme du numéro.

## Vérification
- `xtask/src/release_preflight.rs` : `a_coherent_repository_gives_its_version`, `the_five_locked_packages_and_both_lock_fields_are_read`, `each_file_that_disagrees_is_refused_and_named`, `a_version_in_tauri_conf_is_refused_even_when_it_is_the_right_one`, `a_missing_or_garbled_file_is_a_disagreement_not_a_pass`, `the_message_lists_every_file_in_the_order_of_the_conception`, `a_version_that_is_not_strict_is_refused_even_when_every_file_agrees`, `this_repository_passes_the_versions_only_checks` (le dépôt lui-même).
- À la main : `cargo xtask release-preflight --versions-only`.

## Cas limites
- `Cargo.toml` est la source : s'il est illisible, tous les fichiers sont en discordance.
- Une dépendance dont la `version` ressemble au numéro (`[workspace.dependencies]`, paquet étranger de `Cargo.lock`) n'est jamais lue.

## Règles liées
- BR-RELEASE-004 et 008 (autres contrôles préalables), BR-RELEASE-003 (la version assemblée est celle du dépôt).

## Historique
- 2026-10-16 : création (HRT-172).
