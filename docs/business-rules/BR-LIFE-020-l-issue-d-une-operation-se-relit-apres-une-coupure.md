---
id: BR-LIFE-020
domaine: LIFE
titre: L'issue d'une opération se relit après une coupure du lien : l'opération reste lisible 24 heures après sa fin
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (section 0 bis), contexts/hearth/tickets/hrt/HRT-144.md
maj: 2026-10-14
---

# BR-LIFE-020 — L'issue d'une opération se relit après une coupure du lien : l'opération reste lisible 24 heures après sa fin

## Règle
Un client qui perd le lien pendant une opération de service n'a pas à deviner ce qui s'est passé : l'agent garde l'opération (genre, statut, étapes, cause) et la rend par `GET /operations/services/{id}` pendant 24 heures après sa fin. Une opération qui n'est pas terminée ne s'efface jamais. Le même état arrive par le message `operation` du sujet `services` : à la reconnexion, l'agent renvoie les opérations en cours que le compte voit. L'agent ne rejoue rien pour le client : relire n'a aucun effet. Les statuts à relire après une coupure : inconnue (404), réussie, en cours, échouée, interrompue (l'agent a redémarré, l'état est cohérent), décision attendue, à vérifier.

## Application (code)
- `crates/hearth-agent/src/application/service_op_access.rs::ServiceOpAccess::read`
- `crates/hearth-agent/src/application/service_ops.rs::ServiceOps::{snapshot, sweep}`
- `crates/hearth-agent/src/entrypoint/http/service_ops.rs::get`
- `crates/hearth-link/src/manager/service_ops.rs::LinkManager::service_operation`

## Vérification
- `crates/hearth-agent/tests/service_ops_https.rs::any_account_reads_an_operation_of_a_hearth_service_with_its_steps_and_progress`
- `crates/hearth-agent/tests/service_ops_journal.rs` (purge à 24 heures, opération non terminée jamais purgée)
- `crates/hearth-link/tests/service_ops.rs::an_operation_is_read_and_followed_live_by_the_link`
- `crates/hearth-agent/tests/service_lifecycle_https.rs::an_action_cut_by_an_agent_restart_is_interrupted_frees_the_service_and_is_never_replayed`
- `crates/hearth-agent/src/domain/ops/recover.rs::tests::a_lifecycle_action_cut_by_a_restart_is_interrupted_and_frees_the_service`

## Cas limites
- La table de relecture côté client (HRT-84) arrive avec les écrans ; la liaison fournit la lecture typée.
- Démarrer, arrêter, redémarrer (HRT-64) : au démarrage de l'agent, une action restée `running` devient `interrupted` (cause `interrupted`, « résultat inconnu »), son verrou n'existe plus, rien n'est rejoué, et l'interruption est consignée au journal d'activité (`record_interrupted`). Leurs plans n'ont aucune étape destructive : jamais « à vérifier ».

## Règles liées
- BR-DEPLOY-028, BR-OPS-001, BR-OPS-004, BR-RESIL-009, BR-EDIT-031

## Historique
- 2026-10-17 — les actions de cycle de vie interrompues par un redémarrage de l'agent (HRT-64).
- 2026-10-14 — création (HRT-144 : le socle des opérations branché sur des routes et sur le flux).
