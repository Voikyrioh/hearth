---
id: BR-SVC-003
domaine: SVC
titre: Repères d'origine : « créé hors de Hearth », « géré par le système », pile Compose
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-voir-les-services.md (BR-SVC-003), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §5, §7, §9, HRT-61
maj: 2026-10-13
---

# BR-SVC-003 — Repères d'origine

## Règle
Un service créé hors de Hearth et adopté porte le repère « Créé hors de Hearth » (`origin = external`), accompagné du repère « Adopté » (`adopted`, avec le compte et la date) ; un conteneur non adopté n'est pas dans la liste (BR-ADOPT-001, amendé par HRT-79). Le repère est le seul signe d'origine, jamais déduit du nom ou de l'image. Une clé `h_` donnerait `origin = hearth` et aucun repère d'adoption ; l'agent n'en produit pas encore (le déploiement est un autre ticket). Le repère « Pile Compose {projet} » vient de l'étiquette `com.docker.compose.project`, pour information (`compose_project`). Le repère « Géré par le système ({unité}) » (HRT-63) est dit par `driver = supervisor` et `unit` : le service est lancé et relancé par une unité systemd, détectée par `/proc` (BR-SVC-016) ; il ne dit pas qui a créé le service (il peut être « Créé hors de Hearth » en même temps). Le texte des repères est porté par l'interface.

## Application (code)
- `crates/hearth-proto/src/api/services.rs::ServiceOrigin`
- `crates/hearth-agent/src/entrypoint/services_wire.rs::service_item (origin, adopted ; driver, unit)`
- `crates/hearth-agent/src/infrastructure/engine/translate.rs::observation_from (compose_project)`

## Vérification
- `crates/hearth-agent/src/entrypoint/services_wire.rs::tests::a_service_item_is_external_engine_driven_and_carries_no_future_field`
- `crates/hearth-agent/src/entrypoint/services_wire.rs::tests::a_service_made_by_hearth_keeps_its_details_for_every_account`
- `crates/hearth-agent/src/entrypoint/services_wire.rs::tests::a_supervised_service_names_its_unit_its_driver_and_its_own_grace`
- `crates/hearth-agent/src/infrastructure/engine/translate.rs::tests::a_running_container_is_translated`

## Historique
- 2026-10-13 — création (HRT-61, tranche 1 des déploiements : moteur détecté et liste en lecture seule).
- 2026-10-13 — HRT-63 : repère « géré par le système » (`driver`, `unit`).
- 2026-10-13 — HRT-79 (ADR-0043) : le repère « Créé hors de Hearth » n'est plus que celui d'un service adopté, avec « Adopté » ; `ServiceItem.adopted`.
