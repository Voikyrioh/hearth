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
Un service est « en erreur » quand il est arrêté avec un code de sortie non nul sans que l'arrêt ait été demandé depuis Hearth ; arrêté avec le code zéro, ou arrêté par une demande Hearth, il est « arrêté ». Le code du dernier arrêt est fourni dans les deux cas. Dans les tranches 1 et 2 (aucune action), l'arrêt n'est jamais « demandé » : `derive_state` reçoit `stop_requested = false` ; l'argument sert aux tranches suivantes.

Service supervisé (HRT-63) : son conteneur est supprimé à chaque arrêt, l'état vient de l'UNITÉ tant que le conteneur n'est pas en marche (`derive_supervised`, `derive_unit_state`) : unité arrêtée = « arrêté » ; unité en échec constaté par le système = « en erreur » ; unité qui démarre = « démarrage » ; unité qui attend de se relancer après un arrêt = « redémarre en boucle » ; unité en arrêt = « arrêt en cours ». Le code affiché est celui du dernier arrêt VU par cet agent (jamais relu du système) : absent après un redémarrage de l'agent. Superviseur muet (pas de `systemctl`) : « état non reconnu », jamais « arrêté » à tort.

## Application (code)
- `crates/hearth-agent/src/domain/services/state.rs::derive_state`
- `crates/hearth-agent/src/domain/services/state.rs::{derive_supervised, derive_unit_state}`

## Vérification
- `crates/hearth-agent/src/domain/services/state.rs::tests::derive_state_exit_codes`
- `crates/hearth-agent/src/domain/services/state.rs::tests::a_supervised_service_without_container_takes_its_state_from_the_unit`
- `crates/hearth-agent/src/domain/services/state.rs::tests::a_supervised_service_follows_its_container_when_it_runs_and_its_unit_when_it_does_not`
- `crates/hearth-agent/src/application/services.rs::tests::an_unknown_unit_state_is_not_a_stopped_service`

## Cas limites
- 137 sans arrêt demandé : « en erreur » (arrêt forcé ou manque de mémoire).

## Historique
- 2026-10-13 — création (HRT-61, tranche 1 des déploiements : moteur détecté et liste en lecture seule).
- 2026-10-13 — HRT-63 : état d'un service supervisé d'après son unité.
