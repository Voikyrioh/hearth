---
id: BR-SVC-005
domaine: SVC
titre: « En erreur » : code de sortie non nul sans arrêt demandé par Hearth
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-voir-les-services.md (BR-SVC-005), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §5, §7, §9, HRT-61
maj: 2026-10-13
---

# BR-SVC-005 — « En erreur » : code de sortie non nul sans arrêt demandé par Hearth

## Règle
Un service est « en erreur » quand il est arrêté avec un code de sortie non nul sans que l'arrêt ait été demandé depuis Hearth ; arrêté avec le code zéro, ou arrêté par une demande Hearth, il est « arrêté ». Le code du dernier arrêt est fourni dans les deux cas. Depuis HRT-64, l'arrêt demandé depuis Hearth est écrit AVANT l'ordre (`service_intents`, BR-LIFE-010) et appliqué après la dérivation (`honour_stop_request` : une sortie non nulle d'un service dont l'arrêt est demandé est « arrêtée », pour un conteneur comme pour une unité en échec) ; `derive_state` garde son argument `stop_requested`, que le registre laisse à `false` (le recouvrement se fait dans `apply_hearth_actions`, pour que l'unité soit couverte aussi).

Service supervisé (HRT-63) : son conteneur est supprimé à chaque arrêt, l'état vient de l'UNITÉ tant que le conteneur n'est pas en marche (`derive_supervised`, `derive_unit_state`) : unité arrêtée = « arrêté » ; unité en échec constaté par le système = « en erreur » ; unité qui démarre ou qui attend de se relancer après un arrêt = « démarrage » (« redémarre en boucle » seulement à trois démarrages en dix minutes, BR-SVC-006) ; « démarrage » sans conteneur dure au plus 10 minutes (`STARTING_MAX`), ensuite « état non reconnu » : l'unité se dit active mais le moteur n'a rien, un service ne reste pas « Démarrage » à vie ; unité en arrêt = « arrêt en cours ». Le code affiché est celui du dernier arrêt VU par cet agent (jamais relu du système) : absent après un redémarrage de l'agent. Superviseur muet (pas de `systemctl`) : « état non reconnu », jamais « arrêté » à tort.

## Application (code)
- `crates/hearth-agent/src/domain/services/state.rs::derive_state`
- `crates/hearth-agent/src/domain/services/state.rs::{derive_supervised, derive_unit_state, honour_stop_request}`
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::apply_hearth_actions`

## Vérification
- `crates/hearth-agent/src/domain/services/state.rs::tests::derive_state_exit_codes`
- `crates/hearth-agent/src/domain/services/state.rs::tests::a_supervised_service_without_container_takes_its_state_from_the_unit`
- `crates/hearth-agent/src/domain/services/state.rs::tests::a_supervised_service_follows_its_container_when_it_runs_and_its_unit_when_it_does_not`
- `crates/hearth-agent/src/application/services.rs::tests::an_unknown_unit_state_is_not_a_stopped_service`
- `crates/hearth-agent/src/application/services.rs::tests::a_stop_asked_from_hearth_makes_a_non_zero_exit_a_stop_until_the_service_runs_again`
- `crates/hearth-agent/src/domain/services/state.rs::tests::a_stop_asked_from_hearth_turns_an_error_into_a_stop_and_changes_nothing_else`
- `crates/hearth-agent/tests/service_lifecycle_https.rs::a_non_zero_exit_after_a_stop_asked_from_hearth_is_a_stop_not_an_error`

## Cas limites
- 137 sans arrêt demandé : « en erreur » (arrêt forcé ou manque de mémoire). Avec un arrêt demandé depuis Hearth, le même 137 est « arrêté ».

## Historique
- 2026-10-13 — création (HRT-61, tranche 1 des déploiements : moteur détecté et liste en lecture seule).
- 2026-10-13 — HRT-63 : état d'un service supervisé d'après son unité.
- 2026-10-17 — HRT-64 : l'arrêt demandé existe (`service_intents`, `honour_stop_request`).
