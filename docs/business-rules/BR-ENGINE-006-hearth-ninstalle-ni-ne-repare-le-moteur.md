---
id: BR-ENGINE-006
domaine: ENGINE
titre: Hearth n'installe ni ne répare le moteur : le port est en lecture seule
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-moteur-de-conteneurs.md (BR-ENGINE-006), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §5, §7, §9, HRT-61
maj: 2026-10-13
---

# BR-ENGINE-006 — Hearth n'installe ni ne répare le moteur : le port est en lecture seule

## Règle
Hearth n'installe, ne démarre, ne répare et ne configure jamais le moteur lui-même dans ce lot. Le port `ContainerEngine` n'a que `probe`, `list`, `inspect` et `events` : aucune écriture dans le moteur, aucune route qui en lance une, aucune migration.

## Application (code)
- `crates/hearth-agent/src/application/ports/container_engine.rs::ContainerEngine`
- `crates/hearth-agent/src/infrastructure/engine/bollard.rs (rien ne dépasse des lectures)`

## Vérification
- `crates/hearth-agent/tests/engine_fake_daemon.rs::the_bollard_adapter_honours_the_engine_contract_against_a_fake_daemon (le démon ne voit que des GET)`
- `crates/hearth-agent/src/entrypoint/http/mod.rs::tests::the_services_routes_are_reads_open_to_both_roles_and_never_journaled`

## Historique
- 2026-10-13 — création (HRT-61, tranche 1 des déploiements : moteur détecté et liste en lecture seule).
