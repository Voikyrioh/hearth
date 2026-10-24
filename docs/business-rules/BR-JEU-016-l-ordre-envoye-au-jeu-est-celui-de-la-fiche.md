---
id: BR-JEU-016
domaine: JEU
titre: Le texte envoyé au jeu est celui de la fiche, jamais celui du client ; aucune route ne porte `send_stdin_line`
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-serveur-de-jeu-au-quotidien.md (BR-JEU-016), HRT-103
maj: 2026-10-24
---

# BR-JEU-016 — Le texte envoyé au jeu est celui de la fiche, jamais celui du client ; aucune route ne porte `send_stdin_line`

## Règle
L'ordre d'enregistrement et l'ordre de sortie sont ceux de la fiche (littéral de 64 caractères ASCII imprimables au plus) ; l'utilisateur ne les saisit ni ne les modifie. Hearth les envoie par `ContainerEngine::send_stdin_line`, seule fonction du port qui écrit dans un conteneur, dans le cadre d'une suite qu'un compte qui a le droit a demandée et confirmée. L'adaptateur refuse (`Refused`) tout autre texte sans rien écrire. Aucune route, aucun message du flux ne l'appelle (garde d'architecture). Hearth n'envoie aucun autre texte au jeu. Le journal d'activité nomme l'enregistrement, jamais le texte d'un ordre.

## Application (code)
- `crates/hearth-agent/src/application/ports/container_engine.rs::ContainerEngine::send_stdin_line`
- `crates/hearth-agent/src/infrastructure/engine/bollard.rs::BollardEngine::send_stdin_line`
- `crates/hearth-agent/src/application/game_stop.rs::GameStop::run`

## Vérification
- `crates/hearth-agent/src/domain/catalog/architecture_guard.rs::no_route_carries_the_order_written_to_a_games_input`
- `crates/hearth-agent/src/domain/catalog/architecture_guard.rs::i4_the_engine_port_has_no_function_that_runs_a_command_in_a_container`
- `crates/hearth-agent/src/infrastructure/engine/bollard.rs::tests::only_a_printable_ascii_literal_of_a_fiche_is_written_to_a_container_input`
- `crates/hearth-agent/tests/game_stop_https.rs::no_order_text_and_no_password_reaches_the_activity_journal_or_the_operations`
- `crates/hearth-agent/tests/engine_real.rs::a_line_reaches_the_input_and_an_exit_followed_by_the_engine_stop_leaves_the_service_stopped (job `engine` de la CI)`

## Cas limites
- Aucune commande n'est exécutée dans un conteneur (ADR-0052 point 9).

## Règles liées
- BR-JEU-002, BR-JEU-036

## Historique
- 2026-10-24 — création (HRT-103, côté agent).
