---
id: BR-JEU-038
domaine: JEU
titre: Un élément de fiche non essayé (`untested`) n'est jamais exécuté ni cru ; la fiche est livrée quand même
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-serveur-de-jeu-au-quotidien.md (BR-JEU-038), contexts/hearth/conceptions/2026-10-13-technique-fiches-de-jeu.md, HRT-113
maj: 2026-11-14
---

# BR-JEU-038 — Un élément de fiche non essayé (`untested`) n'est jamais exécuté ni cru ; la fiche est livrée quand même

## Règle
Quatre éléments portent un statut de preuve : la séquence d'arrêt, la séquence d'enregistrement, le signe « prêt », la lecture de la version du jeu. Un élément `untested` est traité comme absent : `stop_plan` rend une DÉCISION (« Arrêter quand même » ou « Annuler »), `save_plan`, `ready_sign` et `game_version_reader` rendent `None`, et rien n'est écrit sur l'entrée standard du service. Le conteneur est tout de même créé avec l'entrée OUVERTE quand la fiche déclare des ordres (`declares_orders`) : le jour où la suite est essayée, les services déjà installés peuvent la recevoir ; un service dont l'entrée est fermée donne une décision (BR-JEU-007). La fiche Terraria a été écrite avec `save` et `exit` en `untested` ; le banc d'essai réel (HRT-114, ADR-0053, `tests/fiche_live.rs`, job `fiche-live`) les a envoyés par l'entrée du conteneur, sans terminal, sur la vraie image `ryshe/terraria:vanilla-1.4.5.8` : le jeu a lu `save` (le fichier du monde est réécrit), puis `exit` (le jeu sort de lui-même, code 0), le service est resté arrêté et le monde a survécu au redémarrage. Sa séquence d'arrêt, son enregistrement et son signe « prêt » (`Server started`) sont donc `tested` dans la fiche livrée (version 4). La règle reste vraie pour toute fiche, et tant qu'un élément n'a pas passé son essai il reste `untested`.

## Application (code)
- `crates/hearth-agent/src/domain/catalog/stop_plan.rs::{stop_plan, save_plan, ready_sign, game_version_reader}`
- `crates/hearth-agent/src/domain/catalog/stop_plan.rs::declares_orders` (`stdin_open` de `translate`)
- `crates/hearth-agent/catalog/terraria/fiche.toml` (les trois éléments `tested` depuis HRT-114)
- `crates/hearth-agent/tests/fiche_live.rs` (l'essai réel qui consigne `evidence.toml`)
- `crates/hearth-agent/src/application/game_stop.rs::GameStop::run` (la décision, HRT-103)

## Vérification
- `crates/hearth-agent/tests/fiche_live.rs::the_terraria_fiche_starts_takes_a_setting_stops_by_its_orders_and_keeps_its_world` (job `fiche-live`, vrai moteur, vraie image)
- `crates/hearth-agent/src/domain/catalog/tests_state.rs::a_sequence_that_was_not_tried_is_never_executed_it_asks_for_a_decision`
- `crates/hearth-agent/src/domain/catalog/tests_state.rs::the_other_stop_kinds_and_every_untested_element_are_handled`
- `crates/hearth-agent/src/domain/catalog/tests_translate.rs::terraria_with_the_proposed_values_writes_only_what_is_known_and_pins_the_tried_image`
- `crates/hearth-agent/src/domain/catalog/tests_fiches.rs::the_embedded_terraria_fiche_is_a_preview_with_an_untested_stop_and_a_stable_digest`
- `crates/hearth-agent/tests/game_stop_https.rs::an_untested_sequence_is_never_sent_the_stop_asks_for_a_decision`

## Cas limites
- Les textes d'écran de remplacement sont côté client (specs).

## Règles liées
- BR-JEU-002, BR-FICHE-049

## Historique
- 2026-11-14 — Terraria : séquence d'arrêt, enregistrement et signe « prêt » passent à `tested` après l'essai réel en CI (HRT-114) ; la fiche reste `preview` (pas de relevé `golden`, pas de mémoire recommandée mesurée sous charge).
- 2026-10-24 — l'arrêt d'une suite `untested` est une décision, aucun ordre n'est envoyé (HRT-103).
- 2026-10-13 — création (HRT-113, format de fiche version 2, domaine pur).
