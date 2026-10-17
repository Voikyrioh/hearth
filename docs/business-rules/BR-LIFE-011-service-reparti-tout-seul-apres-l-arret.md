---
id: BR-LIFE-011
domaine: LIFE
titre: Un service arrêté depuis Hearth qui repart tout seul dans les 30 secondes est signalé : l'arrêt n'est pas déclaré réussi sans cette précision
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-cycle-de-vie-service.md (BR-LIFE-011), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md, HRT-64
maj: 2026-10-17
---

# BR-LIFE-011 — Un service arrêté depuis Hearth qui repart tout seul dans les 30 secondes est signalé : l'arrêt n'est pas déclaré réussi sans cette précision

## Règle
Une fois l'arrêt terminé, une surveillance de 30 secondes relit le service. S'il tourne de nouveau (ou est relancé), la cause `restarted_by_host` est posée après coup sur l'opération et l'entrée « repart tout seul » est écrite au journal d'activité. Hearth ne fait rien de plus : il ne ré-arrête pas, il ne désactive aucune règle du serveur. La surveillance s'arrête dès qu'un autre geste de Hearth reprend le service.

## Application (code)
- `crates/hearth-agent/src/application/service_lifecycle.rs::ServiceLifecycle::{watch, still_last}`
- `crates/hearth-agent/src/domain/ops/lifecycle.rs::{RESTART_WATCH, AfterStop, cause_after_stop}`

## Vérification
- `crates/hearth-agent/tests/service_lifecycle_https.rs::a_service_that_comes_back_alone_after_a_stop_is_told_and_one_that_stays_stopped_is_not`
- `crates/hearth-agent/src/domain/ops/lifecycle.rs::tests::a_service_that_comes_back_is_told_and_one_that_stays_stopped_says_nothing`

## Cas limites
- Sur NixOS, arrêter un conteneur `--rm` par l'API du moteur le fait relancer par l'unité : c'est pourquoi un service supervisé est piloté par son unité (BR-LIFE-002).

## Règles liées
- BR-LIFE-002, BR-LIFE-010

## Historique
- 2026-10-17 — création (HRT-64 : démarrer, arrêter, redémarrer un service, côté agent).
