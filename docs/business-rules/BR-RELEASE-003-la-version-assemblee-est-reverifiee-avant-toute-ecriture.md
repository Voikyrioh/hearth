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
- l'arbre de travail est son commit (HRT-60), vérifié AVANT toute écriture : aucun fichier suivi modifié (indexé ou non, `git status --porcelain --untracked-files=no`), aucun fichier non suivi et non ignoré sous les entrées de la construction (`Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `.cargo`, `.sqlx`, `crates` : `git ls-files --others --exclude-standard`, parce qu'une migration embarquée par `sqlx::migrate!` ou un `build.rs` non suivi entre dans le binaire), aucun fichier IGNORÉ sous ces mêmes chemins (`git ls-files --others --ignored --exclude-standard` : l'ignorance peut venir de `.git/info/exclude` ou de `core.excludesFile`, locaux au poste, et rien d'ignoré n'y vit légitimement ; vérifié vide sur le worktree et sur le clone principal), aucun fichier masqué (`git ls-files -v`, lettre `S` ou minuscule). Les autres fichiers non suivis sont tolérés. Un git qui échoue refuse, jamais « propre ». `HEAD` est lu au début (`check_clean_tree`), relu pour le script et comparé (refus si différent), puis le script est lu par `git show <ce commit>:deploy/install.sh`, donc épinglé au commit vérifié. Le commit est affiché et enregistré dans `target/release-stage/COMMIT` (hors de `releases/`, jamais déposé, ignoré par la liste fermée) ; `release-upload`, essai à blanc compris, refuse un enregistrement absent, un arbre pas propre ou un commit différent (seul `git`, en lecture, y est lancé) ;
- le binaire de l'agent est celui qu'a construit `cargo xtask agent` sur ce commit : `target/dist/hearth-agent.build` (`commit`, `tree`, `sha256`, `version`) doit exister à côté du binaire donné, avoir la somme de ce fichier, le commit de `HEAD` et `tree=clean` (arbre propre avant et après la construction, même commit). Sinon refus, sans option de contournement. `cargo xtask agent` n'échoue jamais pour cela (avertissements) ;
- l'agent est rangé dans le dossier de SA version : `agent.version` est le dossier qui contient le binaire, et l'adresse du manifeste vaut exactement `{préfixe}{agent.version}/hearth-agent-linux-x86_64` (contrainte de `deploy/install.sh` sur le pointeur). Sinon refus.

## Application (code)
- `xtask/src/release_stage.rs::build_stage`, `::verify_stage` (et `check_manifest`, `check_sums`, `check_install_script`), `::check_script_is_committed`, `::check_clean_tree` (`::tree_problems`, `::tracked_changes`, `::untracked_inputs`, `::hidden_changes`, `::ignored_inputs`, `BUILD_INPUTS`), `::check_agent_build`, `::check_stage_commit`, `::write_stage_commit`, `::read_stage_commit`, `::stage_with`, `::write_stage_dir` ; `release_publish.rs::upload_with` ; `agent.rs::BuildRecord`, `::write_build_record`, `::report_tree` ; `release_publish.rs::run_upload` (revérification avant dépôt).
- Primitives : `xtask/src/release_core.rs::verify_signature`, `::verify_agent_signature`, `::manifest`, `::agent_section`.

## Vérification
- `xtask/src/release_stage.rs` : `the_manifest_has_the_exact_shape_the_api_reads`, `a_wrong_sum_is_refused_and_names_the_file`, `a_signature_of_another_key_is_refused_for_the_client_and_for_the_agent`, `a_signed_version_other_than_the_one_of_the_repository_is_refused`, `a_name_outside_the_closed_list_is_refused`, `an_agent_not_stored_in_the_folder_of_its_version_is_refused`, `the_manifest_of_another_shape_is_refused`, `the_development_key_and_a_mixed_up_artifact_are_refused`, `a_script_that_would_not_read_the_pointer_or_is_empty_is_refused`, `the_repository_script_must_be_the_committed_one_and_its_commit_is_shown`, `a_git_that_cannot_answer_stops_the_script_check`, `a_clean_tree_is_accepted_and_its_commit_returned_with_read_only_calls`, `a_tracked_change_staged_or_not_is_refused_and_names_the_files`, `the_list_of_changed_files_is_bounded`, `a_git_that_fails_is_never_read_as_a_clean_tree`, `the_commit_record_is_read_back_and_is_not_part_of_the_deposited_files`, `an_upload_on_another_commit_or_without_record_or_on_a_dirty_tree_is_refused`, `a_missing_or_garbled_record_file_is_an_error` ; `agent.rs` : `a_clean_tree_prints_the_commit_and_no_warning`, `a_modified_tree_warns_but_never_fails`, `an_untracked_build_input_warns`, `a_git_that_cannot_answer_warns_and_does_not_stop_the_build`, `a_git_that_cannot_even_be_launched_warns_twice_and_never_fails`, `the_record_is_rendered_and_parsed_back_and_refuses_anything_else`, `the_recorded_state_is_clean_only_when_both_surveys_are_clean_on_the_same_commit`, `the_record_written_after_a_build_matches_the_file_and_a_failure_to_write_is_an_error` ; branchement : `the_stage_refuses_a_tree_that_is_not_its_commit_before_writing_anything`, `the_stage_of_a_clean_tree_with_a_matching_build_writes_the_commit_record`, `the_stage_refuses_an_agent_without_a_matching_build_before_writing`, `release_publish.rs::the_upload_guard_refuses_before_any_aws_or_curl_with_and_without_confirm` ; arbre : `an_untracked_file_among_the_build_inputs_is_refused_and_named`, `an_ignored_file_among_the_build_inputs_is_refused_and_named`, `the_untracked_check_is_limited_to_the_build_inputs_and_honours_gitignore`, `a_file_hidden_from_git_is_refused`, `git_is_asked_without_optional_locks`, `the_script_of_another_head_is_refused`, `an_agent_binary_needs_a_matching_build_record`.

## Cas limites
- Une version du client seul qui garderait la section `agent` de la précédente (conception 5.4) n'est PAS assemblée par cette commande : ici l'agent et le client partagent la version du dépôt.
- Le test qui fige la forme du manifeste est un miroir écrit à la main de `manifest.ts` : si l'API change, il faut le changer aussi.

## Limites connues
- Le contrôle de l'arbre est fait à l'assemblage et au dépôt, pas pendant la construction ; l'enregistrement de construction (relevés avant et après) borne cette fenêtre. Entre les lectures git et celles de `Cargo.toml` et des clés, une petite fenêtre reste.
- L'enregistrement `.build` est un fichier en clair, falsifiable à la main : une garde contre l'erreur, pas contre la malveillance. Un binaire construit ailleurs n'en a pas, donc refus (voulu tant que le runbook dit de construire sur place).
- Hors de cette garde : l'installateur du client vient de l'artefact du flux CI (rien ne le relie à ce commit), l'image de conteneur est figée par empreinte, mais les paquets `apk` installés au lancement et les volumes de cache de Cargo (`hearth-xtask-cargo`, `hearth-xtask-target`, persistants, fraîcheur par dates de modification : une restauration qui garde une date ancienne ne recompile pas) ne sont pas vus par cette garde, et rien ne vérifie que `HEAD` est le commit qui sera étiqueté (relecture à l'oeil du `headSha`, runbook).
- Avec `--stage-dir`, `COMMIT` est lu dans le dossier donné et comparé au `HEAD` du dépôt de `xtask` : un dossier fabriqué à la main demande un `COMMIT` fabriqué à la main, même confiance que le reste du dossier, que `verify_stage` revérifie.
- `git status` peut dépendre de `core.fsmonitor` : non vérifié ; toutes les lectures git de `release_stage.rs` (état de l'arbre, `rev-parse`, contrôle du script, `git show`) passent `GIT_OPTIONAL_LOCKS=0`.

## Règles liées
- BR-RELEASE-001, BR-RELEASE-002, BR-UPDATE-035, ADR-0035

## Historique
- 2026-10-12 : création (HRT-51).
- 2026-10-12 : correctifs de la review de la PR 73 (round 1).
- 2026-10-12 : arbre de travail propre exigé, commit enregistré et revérifié au dépôt (HRT-60).
- 2026-10-12 : review de la PR 76 (round 1) : fichiers non suivis des entrées de la construction et fichiers masqués refusés, enregistrement de construction du binaire de l'agent, garde branchée testée, `HEAD` lu une fois.
