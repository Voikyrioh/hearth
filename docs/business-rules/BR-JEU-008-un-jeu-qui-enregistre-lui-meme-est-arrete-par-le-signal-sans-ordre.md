---
id: BR-JEU-008
domaine: JEU
titre: Un jeu dont l'image enregistre sur le signal d'arrêt est arrêté avec le délai du service, sans aucun ordre
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-serveur-de-jeu-au-quotidien.md (BR-JEU-008), HRT-103
maj: 2026-10-24
---

# BR-JEU-008 — Un jeu dont l'image enregistre sur le signal d'arrêt est arrêté avec le délai du service, sans aucun ordre

## Règle
Pour un jeu dont la fiche dit `by_image_on_signal` (7 Days to Die), l'agent n'envoie aucun ordre : il demande l'arrêt au moteur avec le délai du service (120 secondes pour 7 Days to Die), et annonce un arrêt forcé à l'issue du délai (BR-LIFE-009). La mention du journal d'activité est « non nécessaire (le jeu enregistre lui-même) ».

## Application (code)
- `crates/hearth-agent/src/domain/ops/game_stop.rs::GameStopKind::of`
- `crates/hearth-agent/src/application/service_lifecycle.rs::ServiceLifecycle::execute`

## Vérification
- `crates/hearth-agent/tests/game_stop_https.rs::a_game_that_saves_by_its_image_is_stopped_with_the_signal_and_no_order`
- `crates/hearth-agent/src/domain/ops/game_stop.rs::tests::a_plan_that_saves_by_the_image_is_self_saving_and_everything_else_is_unrolled`

## Cas limites
- Le signal choisi par la fiche (`SIGTERM`, `SIGINT`) n'est pas posé à la création du conteneur : le plan fermé du conteneur n'a pas encore ce champ, l'arrêt utilise le signal de l'image.

## Règles liées
- BR-JEU-002, BR-JEU-034, BR-LIFE-009

## Historique
- 2026-10-24 — création (HRT-103, côté agent).
