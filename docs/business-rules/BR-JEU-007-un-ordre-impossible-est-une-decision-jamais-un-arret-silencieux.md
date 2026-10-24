---
id: BR-JEU-007
domaine: JEU
titre: Un ordre qui ne peut pas être envoyé est une décision « Arrêter quand même » ou « Annuler » ; l'arrêt n'est jamais refusé
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-serveur-de-jeu-au-quotidien.md (BR-JEU-007), HRT-103
maj: 2026-10-24
---

# BR-JEU-007 — Un ordre qui ne peut pas être envoyé est une décision « Arrêter quand même » ou « Annuler » ; l'arrêt n'est jamais refusé

## Règle
Si l'ordre ne peut pas être envoyé du tout (entrée du service non ouverte, service qui ne répond pas, attache refusée), ou si la fiche déclare une suite non essayée (BR-JEU-038), rien n'est envoyé ni arrêté de soi-même : l'opération passe à « décision attendue » avec les deux choix de BR-JEU-006. « Arrêter quand même » demande l'arrêt au moteur avec le délai du service SANS retenter l'ordre de sortie (le canal a échoué, le compte a choisi l'arrêt du moteur) ; la mention du journal d'activité est « enregistrement impossible ». Cet état ne bloque jamais l'arrêt : Hearth ne refuse jamais d'arrêter un service (BR-LIFE-008).

## Application (code)
- `crates/hearth-agent/src/application/game_stop.rs::GameStop::run`
- `crates/hearth-agent/src/domain/ops/game_stop.rs::AskReason`

## Vérification
- `crates/hearth-agent/tests/game_stop_https.rs::a_server_installed_with_a_closed_input_gets_a_decision_never_an_order_sent_to_nowhere`
- `crates/hearth-agent/tests/game_stop_https.rs::an_order_the_game_does_not_accept_is_a_decision_and_nothing_is_stopped`
- `crates/hearth-agent/tests/game_stop_https.rs::an_untested_sequence_is_never_sent_the_stop_asks_for_a_decision`

## Cas limites
- Le texte des fenêtres (« Hearth ne peut pas demander à {jeu} d'enregistrer son monde. ») est côté client (HRT-104).

## Règles liées
- BR-JEU-001, BR-JEU-006, BR-JEU-038, BR-LIFE-008

## Historique
- 2026-10-24 — création (HRT-103, côté agent).
