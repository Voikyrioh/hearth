---
id: BR-JEU-038
domaine: JEU
titre: Un élément de fiche non essayé (`untested`) n'est jamais exécuté ni cru ; la fiche est livrée quand même
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-serveur-de-jeu-au-quotidien.md (BR-JEU-038), contexts/hearth/conceptions/2026-10-13-technique-fiches-de-jeu.md, HRT-113
maj: 2026-10-13
---

# BR-JEU-038 — Un élément de fiche non essayé (`untested`) n'est jamais exécuté ni cru ; la fiche est livrée quand même

## Règle
Quatre éléments portent un statut de preuve : la séquence d'arrêt, la séquence d'enregistrement, le signe « prêt », la lecture de la version du jeu. Un élément `untested` est traité comme absent : `stop_plan` rend une DÉCISION (« Arrêter quand même » ou « Annuler »), `save_plan`, `ready_sign` et `game_version_reader` rendent `None`, et l'entrée standard du service n'est pas ouverte pour lui. La fiche Terraria est écrite avec `save` et `exit` mais sa séquence est `untested` : l'essai sur une copie ne change qu'un mot (`tested`).

## Application (code)
- `crates/hearth-agent/src/domain/catalog/stop_plan.rs::{stop_plan, save_plan, ready_sign, game_version_reader}`
- `crates/hearth-agent/src/domain/catalog/translate.rs::translate (`stdin_open`)`
- `crates/hearth-agent/catalog/terraria/fiche.toml`

## Vérification
- `crates/hearth-agent/src/domain/catalog/tests_state.rs::a_sequence_that_was_not_tried_is_never_executed_it_asks_for_a_decision`
- `crates/hearth-agent/src/domain/catalog/tests_state.rs::the_other_stop_kinds_and_every_untested_element_are_handled`
- `crates/hearth-agent/src/domain/catalog/tests_translate.rs::terraria_with_the_proposed_values_writes_only_what_is_known_and_sends_no_order`
- `crates/hearth-agent/src/domain/catalog/tests_fiches.rs::the_embedded_terraria_fiche_is_a_preview_with_an_untested_stop_and_a_stable_digest`

## Cas limites
- Les textes d'écran de remplacement sont côté client (specs).

## Règles liées
- BR-JEU-002, BR-FICHE-049

## Historique
- 2026-10-13 — création (HRT-113, format de fiche version 2, domaine pur).
