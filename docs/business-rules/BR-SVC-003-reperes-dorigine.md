---
id: BR-SVC-003
domaine: SVC
titre: Repères d'origine : tout service est « créé hors de Hearth » dans cette tranche
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-voir-les-services.md (BR-SVC-003), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §5, §7, §9, HRT-61
maj: 2026-10-13
---

# BR-SVC-003 — Repères d'origine : tout service est « créé hors de Hearth » dans cette tranche

## Règle
Un service créé hors de Hearth porte le repère « Créé hors de Hearth » ; le repère est le seul signe d'origine, jamais déduit du nom ou de l'image. Dans cette tranche, aucun service n'est créé par Hearth : tout service est `external`. Le repère « Pile Compose {projet} » vient de l'étiquette `com.docker.compose.project`, pour information. « Géré par le système » arrive avec la tranche 2.

## Application (code)
- `crates/hearth-proto/src/api/services.rs::ServiceOrigin`
- `crates/hearth-agent/src/entrypoint/services_wire.rs::service_item (origin = External)`
- `crates/hearth-agent/src/infrastructure/engine/translate.rs::observation_from (compose_project)`

## Vérification
- `crates/hearth-agent/src/entrypoint/services_wire.rs::tests::a_service_item_is_external_and_carries_no_future_field`
- `crates/hearth-agent/src/infrastructure/engine/translate.rs::tests::a_running_container_is_translated`

## Historique
- 2026-10-13 — création (HRT-61, tranche 1 des déploiements : moteur détecté et liste en lecture seule).
