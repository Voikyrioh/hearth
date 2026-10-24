---
id: BR-JEU-034
domaine: JEU
titre: L'arrêt avec enregistrement, sans console : Terraria par « save » puis « exit », 7 Days to Die par le signal ; l'arrêt du moteur suit aussitôt l'ordre de sortie
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-serveur-de-jeu-au-quotidien.md (BR-JEU-034), HRT-103
maj: 2026-10-24
---

# BR-JEU-034 — L'arrêt avec enregistrement, sans console : Terraria par « save » puis « exit », 7 Days to Die par le signal ; l'arrêt du moteur suit aussitôt l'ordre de sortie

## Règle
Quand la suite envoie un ordre de sortie, l'agent demande l'arrêt du service au moteur AUSSITÔT après cet ordre, avec le délai du service : sans cela le programme qui sort de lui-même est relancé par la règle de redémarrage (BR-DEPLOY-021). L'arrêt demandé au moteur revient dès que le programme sort. **Terraria** : « save », une pause de quinze secondes (le signe n'est pas connu), « exit », arrêt du moteur ; la suite ne s'exécute que si la fiche la déclare `tested` (BR-JEU-038). **7 Days to Die** : aucun ordre, arrêt par le signal avec 120 secondes (BR-JEU-008). **Service adopté** : aucun ordre (BR-JEU-001). Après « Arrêter quand même » sur un signe non vu, l'ordre de sortie de la fiche est tout de même envoyé (le canal fonctionne, c'est le signe qui manquait).

## Application (code)
- `crates/hearth-agent/src/application/game_stop.rs::GameStop::run`
- `crates/hearth-agent/src/domain/ops/game_stop.rs::{quit_line, before_quit}`

## Vérification
- `crates/hearth-agent/tests/game_stop_https.rs::terraria_saves_waits_exits_then_the_engine_is_asked_to_stop_right_away`
- `crates/hearth-agent/tests/game_stop_https.rs::a_sign_that_does_not_come_stops_nothing_and_asks_which_way_to_go`
- `crates/hearth-agent/tests/game_stop_https.rs::a_game_that_saves_by_its_image_is_stopped_with_the_signal_and_no_order`
- `crates/hearth-agent/src/domain/ops/game_stop.rs::tests::the_sequence_is_cut_at_the_exit_order_which_comes_last`

## Cas limites
- Si le jeu met plus que le délai à enregistrer après l'ordre de sortie, le moteur le coupe de force et l'arrêt forcé est annoncé : seule la fiche fixe ce délai.
- Un échec de l'envoi de l'ordre de sortie lui-même rend l'arrêt voulu et mène à BR-JEU-007.

## Règles liées
- BR-JEU-002, BR-JEU-007, BR-JEU-008, BR-JEU-038

## Historique
- 2026-10-24 — création (HRT-103, côté agent).
