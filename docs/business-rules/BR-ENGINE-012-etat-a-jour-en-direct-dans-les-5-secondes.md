---
id: BR-ENGINE-012
domaine: ENGINE
titre: L'état du moteur est à jour en direct, dans les 5 secondes
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-moteur-de-conteneurs.md (BR-ENGINE-012), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §5, §7, §9, HRT-61
maj: 2026-10-13
---

# BR-ENGINE-012 — L'état du moteur est à jour en direct, dans les 5 secondes

## Règle
L'état du moteur est mis à jour sans rechargement, au plus 5 secondes après le changement réel, dans les deux sens, quand aucun changement n'a été publié dans les 5 secondes précédentes ; sinon à la fin de la fenêtre (BR-ENGINE-013). L'agent sonde `/_ping` toutes les 2 secondes quand le moteur est prêt, toutes les 5 secondes sinon, publie le changement sur le sujet `services`, et le moteur qui revient repasse à « prêt » seul. Une rupture du flux d'événements déclenche une sonde immédiate puis une relecture complète.

## Application (code)
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::run`
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::ready_phase`
- `crates/hearth-agent/src/application/services.rs::EngineTimings::production`
- `crates/hearth-agent/src/domain/services/engine.rs::Debounce`

## Vérification
- `crates/hearth-agent/tests/services_https.rs::a_missing_engine_is_absent_quietly_and_a_returning_engine_is_picked_up_alone`
- `crates/hearth-agent/tests/services_https.rs::a_broken_event_stream_triggers_a_probe_and_a_full_reread`
- `crates/hearth-agent/src/domain/services/engine.rs::tests::debounce_publishes_the_first_change_at_once`

## Règles liées
- BR-ENGINE-013

## Historique
- 2026-10-13 — création (HRT-61, tranche 1 des déploiements : moteur détecté et liste en lecture seule).
