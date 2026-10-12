---
id: BR-ENGINE-009
domaine: ENGINE
titre: « Ne répond pas » après 5 secondes, dernière liste conservée
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-moteur-de-conteneurs.md (BR-ENGINE-009), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §5, §7, §9, HRT-61
maj: 2026-10-13
---

# BR-ENGINE-009 — « Ne répond pas » après 5 secondes, dernière liste conservée

## Règle
Le moteur qui n'a pas répondu dans les 5 secondes est « ne répond pas ». L'agent borne la sonde à 5 secondes (connexion, `/_ping`, `/version`). Tant qu'il ne répond pas, la dernière liste de services reste dans le registre (famille inaccessible) ; le client la grise et la date.

## Application (code)
- `crates/hearth-agent/src/infrastructure/engine/bollard.rs::PROBE_TIMEOUT`
- `crates/hearth-agent/src/infrastructure/engine/bollard.rs::BollardEngine::probe`
- `crates/hearth-agent/src/domain/services/engine.rs::EngineState::is_inaccessible`
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::commit`

## Vérification
- `crates/hearth-agent/tests/engine_fake_daemon.rs::a_daemon_that_never_answers_is_unresponsive_after_the_probe_delay`
- `crates/hearth-agent/src/application/services.rs::tests::an_inaccessible_engine_keeps_the_last_list_and_a_missing_one_drops_it`

## Historique
- 2026-10-13 — création (HRT-61, tranche 1 des déploiements : moteur détecté et liste en lecture seule).
