---
id: BR-RELEASE-003
domaine: RELEASE
titre: La version assemblée est revérifiée en entier avant d'être écrite ou déposée ; l'agent est rangé dans le dossier de sa version
statut: active
invariant: true
source: HRT-51 (ajout du 2026-10-12, review de la PR 70), contexts/hearth/conceptions/2026-10-09-distribution-scaleway.md (sections 5.2, 5.4, 8.2)
maj: 2026-10-12
---

# BR-RELEASE-003 : Revérification avant d'écrire

## Règle
`cargo xtask release-stage` n'écrit rien (et seulement sous `target/release-stage/`) tant que tout n'est pas vérifié, et `release-upload` rejoue la même vérification sur le dossier lu au moment de déposer :
- version = celle de `[workspace.package]` ;
- signature du client contre `apps/desktop/src-tauri/update-key.pub` (version signée comprise), clé de développement refusée ;
- signature de l'agent contre `crates/hearth-agent/update-key.pub`, commentaire signé `version:X.Y.Z` égal à la version (BR-UPDATE-035) ;
- sommes : `SHA256SUMS` (format de `sha256sum`, trié, tous les fichiers de la version sauf lui-même), `.sha256` de l'agent, somme de l'agent dans le manifeste ;
- adresses exactement `{préfixe}{X.Y.Z}/{fichier}` ;
- noms dans la liste fermée de l'API (8 noms) et rien d'autre ;
- forme exacte du manifeste de la conception 5.4, aucun champ en plus ni en moins (`manifest.ts` de `hearth-releases-api` ignore les champs en plus : l'outil est plus strict que l'API, jamais plus lâche) ;
- script `releases/install.sh` : non vide, texte, première ligne `#!/bin/sh`, contient `releases/latest/agent` et `hearth-agent-linux-x86_64` (sinon c'est l'ancien script ou un mauvais fichier, refus) ; sans `--install-script`, `deploy/install.sh` doit être identique à `HEAD` (le commit est affiché) ;
- l'agent est rangé dans le dossier de SA version : `agent.version` est le dossier qui contient le binaire, et l'adresse du manifeste vaut exactement `{préfixe}{agent.version}/hearth-agent-linux-x86_64` (contrainte de `deploy/install.sh` sur le pointeur). Sinon refus.

## Application (code)
- `xtask/src/release_stage.rs::build_stage`, `::verify_stage` (et `check_manifest`, `check_sums`, `check_install_script`), `::check_script_is_committed`, `::write_stage_dir` ; `release_publish.rs::run_upload` (revérification avant dépôt).
- Primitives : `xtask/src/release_core.rs::verify_signature`, `::verify_agent_signature`, `::manifest`, `::agent_section`.

## Vérification
- `xtask/src/release_stage.rs` : `the_manifest_has_the_exact_shape_the_api_reads`, `a_wrong_sum_is_refused_and_names_the_file`, `a_signature_of_another_key_is_refused_for_the_client_and_for_the_agent`, `a_signed_version_other_than_the_one_of_the_repository_is_refused`, `a_name_outside_the_closed_list_is_refused`, `an_agent_not_stored_in_the_folder_of_its_version_is_refused`, `the_manifest_of_another_shape_is_refused`, `the_development_key_and_a_mixed_up_artifact_are_refused`, `a_script_that_would_not_read_the_pointer_or_is_empty_is_refused`, `the_repository_script_must_be_the_committed_one_and_its_commit_is_shown`, `a_git_that_cannot_answer_stops_the_script_check`.

## Cas limites
- Une version du client seul qui garderait la section `agent` de la précédente (conception 5.4) n'est PAS assemblée par cette commande : ici l'agent et le client partagent la version du dépôt.
- Le test qui fige la forme du manifeste est un miroir écrit à la main de `manifest.ts` : si l'API change, il faut le changer aussi.

## Règles liées
- BR-RELEASE-001, BR-RELEASE-002, BR-UPDATE-035, ADR-0035

## Historique
- 2026-10-12 : création (HRT-51).
- 2026-10-12 : correctifs de la review de la PR 73 (round 1).
