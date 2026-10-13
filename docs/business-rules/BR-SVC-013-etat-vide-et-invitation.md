---
id: BR-SVC-013
domaine: SVC
titre: État vide : liste vide quand le moteur est prêt sans service
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-voir-les-services.md (BR-SVC-013), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §5, §7, §9, HRT-61
maj: 2026-10-13
---

# BR-SVC-013 — État vide : liste vide quand le moteur est prêt sans service

## Règle
Quand le moteur est prêt et qu'il n'y a aucun service, la liste est vide ; le texte « Aucun service pour l'instant » et l'invitation à déployer (administrateurs) sont portés par l'interface. L'agent rend `services: []` avec l'état prêt.

## Application (code)
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::snapshot`
- `crates/hearth-proto/src/api/services.rs::ServicesState`

## Vérification
- `crates/hearth-agent/src/application/services.rs::tests::a_ready_engine_without_a_container_gives_an_empty_list`

## Historique
- 2026-10-13 — création (HRT-61, tranche 1 des déploiements : moteur détecté et liste en lecture seule).
