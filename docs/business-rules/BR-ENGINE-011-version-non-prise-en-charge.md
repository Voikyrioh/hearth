---
id: BR-ENGINE-011
domaine: ENGINE
titre: Docker plus ancien que 20.10 (API 1.41) : version non prise en charge
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-moteur-de-conteneurs.md (BR-ENGINE-011), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §5, §7, §9, HRT-61
maj: 2026-10-13
---

# BR-ENGINE-011 — Docker plus ancien que 20.10 (API 1.41) : version non prise en charge

## Règle
Un Docker plus ancien que la version minimale est « version non prise en charge ». Version minimale : Docker Engine 20.10, soit l'API 1.41 (ADR-0036). L'agent donne la version détectée (`version`) et la version minimale (`min_version`).

## Application (code)
- `crates/hearth-agent/src/domain/services/engine.rs::MIN_API_VERSION`
- `crates/hearth-agent/src/domain/services/engine.rs::ApiVersion`
- `crates/hearth-agent/src/domain/services/engine.rs::classify_probe`
- `crates/hearth-proto/src/api/services.rs::MIN_ENGINE_VERSION`

## Vérification
- `crates/hearth-agent/src/domain/services/engine.rs::tests::an_api_version_compares_numerically`
- `crates/hearth-agent/tests/engine_fake_daemon.rs::an_engine_older_than_the_minimum_is_an_unsupported_version`

## Historique
- 2026-10-13 — création (HRT-61, tranche 1 des déploiements : moteur détecté et liste en lecture seule).
