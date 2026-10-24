---
id: BR-JEU-011
domaine: JEU
titre: Une séquence d'arrêt interrompue n'est jamais reprise seule
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-modifier-un-service.md (BR-JEU-011), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 0, 0 bis et 6), HRT-83
maj: 2026-10-24
---

# BR-JEU-011 — Une séquence d'arrêt interrompue n'est jamais reprise seule

## Règle
L'arrêt d'un serveur de jeu qui enregistre son monde est une opération du journal (étapes `save_world`, `stop`, `start`, aucune destructive) : interrompue par un redémarrage de l'agent, elle est classée « interrompue » (« résultat inconnu », verrou rendu) et rien n'est rejoué ni renvoyé au jeu ni au moteur. Le service tourne toujours, sauf si le jeu avait déjà quitté ; le client dit « L'arrêt de {nom} a été interrompu. Le service tourne toujours. » d'après l'état réel.

## Application (code)
- `crates/hearth-agent/src/domain/ops/recover.rs::recover`

## Vérification
- `crates/hearth-agent/src/domain/ops/recover.rs::tests::the_table_of_the_design`
- `crates/hearth-agent/src/domain/ops/game_stop.rs::tests::a_game_stop_cut_by_an_agent_restart_is_interrupted_and_never_replayed`
- `crates/hearth-agent/tests/game_stop_https.rs::an_agent_restart_in_the_middle_of_a_stop_interrupts_it_and_replays_nothing`

## Cas limites
- Un ordre déjà parti (par exemple « exit ») ne se rappelle pas : si le jeu a quitté, le service est arrêté, et l'état réel est relu.

## Règles liées
- BR-EDIT-031, BR-JEU-003

## Historique
- 2026-10-24 — le déroulé de l'arrêt existe, rien n'est rejoué (HRT-103).
- 2026-10-13 — création (HRT-83, ADR-0044 et ADR-0045 : socle des services créés par Hearth, côté agent).
