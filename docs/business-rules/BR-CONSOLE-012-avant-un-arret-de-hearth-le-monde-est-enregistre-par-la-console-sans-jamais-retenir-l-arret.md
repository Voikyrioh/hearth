---
id: BR-CONSOLE-012
domaine: CONSOLE
titre: Avant un arrêt demandé par Hearth, le monde est enregistré par la console, sans jamais retenir l'arrêt
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-11-17-fonctionnelle-console-et-joueurs-minecraft.md
maj: 2026-11-24
---

# BR-CONSOLE-012 : Enregistrer le monde par la console avant l'arrêt

## Règle
Quand la fiche déclare `[remote_console.use.before_stop]` (commande, réponse attendue, délai de 5 à 120 s) et que le jeu tourne et est prêt, tout arrêt que Hearth demande (Arrêter, Redémarrer, sauvegarde « arrêter, sauvegarder, relancer », lot de mods, modification par recréation) commence par l'étape `console_save` : l'agent envoie la commande de la fiche (Minecraft : `save-all flush`) et attend la réponse attendue (« Saved the game ») au plus le délai de la fiche, puis l'arrêt se déroule comme avant, sans changement.

- Jamais bloquante : jeu pas prêt, console injoignable, mot de passe refusé, jeu muet, réponse inattendue ou délai dépassé = « non confirmé ». L'arrêt continue, aucune décision n'est demandée, aucune erreur n'est montrée ; seule une note fixe dit que le monde n'a pas pu être enregistré avant l'arrêt (voir ci-dessous).
- Journal de l'opération : l'étape `console_save` est terminée (`done`) quand le signe est vu (« confirmé »), en échec (`failed`) sinon (« non confirmé »). Jamais le texte de la commande ni de la réponse.
- Jamais rejouée : l'étape n'est pas destructive, `recover` ne la rejoue pas (opération coupée = `interrupted` comme avant).
- Hors débit (20 commandes par minute) et hors journal d'activité de la console : c'est l'opération qui tient déjà le verrou du service.
- Une fiche sans `before_stop`, un service qui ne tourne pas, un service adopté : aucune étape, aucun changement de comportement.
- L'annulation pendant l'enregistrement attend la fin de l'étape : au plus le délai de la fiche + 5 s (l'échange et la préparation tiennent sous UN seul délai).
- Le client ne dit pas « si la fiche le prévoit » : la fiche le dit exactement (`StopView.console_save`, la même fonction que celle qui ajoute l'étape). Quand l'étape est `failed`, le suivi dit : « Le monde n'a pas pu être enregistré avant l'arrêt. L'arrêt a continué. »
- Exclusions voulues : le retour de mods (`stop_again_for_mods` : on n'enregistre pas un monde qui a tourné avec de mauvais mods) et la suppression d'un service (`delete.rs`). Un arrêt qui ne vient pas de Hearth (moteur, serveur) n'est pas couvert.

## Application (code)
- `crates/hearth-agent/src/domain/catalog/stop_plan.rs::saves_by_console_before_stop` (fiche déclarée ET protocole parlé ET port résolu : la vue et `ConsoleService::declares` l'utilisent)
- `crates/hearth-agent/src/domain/ops/plan.rs::{CONSOLE_SAVE, OpPlan::with_console_save}`
- `crates/hearth-agent/src/application/service_ops.rs::{ServiceOps::plan_before_stop, ServiceOps::console_save}`
- `crates/hearth-agent/src/application/console.rs::{impl BeforeStopSave for ConsoleService}` (port `ports/before_stop_save.rs`)
- Points d'arrêt : `service_lifecycle.rs::{request, execute}`, `backups.rs::{run}` (+ `accept`), `mods_apply.rs::run_apply`, `service_update.rs` (étape `stop_old`)

## Vérification
- `domain/ops/plan.rs::tests::the_console_save_step_is_first_never_destructive_and_added_once`
- `domain/ops/recover.rs::tests::a_cut_console_save_is_interrupted_and_never_replayed`
- `domain/ops/recover.rs::tests::a_failed_console_save_changes_no_classification_of_a_mods_or_backup_operation`
- `domain/catalog/tests_console.rs::hearth_saves_by_the_console_only_when_the_fiche_says_so_with_a_protocol_and_a_port_it_knows`
- `tests/console_https.rs::{the_save_line_leaves_before_the_stop_order_and_the_step_is_done_before_the_stop_begins, restarting_saves_the_world_first_like_stopping, a_game_still_starting_is_not_asked_to_save_and_the_stop_goes_on, the_save_ignores_the_console_rate_limit_and_never_enters_the_activity_journal, a_game_that_never_answers_holds_the_stop_back_at_most_the_delay_plus_five_seconds}`
- `tests/service_backups_https.rs::{a_running_minecraft_saves_by_the_console_before_the_stop_of_a_cold_copy, a_mute_game_never_prevents_the_cold_copy_from_succeeding, a_hot_copy_never_asks_the_game_to_save}`
- `tests/mods_apply_https.rs::{a_ready_game_saves_by_the_console_before_the_stop_and_the_step_is_done, a_mute_game_goes_all_the_way_through_the_lot, a_game_still_starting_is_not_asked_to_save}`
- `tests/service_update_https.rs::{recreating_a_running_game_saves_by_the_console_before_the_old_one_is_stopped, recreating_a_stopped_game_has_no_console_save_step}`
- `tests/console_https.rs::{a_game_that_answers_saved_the_game_is_confirmed_then_stopped_as_before, a_mute_game_or_an_unreachable_console_never_holds_the_stop_back, a_reply_without_the_expected_sign_is_not_confirmed_and_the_stop_goes_on, a_stopped_server_gets_no_save_and_its_operation_has_no_such_step}`

## Règles liées
- BR-CONSOLE-006, BR-CONSOLE-008, BR-JEU-032, BR-JEU-002

## Historique
- 2026-11-24 : création (HRT-292, ADR-0067 point 10).
- 2026-11-24 : revue Stephen : un seul délai (préparation + échange), drapeau `console_save` exact dans la vue, note de non-confirmation, exclusions dites.
