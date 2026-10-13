---
id: BR-ENGINE-010
domaine: ENGINE
titre: Seul Docker est pris en charge : un autre moteur est non pris en charge
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-moteur-de-conteneurs.md (BR-ENGINE-010), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §5, §7, §9, HRT-61
maj: 2026-10-13
---

# BR-ENGINE-010 — Seul Docker est pris en charge : un autre moteur est non pris en charge

## Règle
Seul Docker est pris en charge : un moteur qui s'annonce autrement (Podman, même avec l'API compatible) est « non pris en charge pour l'instant ». L'agent donne le nom annoncé ; l'interface indique que Docker est le moteur pris en charge.

## Application (code)
- `crates/hearth-agent/src/domain/services/engine.rs::is_supported_engine`
- `crates/hearth-agent/src/domain/services/engine.rs::classify_probe`

## Vérification
- `crates/hearth-agent/src/domain/services/engine.rs::tests::only_docker_is_a_supported_engine`
- `crates/hearth-agent/tests/engine_fake_daemon.rs::a_podman_engine_is_an_unsupported_engine`

## Historique
- 2026-10-13 — création (HRT-61, tranche 1 des déploiements : moteur détecté et liste en lecture seule).
