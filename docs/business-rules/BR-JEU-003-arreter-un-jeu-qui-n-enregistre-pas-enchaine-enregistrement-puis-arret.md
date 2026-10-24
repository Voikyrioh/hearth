---
id: BR-JEU-003
domaine: JEU
titre: Arrêter ou redémarrer un serveur de jeu qui n'enregistre pas en s'arrêtant enchaîne l'enregistrement, puis l'arrêt, sous le verrou du service
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-serveur-de-jeu-au-quotidien.md (BR-JEU-003), HRT-103
maj: 2026-10-24
---

# BR-JEU-003 — Arrêter ou redémarrer un serveur de jeu qui n'enregistre pas en s'arrêtant enchaîne l'enregistrement, puis l'arrêt, sous le verrou du service

## Règle
« Arrêter » et « Redémarrer » un serveur de jeu en marche dont la fiche déclare une suite essayée déroulent cette suite (étape `save_world`), puis l'arrêt (étape `stop`), puis pour un redémarrage le démarrage (étape `start`). L'arrêt n'est demandé qu'après la confirmation, ou après le choix du compte (BR-JEU-006). Le verrou du service est tenu du début à la fin (BR-LIFE-013). Quand la suite comporte un ordre de sortie, l'arrêt du moteur est demandé aussitôt après lui (BR-JEU-034). L'attente d'une étape ne bloque pas l'agent : une annulation du compte entre deux étapes arrête la suite avant l'étape suivante.

## Application (code)
- `crates/hearth-agent/src/application/game_stop.rs::GameStop::run`
- `crates/hearth-agent/src/application/service_lifecycle.rs::ServiceLifecycle::run_game`
- `crates/hearth-agent/src/domain/ops/game_stop.rs::op_plan`

## Vérification
- `crates/hearth-agent/tests/game_stop_https.rs::terraria_saves_waits_exits_then_the_engine_is_asked_to_stop_right_away`
- `crates/hearth-agent/tests/game_stop_https.rs::a_restart_follows_the_same_sequence_then_starts_the_container_again`
- `crates/hearth-agent/tests/game_stop_https.rs::cancelling_during_the_wait_stops_the_sequence_before_the_exit_order`
- `crates/hearth-agent/src/domain/ops/game_stop.rs::tests::the_plan_of_the_operation_follows_the_sequence_and_destroys_nothing`

## Cas limites
- Les étapes ne sont pas destructives : une opération interrompue par un redémarrage de l'agent est classée « interrompue », jamais reprise (BR-JEU-011).
- Une annulation du compte après l'ordre de sortie ne défait rien : l'arrêt du moteur suit aussitôt.

## Règles liées
- BR-JEU-006, BR-JEU-011, BR-JEU-034, BR-LIFE-013

## Historique
- 2026-10-24 — création (HRT-103, côté agent).
