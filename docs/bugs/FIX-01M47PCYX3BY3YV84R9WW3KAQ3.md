---
id: FIX-01M47PCYX3BY3YV84R9WW3KAQ3
titre: L'avis de fin de session, arrivé avant la réponse, transformait le résultat d'une action en « résultat inconnu »
date_découverte: 2026-10-06
date_correction: 2026-10-06
---

# FIX-01M47PCYX3BY3YV84R9WW3KAQ3 : L'avis de fin de session devançait la réponse de l'action qui l'avait provoquée

## Symptôme
Supprimer son propre compte (ou fermer ses propres sessions, ou définir son mot de passe par la route d'administration) rendait parfois « résultat inconnu » : l'utilisateur apprenait qu'on ne savait pas si son compte était supprimé, alors qu'il l'était, et il ne pouvait plus le vérifier (session révoquée). Mesuré sous charge CPU : 17 échecs sur 25 exécutions du test `deleting_your_own_account…`.

## Cause root
L'agent révoque la session pendant l'action ; l'avis de révocation arrive par le flux et peut croiser la réponse HTTP. S'il gagne, la machine à états passe à « Accès révoqué » et son effet `MarkPendingUnknown` rendait « résultat inconnu » à l'appelant (`task.rs`, `mark_pending_unknown`) alors que la requête était partie et que sa réponse allait arriver. Cause établie par un test déterministe (`tracking.rs`) : avis livré avant la réponse, retenue à une porte ; sans correction l'appelant est libéré par l'avis.

## Impacté
Depuis HRT-12 (BR-RESIL-009, 010) : toute action qui termine sa propre session.

## Workaround
Aucun.

## Correction
Quand c'est l'AGENT qui met fin à la session (expirée ou révoquée, hors déconnexion voulue), le lien n'est pas tombé : les requêtes en vol ne sont plus marquées « inconnu » par l'avis. Une réponse qui arrive garde son résultat ; sans réponse, la requête s'arrête à `request_timeout` (déjà borné par la tâche) et l'issue est « inconnu ». Rien n'est rejoué. Alternatives écartées : allonger un délai, accepter « inconnu » dans le test.

## Références
- Ticket : HRT-13 (review PR #19, round 2)
- BR : BR-RESIL-009, BR-RESIL-010
- Tests : `crates/hearth-link/tests/tracking.rs::a_session_end_notice_before_the_response_never_turns_a_delivered_result_into_unknown`, `::a_response_that_never_comes_after_the_session_ended_stays_unknown_within_the_request_timeout`, `::an_action_that_keeps_its_session_completes_and_the_link_stays_connected` ; `crates/hearth-link/tests/session_end_actions.rs` (vrai agent, cinq actions)
