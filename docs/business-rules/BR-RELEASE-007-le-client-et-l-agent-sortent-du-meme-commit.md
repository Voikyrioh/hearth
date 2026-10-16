---
id: BR-RELEASE-007
domaine: RELEASE
titre: Le client et l'agent d'une version sont construits depuis le même commit, sur un arbre propre, et prouvent leur état par un enregistrement
statut: active
invariant: true
source: HRT-173, contexts/hearth/conceptions/2026-10-16-fonctionnelle-publier-une-version.md (BR-RELEASE-007), 2026-10-16-technique-publier-une-version.md (7.2, 9.4, 9.6)
maj: 2026-10-16
---

# BR-RELEASE-007 : Client et agent du même commit, état de construction enregistré et vérifié

## Règle
Le client et l'agent d'une version sortent du MÊME commit, et l'étiquette `vX.Y.Z` est posée sur ce commit. La signature et l'assemblage ne se fient pas à la parole de celui qui construit : chaque construction laisse un enregistrement que `release-stage` relit.

- **Agent** : `cargo xtask agent --release` refuse (erreur, pas avertissement) un arbre qui n'est pas le commit, AVANT (rien n'est téléchargé ni construit) et APRÈS la construction (un fichier suivi modifié, un fichier non suivi ou ignoré sous les entrées de la construction, un fichier masqué à git, un `HEAD` qui a bougé). Un état que git n'a pas su dire est refusé, jamais lu comme « propre ». L'enregistrement `hearth-agent.build` est obligatoire : s'il ne peut pas être écrit, la construction échoue. L'image est téléchargée avec reprise (cinq essais, attente 5, 10, 20 puis 40 s, SEULEMENT pour une limite de débit ou une panne passagère du registre ; toute autre erreur arrête tout de suite). Sans `--release`, `cargo xtask agent` reste une construction de développement (avertissement seulement).
- **Client** : `cargo xtask client-record --begin` (avant `npm ci`) refuse un arbre qui n'est pas le commit et note ce commit ; `cargo xtask client-record --installer F --version X.Y.Z` (après `tauri build`) écrit `F.build` seulement si l'arbre est encore propre et sur le même commit. Format `BuildRecord` : `commit=`, `tree=`, `sha256=`, `version=`. Le fichier n'est jamais assemblé ni déposé.
- **Assemblage** : `release-stage` exige `Hearth_X.Y.Z_x64-setup.exe.build` dans `--client-dir` ET `hearth-agent.build` à côté du binaire de l'agent, chacun de somme égale au fichier donné, de commit égal à `HEAD`, d'arbre `clean`. L'arbre de travail doit lui-même être le commit (BR-RELEASE-003). Un seul `HEAD` pour les deux : un client et un agent de deux commits différents sont refusés.
- **Racine** : `xtask` lit la racine du dépôt à la compilation. En environnement GitHub (`GITHUB_ACTIONS=true`), `cargo xtask ci-root-check` et la tête de `client-sign`, `client-manifest`, `agent-sign`, `release-stage`, `release-upload` refusent une racine compilée différente de `$GITHUB_WORKSPACE` (les deux chemins sont affichés). Une seule fonction donne la racine (`repo_root`).
- **Signature de l'agent** : `cargo xtask agent-sign --binary F --version X.Y.Z --out S` signe avec la clé de `HEARTH_AGENT_SIGNING_KEY`, mot de passe OBLIGATOIRE dans `HEARTH_AGENT_SIGNING_KEY_PASSWORD` (jamais un argument, jamais dans une trace, une erreur ou un `Debug`), commentaire signé exactement `version:X.Y.Z`, relue contre `crates/hearth-agent/update-key.pub`.

Les enregistrements sont des fichiers en clair, falsifiables à la main : une garde contre l'erreur (binaire ou installateur d'un autre commit glissé dans un dossier), pas contre la malveillance (la barrière contre elle est la relecture du journal des constructions avant l'approbation, ADR-0056 proposée).

## Application (code)
- `xtask/src/agent.rs::{build_release, refuse_unless_clean, pull_image, is_transient_pull_error, write_build_record, recorded_state}` : construction de publication de l'agent, arbre propre exigé, reprise du téléchargement, enregistrement obligatoire.
- `xtask/src/client_record.rs::{begin, record}` : relevé de l'arbre avant, enregistrement `F.build` après la construction du client.
- `xtask/src/release_stage.rs::{check_client_build, check_agent_build, stage_with}` : somme, commit et arbre vérifiés avant la première écriture ; l'arbre de travail est le commit (`check_clean_tree`).
- `xtask/src/agent_sign.rs::{sign_agent, SigningInputs}` : signature de l'agent, mot de passe obligatoire et masqué (`xtask/src/secret.rs::Secret`).
- `xtask/src/repo_root.rs::{repo_root, check_workspace, guard}` : racine unique et garde de l'espace de travail ; `xtask/src/main.rs` l'appelle en tête des tâches lancées depuis le binaire transmis.

## Vérification
- `xtask/src/agent.rs` : `a_release_build_on_a_dirty_tree_is_refused_before_anything_is_pulled_or_built`, `an_unknown_tree_state_is_refused_never_taken_for_clean`, `a_release_build_on_a_clean_tree_pulls_builds_and_records_the_commit`, `a_tree_dirtied_or_a_head_moved_during_the_build_is_refused_and_leaves_no_record`, `a_record_that_cannot_be_written_fails_the_release_build`, `five_failed_attempts_give_up_after_waits_of_5_10_20_and_40_seconds`, `a_rate_limit_is_retried_with_a_growing_wait_until_it_passes`, `any_other_error_stops_at_once_without_waiting`.
- `xtask/src/client_record.rs` : `a_clean_tree_before_and_after_writes_the_record_with_the_sum_and_the_commit`, `begin_refuses_a_dirty_or_unknown_tree_and_writes_nothing`, `a_tree_dirtied_by_the_build_is_refused_and_no_record_is_left`, `a_head_that_moved_during_the_build_is_refused`, `without_the_begin_survey_nothing_is_recorded`.
- `xtask/src/release_stage.rs` : `an_installer_needs_a_matching_build_record`, `the_stage_refuses_an_installer_without_a_matching_build_before_writing`, `an_agent_binary_needs_a_matching_build_record`, `the_stage_refuses_an_agent_without_a_matching_build_before_writing`.
- `xtask/src/agent_sign.rs` : `the_signature_carries_exactly_the_version_and_verifies_against_the_embedded_key`, `a_key_that_is_not_the_pair_of_the_embedded_key_is_refused`, `a_missing_key_a_missing_password_or_a_wrong_one_is_refused_without_echoing_secrets`, `the_secrets_are_never_shown_by_debug`.
- `xtask/src/repo_root.rs` : `the_same_workspace_passes`, `another_workspace_is_refused_with_both_paths`, `a_missing_workspace_or_an_unresolvable_path_is_a_refusal_never_an_agreement`.
- Tout est testé par faux (`Host`, `TreeFake`, `DockerFake`) : jamais de Docker, de réseau ni de clé réelle. Commande : `cargo test -p xtask -j 2 -- --test-threads=2`.

## Cas limites
- Une construction qui échoue ne laisse pas l'enregistrement d'une précédente (supprimé en tête de `agent`, de `client-record --installer`).
- Hors GitHub (procédure manuelle de secours), la garde de la racine ne fait rien.
- `cargo xtask agent` sans `--release` n'est pas une construction de publication : `release-stage` refusera quand même un binaire dont l'enregistrement n'est pas `clean`.
- Limites héritées de BR-RELEASE-003 : l'image du conteneur est figée par empreinte, mais les paquets `apk` et le cache de Cargo ne sont pas vus par la garde.

## Règles liées
- BR-RELEASE-003 (la version assemblée est revérifiée, l'arbre est le commit), BR-RELEASE-006 (numéros concordants), BR-RELEASE-008, BR-UPDATE-035 (la version signée de l'agent).

## Historique
- 2026-10-16 : création (HRT-173) : `agent --release`, `client-record`, `check_client_build`, `agent-sign`, `ci-root-check`. Reste à écrire (tranches suivantes) : l'étiquette posée sur ce commit par `release-github`, le flux `publish.yml`.
