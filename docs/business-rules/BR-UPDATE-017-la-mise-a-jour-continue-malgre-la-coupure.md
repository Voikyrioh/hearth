---
id: BR-UPDATE-017
domaine: UPDATE
titre: La mise à jour continue côté serveur malgré une coupure réseau
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-mises-a-jour.md (BR-UPDATE-017), HRT-17
maj: 2026-10-05
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

## Cas limites
- Le serveur qui redémarre pendant la mise à jour avant le lancement du superviseur : aucun résultat (l'agent n'a pas changé) ; après : le superviseur termine seul.

## Règles liées
- ADR-0008 (mises à jour signées), ADR-0012 (service système), ADR-0014 (dépendances de la mise à jour)

## Historique
- 2026-10-05 : création (HRT-17, lot agent, session 2026-10-04-hearth-creation).
