---
id: BR-LIFE-005
domaine: LIFE
titre: « En marche » ne veut pas dire prêt : Hearth surveille dix secondes un service qu'il vient de lancer et dit s'il s'arrête aussitôt
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-cycle-de-vie-service.md (BR-LIFE-005), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md, HRT-64
maj: 2026-10-17
---

# BR-LIFE-005 — « En marche » ne veut pas dire prêt : Hearth surveille dix secondes un service qu'il vient de lancer et dit s'il s'arrête aussitôt

## Règle
Une fois l'action terminée (le verrou est rendu, aucune autre action n'est bloquée), une surveillance de 10 secondes relit le service. S'il s'arrête dans ce délai (ou si le moteur le relance), la cause `stopped_right_after_start` est posée APRÈS COUP sur l'opération déjà terminée en succès (`OpJournal::annotate_cause`, une cause n'en remplace jamais une autre) et une entrée « arrêté juste après le démarrage » est écrite au journal d'activité. Le code de sortie est dans le service (`exit_code`). La surveillance s'arrête dès qu'un autre geste de Hearth reprend le service. « Redémarre en boucle » vient du compteur de démarrages (BR-SVC-006), pas de cette surveillance.

## Application (code)
- `crates/hearth-agent/src/application/service_lifecycle.rs::ServiceLifecycle::{watch, watch_later}`
- `crates/hearth-agent/src/domain/ops/lifecycle.rs::{STARTUP_WATCH, cause_of_early_exit}`
- `crates/hearth-agent/src/infrastructure/sqlite/ops_repo.rs::SqliteOpJournal::annotate_cause`

## Vérification
- `crates/hearth-agent/tests/service_lifecycle_https.rs::a_service_that_stops_right_after_its_start_is_noted_on_the_finished_action`
- `crates/hearth-agent/tests/service_ops_journal.rs::forced_is_noted_while_running_and_a_cause_is_added_once_to_a_success_only`

## Cas limites
- Les durées sont celles de `LifecycleTimings` (10 s et 30 s en production, raccourcies en test).

## Règles liées
- BR-LIFE-004, BR-LIFE-011, BR-SVC-006

## Historique
- 2026-10-17 — création (HRT-64 : démarrer, arrêter, redémarrer un service, côté agent).
