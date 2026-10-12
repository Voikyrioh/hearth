---
id: BR-SVC-019
domaine: SVC
titre: 200 services : premier instantané en moins d'une seconde
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-voir-les-services.md (BR-SVC-019), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §5, §7, §9, HRT-61
maj: 2026-10-13
---

# BR-SVC-019 — 200 services : premier instantané en moins d'une seconde

## Règle
Avec 200 services, le premier instantané part en moins d'1 seconde après l'abonnement (le registre est en mémoire) et un delta moins de 500 ms après l'événement ; la liste n'est jamais découpée en pages. La relecture complète lit les conteneurs par lots de 8 inspections en parallèle. La fluidité de l'affichage (recherche, défilement) est portée par l'interface.

## Application (code)
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::subscribe`
- `crates/hearth-agent/src/infrastructure/engine/bollard.rs::BollardEngine::list (INSPECT_CONCURRENCY)`

## Vérification
- `crates/hearth-agent/tests/services_https.rs::two_hundred_services_are_in_the_first_snapshot_in_under_a_second`
- `crates/hearth-agent/tests/services_https.rs::subscribing_sends_a_snapshot_with_the_engine_and_every_service_then_deltas_in_time`

## Historique
- 2026-10-13 — création (HRT-61, tranche 1 des déploiements : moteur détecté et liste en lecture seule).
