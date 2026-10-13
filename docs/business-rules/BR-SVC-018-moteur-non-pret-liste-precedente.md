---
id: BR-SVC-018
domaine: SVC
titre: Moteur non prêt : liste conservée (inaccessible) ou vidée (absent, non pris en charge, désactivé)
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-voir-les-services.md (BR-SVC-018), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §5, §7, §9, HRT-61
maj: 2026-10-13
---

# BR-SVC-018 — Moteur non prêt : liste conservée (inaccessible) ou vidée (absent, non pris en charge, désactivé)

## Règle
Quand le moteur n'est pas prêt, la dernière liste connue reste dans le registre pour les états inaccessibles (arrêté, accès refusé, ne répond pas) : le client la grise et la date (« Liste du moteur d'il y a {âge} »). Dans les états absent, non pris en charge, version non prise en charge et désactivé, aucune liste ancienne n'est gardée : le registre se vide.

## Application (code)
- `crates/hearth-agent/src/domain/services/engine.rs::EngineState::{is_inaccessible, keeps_no_list}`
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::commit`

## Vérification
- `crates/hearth-agent/src/application/services.rs::tests::an_inaccessible_engine_keeps_the_last_list_and_a_missing_one_drops_it`
- `crates/hearth-agent/src/domain/services/engine.rs::tests::inaccessible_states_keep_the_list_and_missing_ones_drop_it`

## Historique
- 2026-10-13 — création (HRT-61, tranche 1 des déploiements : moteur détecté et liste en lecture seule).
