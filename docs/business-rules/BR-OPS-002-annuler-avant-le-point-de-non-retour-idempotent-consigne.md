---
id: BR-OPS-002
domaine: OPS
titre: Une opération s'annule avant son point de non-retour seulement ; l'annulation est idempotente et consignée avec son auteur
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 0 bis et 7), contexts/hearth/tickets/hrt/HRT-144.md
maj: 2026-10-14
---

# BR-OPS-002 — Une opération s'annule avant son point de non-retour seulement ; l'annulation est idempotente et consignée avec son auteur

## Règle
`DELETE /operations/services/{id}` (acte `OperationCancel`, `0x10`, couvert par l'élévation : défaut non validé par le propriétaire, seule l'annulation est couverte par une saisie récente du mot de passe (élévation de 5 minutes)) annule une opération `accepted`, `running` ou `awaiting_decision` tant que l'étape du point de non-retour de son genre n'a aucune ligne au journal. Après : `409 TOO_LATE_TO_CANCEL`, rien ne change. Une opération en cours passe en `cancelling` (son exécuteur retire ce qu'elle a créé et la clôt) ; une opération qui n'a rien commencé ou qui attend une décision passe en `cancelled`, son verrou est rendu et ce qu'elle avait créé pour elle-même est retiré (après le changement de statut, revérifié par inode). L'annulation est IDEMPOTENTE : annuler une opération déjà annulée par un compte rend `200` avec la même issue, sans nouvelle entrée de journal. Une opération dans un autre statut (terminée, à vérifier, en retour arrière) : `409 CONFLICT` (`details.reason = not_cancellable`). Le changement de statut et l'entrée du journal d'activité s'écrivent dans la MÊME transaction, sous l'action du GENRE de l'opération (par exemple `service.deploy`), avec le compte ; la condition de statut et le point de non-retour sont jugés dans cette transaction, et l'écriture d'un `intent` destructif par l'exécuteur vérifie le statut dans sa propre instruction : il n'y a aucune fenêtre entre « annulable » et « annulée ». Un refus ou un échec de la route se consigne sous `operation.cancel`.

Un identifiant d'opération qui n'est pas celui d'une opération visible, un compte sans le droit, une preuve qui ne colle pas (autre acte, autre opération, autre compte, autre session, rejouée), un mot de passe absent ou faux, une élévation : mêmes refus que tout acte d'administration, prouvés par la MATRICE de `admin_reauth.rs` (annuler y figure comme les autres actes, revue de la PR 94, B1).

## Application (code)
- `crates/hearth-agent/src/domain/ops/decision.rs::cancel_verdict`
- `crates/hearth-agent/src/application/service_op_access.rs::ServiceOpAccess::cancel`
- `crates/hearth-agent/src/infrastructure/sqlite/ops_repo.rs` (`append_step` avec `StepGuard`, `ServiceOpTx for SqliteUnitOfWork`)
- `crates/hearth-agent/src/entrypoint/http/service_ops.rs::cancel`

## Vérification
- `crates/hearth-agent/src/domain/ops/decision.rs::tests`
- `crates/hearth-agent/tests/service_ops_https.rs::a_running_operation_is_cancelled_by_an_account_with_the_right_and_the_journal_keeps_the_author`
- `crates/hearth-agent/tests/service_ops_https.rs::cancelling_twice_answers_the_same_and_writes_one_journal_entry`
- `crates/hearth-agent/tests/service_ops_https.rs::an_accepted_operation_is_cancelled_at_once_and_frees_its_service`
- `crates/hearth-agent/tests/service_ops_https.rs::after_the_point_of_no_return_the_answer_is_too_late_and_nothing_changes`
- `crates/hearth-agent/tests/service_ops_https.rs::an_intent_after_a_cancellation_is_refused_by_the_write_itself`
- `crates/hearth-agent/tests/service_ops_https.rs::cancelling_needs_the_right_and_the_confirmation_of_the_act`
- `crates/hearth-agent/tests/service_ops_https.rs::the_proof_of_a_cancellation_is_bound_to_the_operation_it_names`
- `crates/hearth-agent/tests/admin_reauth.rs` (matrice : `OperationCancel` dans `generic_kinds()` : réussite et une entrée de journal, preuve d'un autre acte / autre opération / autre compte / autre session / rejouée, sans preuve, sans mot de passe, mot de passe faux et attente au cinquième, client ancien `426`), `::every_answer_to_a_decision_asks_for_the_password_during_the_elevation_and_only_cancelling_is_covered` (annuler passe sous l'élévation), `::an_account_without_the_services_right_cannot_cancel_or_decide_even_with_a_valid_confirmation`

## Cas limites
- La décision `cancel` suit la même règle : au-delà du point de non-retour, `409 TOO_LATE_TO_CANCEL` (comme `DELETE`), et un exécuteur ne peut pas offrir « annuler » à ce moment (`ask_decision` refuse). Tests : `service_ops_https.rs::cancelling_by_a_decision_after_the_point_of_no_return_is_too_late_like_delete`, `service_ops_journal.rs::cancel_is_never_offered_as_a_choice_after_the_point_of_no_return`.
- Aucun exécuteur de geste n'existe encore (HRT-64, HRT-68) : une opération `cancelling` n'est close que par son exécuteur ou, après un redémarrage de l'agent, par la reprise (BR-EDIT-031). Les objets d'une opération `cancelling` sont retirés par cet exécuteur ou par la reprise, pas par la route.
- Si l'agent s'arrête entre le changement de statut et le retrait des objets créés, ceux-ci restent en place (aucune opération n'est plus là pour les nommer) : limite connue, ADR-0045.

## Règles liées
- BR-OPS-001, BR-LIFE-013, BR-EDIT-031, BR-JEU-006

## Historique
- 2026-10-14 — création (HRT-144 : le socle des opérations branché sur des routes et sur le flux).
- 2026-10-14 — revue de la PR 94 : annuler rétabli dans la matrice de confirmation ; classement d'élévation marqué « défaut non validé » ; décider `cancel` refusé hors du point de non-retour.
