---
id: BR-ENGINE-002
domaine: ENGINE
titre: Le moteur est dans un des huit états fermés (famille « inaccessible » comprise)
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-moteur-de-conteneurs.md (BR-ENGINE-002), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §5, §7, §9, HRT-61
maj: 2026-10-13
---

# BR-ENGINE-002 — Le moteur est dans un des huit états fermés (famille « inaccessible » comprise)

## Règle
Le moteur est toujours dans un et un seul de huit états : prêt, arrêté, accès refusé, ne répond pas, absent, non pris en charge (autre moteur), version non prise en charge, désactivé. Les trois états arrêté, accès refusé et ne répond pas forment la famille « inaccessible ». `checking` n'est pas un des huit états (BR-ENGINE-019). La table `classify_probe` couvre les huit.

## Application (code)
- `crates/hearth-agent/src/domain/services/engine.rs::classify_probe`
- `crates/hearth-agent/src/domain/services/engine.rs::EngineState::is_inaccessible`
- `crates/hearth-proto/src/api/services.rs::EngineState`

## Vérification
- `crates/hearth-agent/src/domain/services/engine.rs::tests::classify_probe_covers_every_state`
- `crates/hearth-agent/src/domain/services/engine.rs::tests::the_eight_states_are_all_reachable_from_a_probe`
- `crates/hearth-agent/tests/engine_fake_daemon.rs (sonde de chaque état contre un faux démon)`

## Cas limites
- Socket présent mais connexion refusée : classé « arrêté » (un socket resté sans moteur derrière est celui d'un moteur éteint), même sans repère d'installation.
- Un proxy de socket qui répond 401 ou 403 à `/_ping` est « accès refusé ».

## Historique
- 2026-10-13 — création (HRT-61, tranche 1 des déploiements : moteur détecté et liste en lecture seule).
