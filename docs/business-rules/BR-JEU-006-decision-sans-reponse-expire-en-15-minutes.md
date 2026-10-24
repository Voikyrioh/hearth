---
id: BR-JEU-006
domaine: JEU
titre: Une décision laissée sans réponse expire au bout de 15 minutes vers « annulé » ; le verrou est rendu
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-modifier-un-service.md (BR-JEU-006), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 0, 0 bis et 6), HRT-83
maj: 2026-10-24
---

# BR-JEU-006 — Une décision laissée sans réponse expire au bout de 15 minutes vers « annulé » ; le verrou est rendu

## Règle
Une opération `awaiting_decision` porte les choix offerts et la date de la demande. Le balayage périodique (toutes les 30 s) la passe à `cancelled` (cause `decision_expired`) quand 15 minutes se sont écoulées (`decision_expired`), efface les choix et rend le verrou du service. Une décision prise avant l'échéance fait reprendre l'opération. L'agent ne renvoie jamais rien tout seul.

## Application (code)
- `crates/hearth-agent/src/domain/ops/decision.rs::{decision_expired, decision_expiry, apply_choice}`
- `crates/hearth-agent/src/application/service_ops.rs::ServiceOps::{ask_decision, decide, sweep}`
- `crates/hearth-agent/src/entrypoint/tasks.rs::spawn_service_ops_sweep`
- `crates/hearth-agent/src/application/game_stop.rs::GameStop::ask` (la décision d'un arrêt de jeu : « Arrêter quand même » = `continue`, « Annuler » = `cancel`, HRT-103)

## Vérification
- `crates/hearth-agent/src/domain/ops/decision.rs::tests::a_decision_expires_after_exactly_fifteen_minutes`
- `crates/hearth-agent/tests/service_ops_journal.rs::a_pending_decision_expires_after_fifteen_minutes_towards_cancelled`
- `crates/hearth-agent/tests/service_ops_journal.rs::a_decision_before_the_expiry_resumes_or_cancels_the_operation`
- `crates/hearth-agent/tests/game_stop_https.rs::a_sign_that_does_not_come_stops_nothing_and_asks_which_way_to_go`
- `crates/hearth-agent/tests/game_stop_https.rs::cancelling_the_decision_leaves_the_service_running_releases_the_lock_and_never_resends`
- `crates/hearth-agent/tests/game_stop_https.rs::a_decision_left_unanswered_expires_in_15_minutes_the_service_keeps_running`

## Cas limites
- L'agent d'un arrêt de jeu attend la réponse sans rien arrêter ; sans réponse, le balayage périodique (la seule expiration, alignée sur l'échéance de la décision) clôt l'opération, et l'attente de l'arrêt le voit comme toute clôture. Les textes et les boutons de la décision sont côté client (HRT-104).
- Après « Arrêter quand même » sur un signe obligatoire non vu, l'ordre de sortie de la fiche est tout de même envoyé (le canal fonctionne) : BR-JEU-034.
- `OperationCancel` après le point de non-retour répond `TOO_LATE_TO_CANCEL` (`cancel_verdict`) ; l'acte lui-même et sa route ne sont pas dans ce ticket.

## Règles liées
- BR-EDIT-031, BR-LIFE-013

## Historique
- 2026-10-24 — l'arrêt d'un serveur de jeu demande cette décision (HRT-103).
- 2026-10-14 — la route de décision refuse une décision expirée avant même le balayage, et le balayage retire ce que l'opération avait créé pour elle-même (HRT-144, BR-OPS-003).
- 2026-10-13 — création (HRT-83, ADR-0044 et ADR-0045 : socle des services créés par Hearth, côté agent).
