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
Une action envoyée porte une clé d'opération (`Idempotency-Key`, un ULID). Si le lien tombe avant la réponse, l'appelant reçoit tout de suite « résultat inconnu » avec la clé, l'opération reste suivie, et la bibliothèque ne la renvoie **jamais** d'elle-même : seul l'utilisateur peut relancer. Hors « Connecté », `execute` refuse sans rien envoyer (BR-RESIL-008). Le nombre d'opérations suivies est borné (256). La clé est choisie par la façade : si la requête dépasse son délai, ou si l'appelant abandonne l'attente, `execute` rend (ou la tâche retient) `ResultUnknown` avec la clé, jamais un simple délai dépassé, et l'opération reste suivie. **Persister PUIS envoyer** : le suivi de l'action est écrit et confirmé sur disque (écriture atomique terminée, attente bornée à 2 s, dans le chemin de l'action et non dans la boucle du lien) AVANT que la requête parte ; si l'écriture échoue ou dépasse ce délai, l'action n'est PAS envoyée et `execute` rend `LinkError::TrackingUnavailable` (« suivi impossible, l'action n'a pas été lancée »). Une application tuée juste après l'envoi retrouve donc l'opération au redémarrage. Si le fichier des suivis est illisible, il est mis de côté en `.corrupt`, une entrée invalide n'emporte pas les autres, et `Event::OperationsLost` signale que des suivis ont pu être perdus. Les opérations en suspens sont écrites sur disque (un fichier par serveur, écriture atomique, 24 h au plus comme l'agent) et survivent à un redémarrage de l'application ; elles sont relues au premier retour du lien.

## Application (code)
- `crates/hearth-link/src/domain/pending_ops.rs::PendingOps::{register, complete, link_lost}`.
- `crates/hearth-link/src/manager/task.rs` : `execute` (clé, suivi, réponse « inconnu » à la coupure).
- Interface : `apps/desktop/src/composables/useServerAction.ts` (message « Le résultat de cette action n'est pas connu. » puis « Vérifie l'état du serveur, puis relance l'action si besoin. », jamais de rejeu) ; chaque commande typée à venir rend `ActionResult::unknown { opId }` (ADR-0016).

## Vérification
- Tests : `domain::pending_ops::tests::a_dropped_link_makes_in_flight_operations_unknown_and_nothing_replays_them`, `::a_response_before_the_link_drops_forgets_the_operation`, `::the_number_of_tracked_operations_is_bounded`.
- Intégration : `tests/fault_proxy.rs::an_action_cut_before_the_answer_is_unknown_and_never_replayed`, `::an_action_is_refused_without_sending_anything_when_the_link_is_not_connected`, `::a_completed_action_returns_the_agent_answer_even_when_it_is_a_refusal`, `::an_abandoned_action_stays_tracked_and_its_outcome_is_announced`, `::an_unknown_operation_survives_a_restart_of_the_application` ; `crates/hearth-link/tests/tracking.rs::a_failed_write_means_the_action_is_not_sent_and_the_caller_is_told`, `::a_write_that_never_ends_does_not_send_the_action_either`, `::a_slow_write_delays_the_request_until_it_is_done` (écriture retenue à une porte, le test l'ouvre : aucune horloge), `::a_crash_right_after_the_send_leaves_the_operation_on_disk_for_the_restart` ; `domain::pending_ops::tests::pending_operations_survive_a_restart_as_unknown_and_old_ones_are_dropped`.
- Coquille, contre un vrai agent : `apps/desktop/src-tauri/tests/offline.rs``::an_action_cut_before_the_answer_is_unknown_never_replayed_and_its_outcome_comes_back` (l'agent ne voit qu'une exécution).
- Interface : `apps/desktop/src/stores/offline.test.ts``::dit que le résultat n'est pas connu, ne rejoue jamais…`, `apps/desktop/e2e/offline.spec.ts` (« action lancée à la coupure »).

## Cas limites
- Un résultat connu (réponse reçue, même une erreur 4xx) n'est pas « inconnu ».
- Jamais de rejeu automatique, même si l'agent répondrait « non exécuté » : l'utilisateur décide.

## Règles liées
- BR-RESIL-010, BR-RESIL-008.

## Historique
- 2026-10-05 — création (HRT-07, session 2026-10-04-hearth-creation).
- 2026-10-05 — précisé (HRT-07, review Stephen round 1).
- Si le lien tombe pendant l'écriture du suivi, l'appelant reçoit « résultat inconnu » tout de suite, la tâche qui devait envoyer est interrompue (la requête n'est jamais partie) et l'issue « Non exécuté, tu peux relancer » arrive par la relecture au retour du lien (404), une seule fois ; `manager/task.rs::on_not_sent` ne traite que l'appelant encore présent. Test : `tests/tracking.rs::a_link_cut_while_the_tracking_is_written_ends_as_not_executed_exactly_once`. Un disque trop lent (délai dépassé) est distingué d'une écriture en échec : `LinkError::TrackingSlow` / `TrackingUnavailable`, deux textes (`failure.trackingSlow`, `failure.trackingUnavailable`). L'écriture du suivi d'une action ne passe jamais derrière celle de la dernière vue (`manager/persist.rs`, deux files). Un fichier d'opérations illisible (lecture en erreur, pas seulement mal formé) est mis de côté en `.corrupt` et signalé par `Event::OperationsLost`, affiché à l'utilisateur (`bridge.operationsLost`). Tests : `persist::tests::a_hung_view_write_never_delays_the_acknowledgement_of_an_action`, `file_store::tests::an_operations_file_that_cannot_be_read_is_set_aside_and_reported`, `tests/tracking.rs::a_write_that_never_ends_does_not_send_the_action_either`.
- 2026-10-05 : issue manquante, disque lent, fichier d'opérations illisible, file d'écriture séparée (HRT-10, suivis de review HRT-07).
- 2026-10-05 : parties coquille et interface (HRT-12) ; tests à temps réel rendus déterministes (porte `Disk::Gated`, agent qui retient l'action).
- 2026-10-06 : premier appelant de production de `useServerAction` (HRT-13) : `apps/desktop/src/composables/useAccountActions.ts` (une commande typée par action de compte, ADR-0018) ; texte de coupure propre à l'écran des comptes (`accounts.unknownResult`), liste relue au retour du lien. Tests : `apps/desktop/src/pages/Accounts.test.ts`, `apps/desktop/e2e/accounts.spec.ts`, `apps/desktop/src-tauri/tests/accounts_runtime.rs::an_action_cut_before_its_answer_is_unknown_never_replayed_and_its_outcome_comes_back`.
