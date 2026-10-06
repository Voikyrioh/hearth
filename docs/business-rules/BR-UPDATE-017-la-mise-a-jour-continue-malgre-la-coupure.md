---
id: BR-UPDATE-017
domaine: UPDATE
titre: La mise à jour continue côté serveur malgré une coupure réseau
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-mises-a-jour.md (BR-UPDATE-017), HRT-17
maj: 2026-10-06
---

# BR-UPDATE-017 : La mise à jour continue côté serveur malgré une coupure réseau

## Règle
La mise à jour s'exécute dans une tâche détachée de la requête (réponse `202` immédiate), puis dans le superviseur : la coupure du client, ou un changement de compte, ne l'arrête pas. Le résultat est un fichier (`update/last.json`) qui survit au redémarrage : à son retour, le client le lit par `GET /agent/update/last` (ou `GET /agent/update`) et par l'abonnement au sujet `update` (état courant d'abord). Rejouer la même clé d'opération renvoie la première réponse sans relancer.

## Application (code)
- `crates/hearth-agent/src/application/update.rs::UpdateService::{start, last, status}`.
- `crates/hearth-agent/src/infrastructure/update/host.rs::FsUpdateHost::{read_last, write_last}` (écriture atomique).
- `crates/hearth-agent/src/entrypoint/http/update.rs`.

## Vérification
- `tests/update_http.rs` : `replaying_the_same_operation_key_after_a_cut_does_not_start_a_second_update`, `the_last_result_is_readable_by_any_account_and_empty_before_the_first_update`.
- `tests/update_use_cases.rs::the_result_survives_the_loss_of_the_service_itself`.
- `tests/update_stream.rs::a_client_that_subscribes_in_the_middle_gets_the_current_step_then_the_result`.
- `infrastructure::update::host::tests::the_last_result_and_the_state_survive_a_new_host_on_the_same_directory`.

## Interface (HRT-17, lot interface)
- Au retour du lien, l'interface relit l'état et le dernier résultat (`get_agent_update` : `GET /agent/update`, dont le champ `last` EST le dernier résultat ; `GET /agent/update/last` reste lisible par la bibliothèque, non utilisé par l'écran) et affiche le résultat réel : message de la carte si le résultat date de moins de 24 h (`recent`, décidé par la coquille avec une tolérance de 1 h sur l'horloge du serveur en avance), simple ligne d'historique sinon ; l'ANNONCE ne dépend pas de `recent` (un résultat jamais annoncé s'annonce, jamais perdu pour un décalage d'horloge). Le résultat n'est PAS attendu sur le flux au retour : le nouvel agent l'annonce dès son démarrage, souvent avant que le client ne se soit réabonné, et l'état courant d'un abonnement ne rejoue que ce qui est en cours : c'est la lecture qui le donne ET qui l'annonce, une fois (l'étape `done` du flux n'est qu'un signal, BR-UPDATE-015). Une lecture plus ancienne qu'un événement reçu depuis ne l'écrase pas ; une lecture qui échoue garde la dernière sans message.
- Code : `apps/desktop/src/stores/agentUpdates.ts` (`refresh`, surveillance des retours du lien), `apps/desktop/src-tauri/src/agent_update/service.rs::{view, is_recent}`, `crates/hearth-link/src/manager/agent_update.rs`.
- Tests : `crates/hearth-link/tests/agent_update.rs::a_restart_announced_by_the_agent_is_an_expected_cut_then_the_result_is_read_back` ; `apps/desktop/src-tauri/tests/agent_update_runtime.rs` (`the_agent_stays_the_arbiter…`, `a_result_is_recent_for_a_day…`) ; `apps/desktop/src/stores/agentUpdates.test.ts` ; `apps/desktop/src/components/organisms/agentUpdate.test.ts`, `apps/desktop/e2e/agent-update.spec.ts` (« keeps the steps through a cut of the link… »).

## Cas limites
- Le serveur qui redémarre pendant la mise à jour avant le lancement du superviseur : aucun résultat (l'agent n'a pas changé) ; après : le superviseur termine seul.

## Règles liées
- ADR-0008 (mises à jour signées), ADR-0012 (service système), ADR-0014 (dépendances de la mise à jour)

## Historique
- 2026-10-05 : création (HRT-17, lot agent, session 2026-10-04-hearth-creation).
- 2026-10-06 : section Interface (HRT-17, lot interface, session 2026-10-04-hearth-creation, T28).
