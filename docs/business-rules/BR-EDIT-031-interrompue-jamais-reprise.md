---
id: BR-EDIT-031
domaine: EDIT
titre: Une opération interrompue par un redémarrage de l'agent n'est jamais reprise, rejouée ni défaite seule
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-modifier-un-service.md (BR-EDIT-031), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 0, 0 bis et 6), HRT-83
maj: 2026-10-13
---

# BR-EDIT-031 — Une opération interrompue par un redémarrage de l'agent n'est jamais reprise, rejouée ni défaite seule

## Règle
Au démarrage de l'agent, chaque opération non terminée est CLASSÉE, jamais continuée. Aucune étape destructive n'a de ligne : « interrompue », rien n'a eu lieu. Une étape destructive a un `intent` sans `done`, ou a échoué : « à vérifier », le verrou du service est repris et les décisions possibles sont offertes au compte. Tout est `done` et un seul état complet existe (ancien ou nouveau) : « interrompue », avec le côté où tourne le service. Tout est `done` mais deux états coexistent, ou le moteur n'a pas pu être lu : « à vérifier ». Le journal d'une opération classée n'est pas modifié : aucun geste n'est rejoué. Les seuls objets retirés sont ceux que l'opération avait créés pour elle-même et nommés au journal (BR-BACKUP-025, BR-MONDE-026).

La reprise n'empêche jamais l'agent de démarrer : un journal d'étapes qui ne se relit pas classe l'opération « à vérifier » (cause « état inconnu »), verrou repris, rien retiré ni rejoué ; une ligne d'opération illisible est isolée et son service reste refusé par l'index de la base. Choisir « revenir » sur une opération à vérifier démarre un retour arrière (`rolling_back`), jamais un « revenu en arrière » sans geste.

Cette tranche ne branche pas encore d'opération réelle (modifier, supprimer, sauvegarder) : elle fournit le classement, le journal et ses tests ; l'observateur du moteur de ce ticket répond « illisible », donc « à vérifier » plutôt que « tout va bien ».

## Application (code)
- `crates/hearth-agent/src/domain/ops/recover.rs::{recover, recover_unreadable_journal}`
- `crates/hearth-agent/src/application/service_ops.rs::ServiceOps::recover_at_startup`
- `crates/hearth-agent/src/infrastructure/ops_observer.rs::UnobservedState`

## Vérification
- `crates/hearth-agent/src/domain/ops/recover.rs::tests::the_table_of_the_design`
- `crates/hearth-agent/tests/service_ops_journal.rs::an_intent_without_done_needs_attention_keeps_the_lock_and_asks_the_account`
- `crates/hearth-agent/tests/service_ops_journal.rs::a_destructive_gesture_is_never_replayed_by_the_startup`
- `crates/hearth-agent/tests/service_ops_journal.rs::without_an_intent_on_a_destructive_step_the_operation_is_interrupted_and_the_service_is_free`
- `crates/hearth-agent/tests/service_ops_journal.rs::an_operation_to_verify_stays_so_across_restarts_and_waits_for_the_account`
- `crates/hearth-agent/tests/service_ops_journal.rs::an_unreadable_step_column_makes_the_operation_to_verify_and_the_agent_still_starts`
- `crates/hearth-agent/tests/service_ops_journal.rs::an_unreadable_operation_column_isolates_the_row_and_the_agent_still_starts`
- `crates/hearth-agent/tests/service_ops_journal.rs::the_composition_root_starts_even_when_the_whole_journal_is_unreadable`
- `crates/hearth-agent/tests/service_ops_journal.rs::a_name_and_a_heavy_seat_survive_a_restart_and_reverting_only_starts_the_rollback`

## Cas limites
- Un second redémarrage ne reclasse pas une opération déjà « à vérifier » : il reprend seulement son verrou, sans reconsulter le moteur.
- Une ligne du journal qui nomme une étape absente du plan, ou un objet hors de sa racine, ne fait rien retirer.
- Les textes affichés (« Une modification de ce service a été interrompue… ») et les boutons viennent avec le ticket de la modification (HRT-87, HRT-88).

## Règles liées
- BR-LIFE-013, BR-DEL-025, BR-BACKUP-025, BR-JEU-011, BR-MONDE-026, BR-RESIL-009

## Historique
- 2026-10-13 — création (HRT-83, ADR-0044 et ADR-0045 : socle des services créés par Hearth, côté agent).
