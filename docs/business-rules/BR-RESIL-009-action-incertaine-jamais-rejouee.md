---
id: BR-RESIL-009
domaine: RESIL
titre: Une action coupée avant sa réponse est « résultat inconnu » et n'est jamais rejouée
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-lien-resilient.md (BR-RESIL-009), ADR-0004, ADR-0007
maj: 2026-10-05
---

# BR-RESIL-009 — Une action coupée avant sa réponse est « résultat inconnu » et n'est jamais rejouée

## Règle
Une action envoyée porte une clé d'opération (`Idempotency-Key`, un ULID). Si le lien tombe avant la réponse, l'appelant reçoit tout de suite « résultat inconnu » avec la clé, l'opération reste suivie, et la bibliothèque ne la renvoie **jamais** d'elle-même : seul l'utilisateur peut relancer. Hors « Connecté », `execute` refuse sans rien envoyer (BR-RESIL-008). Le nombre d'opérations suivies est borné (256).

## Application (code)
- `crates/hearth-link/src/domain/pending_ops.rs::PendingOps::{register, complete, link_lost}`.
- `crates/hearth-link/src/manager/task.rs` : `execute` (clé, suivi, réponse « inconnu » à la coupure).

## Vérification
- Tests : `domain::pending_ops::tests::a_dropped_link_makes_in_flight_operations_unknown_and_nothing_replays_them`, `::a_response_before_the_link_drops_forgets_the_operation`, `::the_number_of_tracked_operations_is_bounded`.
- Intégration : `tests/fault_proxy.rs::an_action_cut_before_the_answer_is_unknown_and_never_replayed`, `::an_action_is_refused_without_sending_anything_when_the_link_is_not_connected`, `::a_completed_action_returns_the_agent_answer_even_when_it_is_a_refusal`.

## Cas limites
- Un résultat connu (réponse reçue, même une erreur 4xx) n'est pas « inconnu ».
- Jamais de rejeu automatique, même si l'agent répondrait « non exécuté » : l'utilisateur décide.

## Règles liées
- BR-RESIL-010, BR-RESIL-008.

## Historique
- 2026-10-05 — création (HRT-07, session 2026-10-04-hearth-creation).
