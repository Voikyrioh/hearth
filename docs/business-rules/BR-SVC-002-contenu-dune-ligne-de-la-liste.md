---
id: BR-SVC-002
domaine: SVC
titre: Contenu d'une ligne de la liste des services
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-voir-les-services.md (BR-SVC-002), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §5, §7, §9, HRT-61
maj: 2026-10-13
---

# BR-SVC-002 — Contenu d'une ligne de la liste des services

## Règle
Chaque ligne montre : le nom, l'image avec sa version, l'état, depuis quand, les ports exposés et les repères. L'agent fournit ces données dans `ServiceItem` (nom, `image`, `state`, `since`, `exit_code`, `restarts_10m`, `never_started`, `ports`, `compose_project`) ; le format des durées (« 3 j 4 h 12 min »), « Jamais démarré », « Arrêté il y a 2 h » et la coupe à 3 ports sont portés par l'interface.

## Application (code)
- `crates/hearth-proto/src/api/services.rs::ServiceItem`
- `crates/hearth-agent/src/entrypoint/services_wire.rs::service_item`
- `crates/hearth-agent/src/domain/services/state.rs::derive_state (`since`, `never_started`)`

## Vérification
- `crates/hearth-agent/src/entrypoint/services_wire.rs::tests::a_service_item_is_external_engine_driven_and_carries_no_future_field`
- `crates/hearth-agent/src/infrastructure/engine/translate.rs::tests::a_running_container_is_translated`

## Historique
- 2026-10-13 — création (HRT-61, tranche 1 des déploiements : moteur détecté et liste en lecture seule).
