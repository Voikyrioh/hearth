---
id: BR-UPDATE-032
domaine: UPDATE
titre: Un superviseur rejoué ne refait aucune étape déjà faite
statut: active
invariant: true
source: HRT-27, ADR-0014 (amendement du 2026-10-07)
maj: 2026-10-07
---

# BR-UPDATE-032 : Un superviseur rejoué ne refait aucune étape déjà faite

## Règle
Après chaque étape durable, le superviseur écrit `update/phase.json` : fichier voisin, `fsync` du fichier, renommage, `fsync` du dossier (jamais lu à moitié écrit). Étapes : `started`, `stopped`, `database_saved`, `swapping`, `swapped`, `checking`, `rolling_back`, `rollback_stopped`, `rollback_database`, `rollback_binary`, `concluded`, `abandoned`. Une exécution relancée lit ce marqueur et reprend à son étape ; chaque étape se retrouve dans les faits du disque quand son marqueur n'a pas été écrit :
- **échange des binaires** : fait si le binaire en place a exactement le contenu du binaire déposé (`swap_done`) ; jamais refait (refaire l'échange remplacerait la sauvegarde de l'ancien binaire par le nouveau) ;
- **retour arrière, base d'abord, binaire ensuite** (BR-UPDATE-029 ; la décision est la fonction pure `rollback_step`, le superviseur l'applique) : la base d'avant se remet tant que l'ancien binaire n'est pas revenu, et se remet une **seconde fois** si le retour arrière a été interrompu après l'arrêt du service (la machine a pu redémarrer, et le nouveau binaire migrer de nouveau la base remise) ; dès que l'ancien binaire est revenu (drapeau `backup_kept` tombé et sauvegarde absente, `binary_restored`), la base ne se remet **jamais plus** (l'ancien agent a pu écrire dedans) ;
- **résultat** : décidé (`concluded`, avec l'instant) avant d'être écrit ; une reprise le réécrit à l'identique et ne réécrit pas un résultat déjà écrit (déjà annoncé par l'agent) ; le nettoyage retire le travail avant le marqueur (une relance sans travail ne fait rien, une relance avec travail et sans marqueur recommencerait).

**Issues sûres** : un marqueur **illisible** (à moitié écrit par autre chose que nous, abîmé, édité), ou d'**une autre version** que le travail, ne se devine pas : le superviseur ne touche **ni binaire, ni base, ni service**, écrit `failed` / `rollback_failed` (ou `interrupted` sans sauvegarde de l'ancien binaire), journalise une fois et abandonne (BR-UPDATE-033). Une copie de la base ou une sauvegarde de l'ancien binaire **perdue** (alors que le marqueur dit qu'il y en avait une), ou une remise qui échoue (disque plein, copie illisible) : abandon, copies gardées ; la base vivante n'est jamais touchée par une remise qui échoue (copie voisine puis renommage) ; un ancien binaire n'est jamais remis devant une base qui n'a pas pu l'être. Le service est alors relancé tel quel (nouveau binaire sur sa base, ou l'ancien sur la sienne : un couple cohérent, puisque la base se remet avant le binaire).

## Application (code)
- `crates/hearth-agent/src/domain/update/resume.rs` (`Marker`, `Phase`, `enter`, `swap_done`, `database_copy`, `binary_backup`, `binary_restored`, `rollback_step`, `RollbackFacts`).
- `crates/hearth-agent/src/application/update_supervisor.rs::Supervisor::{run, drive, conclude, finish}`.
- `crates/hearth-agent/src/infrastructure/update/host.rs::FsUpdateHost::{read_marker, write_marker, discard_marker, same_content, clear_staging}`.

## Vérification
- `domain::update::resume::tests` (entrée, reprise, bornes, issues sûres ; `the_database_comes_back_before_the_binary`, `without_a_copy_the_database_is_left_alone`, `once_the_old_binary_is_back_the_database_is_never_put_back_again`, `a_lost_copy_puts_back_nothing` : l'ordre du retour arrière).
- `tests/update_supervisor_resume.rs` : superviseur tué à chaque point (avant et après chaque écriture), réussite (`…_of_a_successful_update_…`) et retour arrière (`…_of_a_rollback_rolls_back_exactly_once`), `a_result_already_announced_by_the_agent_is_not_written_again_by_a_resumed_supervisor`, `a_corrupted_marker_guesses_nothing_and_touches_nothing`, `a_leftover_half_written_temporary_marker_is_ignored`, `a_marker_of_another_version_is_not_this_work_and_touches_nothing`, `a_full_disk_at_the_database_restore_…`, `a_database_copy_that_is_gone_…`, `a_database_copy_that_is_not_a_file_…`, `a_lost_binary_backup_…`.

## Cas limites
- Ce qui est perdu : le contrôle de 60 s repart de zéro à chaque reprise (borné par BR-UPDATE-033).
- L'activation du mode attaque faite pendant la fenêtre de contrôle est perdue par un retour arrière (BR-UPDATE-029, ADR-0025) ; ADR-0014 (2026-10-07) le redit.

## Règles liées
- BR-UPDATE-015, BR-UPDATE-028, BR-UPDATE-029, BR-UPDATE-030, BR-UPDATE-033, BR-UPDATE-034, ADR-0014

## Historique
- 2026-10-07 : création (HRT-27).
