---
id: BR-LIFE-010
domaine: LIFE
titre: L'arrêt demandé depuis Hearth est écrit AVANT l'ordre : une sortie non nulle voulue n'est pas « En erreur », même après un redémarrage de l'agent
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-cycle-de-vie-service.md (BR-LIFE-010), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md, HRT-64
maj: 2026-10-17
---

# BR-LIFE-010 — L'arrêt demandé depuis Hearth est écrit AVANT l'ordre : une sortie non nulle voulue n'est pas « En erreur », même après un redémarrage de l'agent

## Règle
`service_intents` reçoit la ligne (clé du service, conteneur, compte, date) AVANT l'ordre d'arrêt ; si l'écriture échoue, l'ordre n'est pas donné (opération échouée, cause `step_failed`). Tant que l'arrêt voulu existe, une sortie non nulle d'un service arrêté (137 d'un arrêt forcé, 143…) est « Arrêté », pas « En erreur » (`honour_stop_request`). Il s'efface quand Hearth démarre le service, quand l'ordre est refusé (rien n'a été arrêté), quand le service est revu en marche DEPUIS la demande (démarrage postérieur à la date de la demande), ou quand le service n'existe plus ; ce que le registre défait seul s'efface par sa date (`clear_if`), jamais un arrêt redemandé entre-temps. Un résultat inconnu (délai, moteur perdu) le GARDE : l'arrêt a peut-être eu lieu. Il survit à un redémarrage de l'agent (relu au démarrage). Un redémarrage dont le démarrage échoue après l'arrêt le retient après coup (BR-LIFE-012).

## Application (code)
- `crates/hearth-agent/src/application/ports/service_intents.rs::{ServiceIntents, StopIntent}`
- `crates/hearth-agent/src/infrastructure/sqlite/intents_repo.rs`
- `crates/hearth-agent/src/domain/services/state.rs::honour_stop_request`
- `crates/hearth-agent/src/application/services.rs::{ServicesRegistry::note_stop_requested, forget_stop_requested, load_intents, apply_hearth_actions, flush_intent_clears}`
- `crates/hearth-agent/src/application/service_lifecycle.rs::ServiceLifecycle::{run_stop, forget_stop, keep_stop}`

## Vérification
- `crates/hearth-agent/tests/service_intents_repo.rs`
- `crates/hearth-agent/src/application/services.rs::tests::a_stop_asked_from_hearth_makes_a_non_zero_exit_a_stop_until_the_service_runs_again`
- `crates/hearth-agent/tests/service_lifecycle_https.rs::the_wanted_stop_survives_an_agent_restart_and_is_forgotten_when_the_service_runs_again`
- `crates/hearth-agent/tests/service_lifecycle_https.rs::a_non_zero_exit_after_a_stop_asked_from_hearth_is_a_stop_not_an_error`

## Cas limites
- Un service créé par Hearth arrêté depuis Hearth reste arrêté après un redémarrage du serveur tant que le moteur démarre avec le serveur (`unless-stopped`) : le moteur, pas l'agent, s'en charge.

## Règles liées
- BR-SVC-005, BR-LIFE-011, BR-LIFE-012

## Historique
- 2026-10-17 — l'effacement par le registre ne vise que la demande défaite (review HRT-64).
- 2026-10-17 — création (HRT-64 : démarrer, arrêter, redémarrer un service, côté agent).
