---
id: BR-OPS-003
domaine: OPS
titre: Une décision d'opération n'est acceptée que parmi les choix offerts, avant son expiration, par un acte confirmé qui signe le choix
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 0 bis et 7), contexts/hearth/tickets/hrt/HRT-144.md
maj: 2026-10-14
---

# BR-OPS-003 — Une décision d'opération n'est acceptée que parmi les choix offerts, avant son expiration, par un acte confirmé qui signe le choix

## Règle
`POST /operations/services/{id}/decision` (acte `OperationDecide`, `0x1D`, couvert par l'élévation) applique le choix du compte à une opération `awaiting_decision` ou `needs_attention`. Le CHOIX est signé avec l'identifiant de l'opération : une preuve faite pour un autre choix ou une autre opération est refusée. Le choix doit être parmi ceux que l'opération offre (`decision.choices`) ; sinon `409 DECISION_NOT_OFFERED` avec `details.reason` (`not_waiting`, `not_offered`, `not_applicable`, `expired`). Un choix inconnu de l'agent est lu `unknown` et jamais offert. Une décision demandée depuis 15 minutes ou plus est expirée : refusée (`expired`) que le balayage soit passé ou non, puis annulée par le balayage (cause `decision_expired`) ; une opération à vérifier n'expire jamais (son `decision.expires_at` est absent). Le changement de statut et l'entrée du journal d'activité (action du genre, cible « décision « {libellé} » ») s'écrivent dans la même transaction.

## Application (code)
- `crates/hearth-agent/src/domain/ops/decision.rs::{apply_choice, decision_expired}`
- `crates/hearth-agent/src/application/service_op_access.rs::ServiceOpAccess::decide`
- `crates/hearth-agent/src/entrypoint/http/service_ops.rs::decide`
- `crates/hearth-proto/src/admin_act.rs::AdminAct::OperationDecide`

## Vérification
- `crates/hearth-agent/src/domain/ops/decision.rs::tests`
- `crates/hearth-agent/tests/service_ops_https.rs::a_decision_among_the_offered_ones_is_applied_and_journaled`
- `crates/hearth-agent/tests/service_ops_https.rs::a_decision_that_is_not_offered_is_refused_whatever_it_is`
- `crates/hearth-agent/tests/service_ops_https.rs::an_expired_decision_is_refused_and_the_sweep_cancels_it_with_its_cause`
- `crates/hearth-agent/tests/service_ops_https.rs::deciding_to_cancel_ends_the_operation_and_an_operation_to_verify_never_expires`
- `crates/hearth-proto/src/admin_act.rs::tests::a_decision_signs_its_choice_so_that_another_choice_is_another_act`

## Cas limites
- « Revenir » sur une opération à vérifier démarre un retour arrière (`rolling_back`) : sans exécuteur de retour arrière (lots suivants), l'opération y reste jusqu'au prochain redémarrage de l'agent, qui la classe (BR-EDIT-031).
- Qui peut décider : le droit « gérer les services » (niveau de la route) ; l'opération n'est pas réservée au compte qui l'a lancée.

## Règles liées
- BR-JEU-006, BR-OPS-001, BR-EDIT-031

## Historique
- 2026-10-14 — création (HRT-144 : le socle des opérations branché sur des routes et sur le flux).
