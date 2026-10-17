---
id: BR-RELEASE-009
domaine: RELEASE
titre: Une publication interrompue se reprend en relançant l'étape en échec de la même exécution
statut: active
invariant: true
source: HRT-174, contexts/hearth/conceptions/2026-10-16-fonctionnelle-publier-une-version.md (BR-RELEASE-009), 2026-10-16-technique-publier-une-version.md (7.2, 9.5)
maj: 2026-10-17
---

# BR-RELEASE-009 : Reprise par relance de la même exécution

## Règle
La signature, l'assemblage et le dépôt peuvent être relancés après un échec sans risque de corruption. La reprise se fait en relançant l'étape échouée de la MÊME exécution, qui réutilise les fichiers déjà construits et signés (artefact `hearth-release-signed`, gardé 14 jours). Une NOUVELLE exécution reconstruit des fichiers différents (les signatures changent) et se heurte à BR-RELEASE-001 : arrêt sans rien écraser.

`cargo xtask release-upload --confirm` :
- lance d'abord `aws --version` : la majeure doit être 2, sinon « L'outil aws doit être l'AWS CLI en version 2 : « aws --version » a répondu « … ». Rien n'a été écrit. » ;
- fichier déjà déposé et identique : reconnu, passé, jamais réécrit ;
- fichier déjà déposé avec un autre contenu : arrêt AVANT toute écriture, première ligne exacte « {fichier} est déjà déposé avec un autre contenu : rien n'est écrasé. Si une exécution précédente a échoué, relancez l'étape en échec de cette exécution-là. » (plusieurs fichiers : « … sont déjà déposés avec un autre contenu : … ») ; le détail (BR-RELEASE-001, numéro brûlé) suit sur les lignes d'après ;
- dépôt coupé avant le manifeste : première ligne exacte « Dépôt interrompu après {n} fichiers sur {total}. Le manifeste n'a pas été déposé : les utilisateurs voient toujours X.Y.W. Relancez l'étape en échec. » ; sans manifeste en place : « … les utilisateurs ne voient encore aucune version. … ». `n` compte les fichiers écrits OU reconnus identiques (script compris), `total` tous les fichiers hors manifeste, `X.Y.W` la plus récente des versions du manifeste en place (client, agent).

`cargo xtask client-manifest` écrit les notes (`HEARTH_NOTES`, contrôlées comme au lancement : 1 à 2000 caractères) dans `notes.txt` à côté de `--out` et prend la date UTC du moment (RFC 3339, à la seconde) quand `--notes-file` et `--date` sont absents. `client-sign` et `agent-sign` signent avec la clé de l'environnement, relisent la signature contre la clé publique du dépôt et refusent une clé qui n'en est pas la paire.

En mode GitHub (`GITHUB_ACTIONS=true`), la première ligne du refus devient l'annotation `::error` et la ligne du résumé ; le détail reste au journal.

## Application (code)
- `xtask/src/release_publish.rs::{refuse_different, interrupted, deposit_before_manifest, check_aws_version, publish}`.
- `xtask/src/client_release.rs::{manifest_text, run_manifest, run_sign}`, `xtask/src/agent_sign.rs`.
- `xtask/src/main.rs` : annonce de la première ligne en mode GitHub.

## Vérification
- `xtask/src/release_publish.rs` : `a_key_already_there_with_another_content_is_refused_before_any_write`, `several_different_files_are_named_in_one_refusal`, `a_write_failing_in_the_middle_leaves_a_clean_state_and_the_rerun_resumes`, `an_interrupted_deposit_says_what_users_still_see`, `identical_files_count_as_done_in_the_interrupted_message`, `a_key_already_there_with_the_same_content_is_skipped`, `only_the_major_version_two_of_aws_is_accepted`.
- `xtask/src/client_release.rs` : `the_notes_come_from_the_environment_and_land_in_notes_txt_next_to_the_manifest`, `an_explicit_date_wins_and_empty_or_too_long_notes_are_refused`, `the_current_utc_date_is_a_whole_second_rfc3339_ending_in_z`, `a_wrong_version_file_key_or_password_is_refused`.

## Cas limites
- L'artefact signé n'existe qu'en publication réelle (BR-RELEASE-011) et expire après 14 jours : au-delà, une relance signe de nouveau et se heurte à BR-RELEASE-001 si des fichiers sont déjà déposés (numéro brûlé, version suivante).
- Une panne du manifeste lui-même (écriture ou relecture) n'est pas « avant le manifeste » : son erreur est rendue telle quelle.

## Règles liées
- BR-RELEASE-001, BR-RELEASE-002, BR-RELEASE-010, BR-RELEASE-011

## Historique
- 2026-10-17 : création (HRT-174).
