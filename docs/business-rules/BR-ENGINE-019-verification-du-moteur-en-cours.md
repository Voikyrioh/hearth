---
id: BR-ENGINE-019
domaine: ENGINE
titre: Première détection : « Vérification du moteur… » (`checking`)
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-moteur-de-conteneurs.md (BR-ENGINE-019), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §5, §7, §9, HRT-61
maj: 2026-10-13
---

# BR-ENGINE-019 — Première détection : « Vérification du moteur… » (`checking`)

## Règle
Pendant la première détection (ouverture de la section, ou redémarrage de l'agent), l'état est « Vérification du moteur… ». Ce n'est pas un des huit états : `checking` ne dure que le temps de la sonde (au plus 5 secondes), puis un des huit le remplace. L'agent part de `Checking` et publie le premier état réel tout de suite.

## Application (code)
- `crates/hearth-agent/src/domain/services/engine.rs::EngineState::Checking`
- `crates/hearth-agent/src/domain/services/engine.rs::classify_probe (sonde absente et pilotage permis)`
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::new`

## Vérification
- `crates/hearth-agent/src/application/services.rs::tests::the_first_state_is_checking_and_the_first_probe_publishes_at_once`

## Historique
- 2026-10-13 — création (HRT-61, tranche 1 des déploiements : moteur détecté et liste en lecture seule).
