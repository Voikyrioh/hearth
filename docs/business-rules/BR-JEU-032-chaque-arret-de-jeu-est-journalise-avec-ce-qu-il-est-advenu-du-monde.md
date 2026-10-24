---
id: BR-JEU-032
domaine: JEU
titre: Chaque arrêt et redémarrage d'un serveur de jeu est inscrit au journal d'activité avec la mention de l'enregistrement
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-serveur-de-jeu-au-quotidien.md (BR-JEU-032), HRT-103
maj: 2026-10-24
---

# BR-JEU-032 — Chaque arrêt et redémarrage d'un serveur de jeu est inscrit au journal d'activité avec la mention de l'enregistrement

## Règle
L'entrée de l'arrêt ou du redémarrage (BR-LIFE-018) ajoute à la cible, en texte fixe : « monde enregistré » (signe vu), « enregistrement non confirmé » (ordres partis, rien ne le prouve), « enregistrement impossible » (ordre impossible, puis « Arrêter quand même »), « non nécessaire (le jeu enregistre lui-même) », « arrêt malgré tout » (signe obligatoire non vu, puis « Arrêter quand même »), et « arrêt forcé » s'il l'a été. Le choix du compte (« poursuivre », « annuler ») est inscrit par la route de décision (BR-OPS-003). Jamais le texte d'un ordre.

## Application (code)
- `crates/hearth-agent/src/domain/audit/event.rs::{LifecycleNote, SaveMention}`
- `crates/hearth-agent/src/application/service_lifecycle.rs::ServiceLifecycle::draft`

## Vérification
- `crates/hearth-agent/src/domain/audit/event.rs::tests::a_game_lifecycle_entry_says_what_became_of_the_world_save_and_never_an_order`
- `crates/hearth-agent/tests/game_stop_https.rs::terraria_saves_waits_exits_then_the_engine_is_asked_to_stop_right_away`
- `crates/hearth-agent/tests/game_stop_https.rs::a_sign_that_does_not_come_stops_nothing_and_asks_which_way_to_go`

## Cas limites
- Une décision « Annuler » ou expirée n'écrit pas d'entrée d'arrêt : le service n'a pas été arrêté.

## Règles liées
- BR-JEU-006, BR-JEU-016, BR-LIFE-018

## Historique
- 2026-10-24 — création (HRT-103, côté agent).
