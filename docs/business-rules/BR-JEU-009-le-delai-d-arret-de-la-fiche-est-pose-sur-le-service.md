---
id: BR-JEU-009
domaine: JEU
titre: Le délai d'arrêt de la fiche est posé sur le service à sa création : tout arrêt demandé au moteur le respecte
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-serveur-de-jeu-au-quotidien.md (BR-JEU-009), HRT-103
maj: 2026-10-24
---

# BR-JEU-009 — Le délai d'arrêt de la fiche est posé sur le service à sa création : tout arrêt demandé au moteur le respecte

## Règle
Le délai d'arrêt de la fiche (de 10 à 600 secondes) est celui du conteneur dès sa création (`stop_grace_s` du plan), de sorte que tout arrêt demandé au moteur, y compris hors de Hearth, le respecte. L'arrêt d'un jeu par Hearth utilise le délai DU SERVICE, jamais un autre. Il n'est pas réglable depuis Hearth.

## Application (code)
- `crates/hearth-agent/src/domain/services/from_fiche.rs`
- `crates/hearth-agent/src/application/service_lifecycle.rs::ServiceLifecycle::run_game`

## Vérification
- `crates/hearth-agent/tests/game_stop_https.rs::terraria_saves_waits_exits_then_the_engine_is_asked_to_stop_right_away`
- `crates/hearth-agent/tests/game_stop_https.rs::a_game_that_saves_by_its_image_is_stopped_with_the_signal_and_no_order`

## Cas limites
- Le délai ne se rallonge jamais tout seul.

## Règles liées
- BR-JEU-002, BR-JEU-034

## Historique
- 2026-10-24 — création (HRT-103, côté agent).
