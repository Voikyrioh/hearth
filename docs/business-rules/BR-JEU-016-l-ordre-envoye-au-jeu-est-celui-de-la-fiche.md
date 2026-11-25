---
id: BR-JEU-016
domaine: JEU
titre: Le texte envoyé au jeu est celui de la fiche, jamais celui du client ; aucune route ne porte `send_stdin_line`
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-serveur-de-jeu-au-quotidien.md (BR-JEU-016), HRT-103
maj: 2026-11-18
---

# BR-JEU-016 — Le texte envoyé au jeu est celui de la fiche, jamais celui du client ; aucune route ne porte `send_stdin_line`

## Règle
L'ordre d'enregistrement et l'ordre de sortie sont ceux de la fiche (littéral de 64 caractères ASCII imprimables au plus) ; l'utilisateur ne les saisit ni ne les modifie. Hearth les envoie par `ContainerEngine::send_stdin_line`, seule fonction du port qui écrit dans un conteneur, dans le cadre d'une suite qu'un compte qui a le droit a demandée et confirmée. Le port ne prend qu'un `FicheLiteral`, type qui ne se construit que dans `domain/catalog` depuis une fiche acceptée : aucun autre texte ne peut l'atteindre. L'adaptateur inspecte le conteneur et refuse (`Refused`) s'il ne tourne pas ou si son entrée n'est pas ouverte. Aucune route, aucun message du flux ne l'appelle (garde d'architecture). Hearth n'envoie aucun autre texte au jeu PAR L'ENTRÉE STANDARD. Le journal d'activité nomme l'enregistrement, jamais le texte d'un ordre.

**Amendement (HRT-267, HRT-268, ADR-0067)** : un compte qui a le droit de gérer les services peut écrire une commande dans la console du jeu quand la fiche l'y autorise (`[remote_console.use]`). Ce texte-là ne passe JAMAIS par `send_stdin_line` : il passe par le port `GameConsole` (RCON vers l'adresse privée du conteneur), seulement depuis `application/console.rs`, sous l'acte `ConsoleCommand` `0x30` et la politique de la fiche (BR-CONSOLE-003, 004, 005). L'invariant devient : aucune route ni message du flux ne porte `send_stdin_line`, et aucun texte saisi n'atteint un jeu autrement que par la console.

## Application (code)
- `crates/hearth-agent/src/domain/catalog/stop_plan.rs::FicheLiteral`
- `crates/hearth-agent/src/application/ports/container_engine.rs::ContainerEngine::send_stdin_line`
- `crates/hearth-agent/src/infrastructure/engine/bollard.rs::BollardEngine::send_stdin_line`
- `crates/hearth-agent/src/application/game_stop.rs::GameStop::run`

## Vérification
- `crates/hearth-agent/src/domain/catalog/architecture_guard.rs::no_route_carries_the_order_written_to_a_games_input`
- `crates/hearth-agent/src/domain/catalog/architecture_guard.rs::i4_the_engine_port_has_no_function_that_runs_a_command_in_a_container`
- `crates/hearth-agent/src/domain/catalog/tests_state.rs::an_order_is_a_printable_ascii_literal_of_a_fiche_and_nothing_else`
- `crates/hearth-agent/tests/game_stop_https.rs::no_order_text_and_no_password_reaches_the_activity_journal_or_the_operations`
- `crates/hearth-agent/src/domain/catalog/architecture_guard.rs::the_game_console_is_reached_only_from_the_console_use_case_the_adapter_and_the_composition_root`
- `crates/hearth-agent/tests/console_https.rs` (le texte saisi passe par la console, jamais par l'entrée standard)
- `crates/hearth-agent/tests/engine_real.rs::a_line_reaches_the_input_and_an_exit_followed_by_the_engine_stop_leaves_the_service_stopped (job `engine` de la CI)`

## Cas limites
- Aucune commande n'est exécutée dans un conteneur (ADR-0052 point 9).

## Règles liées
- BR-JEU-002, BR-JEU-036, BR-JEU-040, BR-CONSOLE-001, BR-CONSOLE-003

## Historique
- 2026-11-18 : amendée (HRT-267, HRT-268) : la console distante est un second canal, séparé, pour un texte saisi (ADR-0067) ; l'entrée standard reste réservée aux littéraux de fiche.
- 2026-10-24 — création (HRT-103, côté agent).
