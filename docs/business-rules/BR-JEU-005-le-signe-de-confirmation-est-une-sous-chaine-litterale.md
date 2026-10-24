---
id: BR-JEU-005
domaine: JEU
titre: Le signe de confirmation est une sous-chaîne littérale cherchée dans les seules lignes du jeu postérieures à l'ordre
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-serveur-de-jeu-au-quotidien.md (BR-JEU-005), HRT-103
maj: 2026-10-24
---

# BR-JEU-005 — Le signe de confirmation est une sous-chaîne littérale cherchée dans les seules lignes du jeu postérieures à l'ordre

## Règle
Si la fiche déclare un signe (étape `wait_log`), l'agent le cherche comme sous-chaîne LITTÉRALE, jamais comme expression régulière, dans les seules lignes des journaux du jeu écrites à partir de l'envoi du dernier ordre, au plus le délai de l'étape (de 1 à 300 secondes de fiche). Vu, le monde est « enregistré » (mention du journal d'activité). Sans signe déclaré, l'agent attend le délai (étape `wait`) et ne prétend pas savoir : mention « enregistrement non confirmé ». Un signe non obligatoire qui n'est pas vu laisse la suite continuer ; un signe obligatoire qui n'est pas vu mène à BR-JEU-006.

## Application (code)
- `crates/hearth-agent/src/domain/ops/game_stop.rs::confirms`
- `crates/hearth-agent/src/application/game_stop.rs::GameStop::wait_for_sign`

## Vérification
- `crates/hearth-agent/tests/game_stop_https.rs::cancelling_while_the_sign_is_awaited_closes_the_operation_at_once`
- `crates/hearth-agent/src/domain/ops/game_stop.rs::tests::the_sign_is_a_literal_in_a_line_written_since_the_order_never_an_expression`
- `crates/hearth-agent/tests/game_stop_https.rs::the_sign_in_the_games_logs_makes_the_world_saved_before_the_exit_order`
- `crates/hearth-agent/tests/game_stop_https.rs::a_sign_that_does_not_come_stops_nothing_and_asks_which_way_to_go`

## Cas limites
- Le moteur ne filtre les lignes qu'à la seconde : l'agent écarte le surplus à la milliseconde (`confirms`).
- Une ligne de journal est du contenu sensible : jamais tracée ni gardée.

## Règles liées
- BR-JEU-002, BR-JEU-006

## Historique
- 2026-10-24 — création (HRT-103, côté agent).
