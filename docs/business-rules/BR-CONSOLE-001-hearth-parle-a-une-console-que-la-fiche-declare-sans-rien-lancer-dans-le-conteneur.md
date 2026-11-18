---
id: BR-CONSOLE-001
domaine: CONSOLE
titre: Hearth ne parle qu'à une console que la fiche déclare et autorise, par l'adresse privée du conteneur, sans rien lancer dans le conteneur
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-11-17-fonctionnelle-console-et-joueurs-minecraft.md
maj: 2026-11-18
---

# BR-CONSOLE-001 : Le canal de la console

## Règle
Hearth parle à la console d'un jeu seulement si la fiche la déclare dans un protocole qu'il sait parler (`source_rcon`) ET déclare l'usage de la console (`[remote_console.use]`, invariants I-CON du chargeur : verbes uniques et jamais à la fois refusés et sensibles, raisons et lecteur de listes fermées, textes sans caractère à risque, littéraux d'envoi de classe libre, 6 commandes rapides au plus, délais bornés). Il s'y connecte à l'adresse IPv4 PRIVÉE du conteneur sur son réseau `bridge` (`ContainerEngine::private_address`, lecture seule, jamais en réseau `host`, jamais une adresse publique), au port conteneur du port de rôle `remote_console`, jamais à un port publié. Il n'exécute aucune commande dans le conteneur (pas d'`exec`, pas de `rcon-cli`), et ne passe aucun texte d'utilisateur par l'entrée standard. Le mot de passe est celui que l'agent a tiré à l'installation et scellé : il passe dans le paquet d'authentification, dans un tampon effacé ensuite, jamais en argument, variable d'environnement, trace, erreur ni journal. Une connexion par échange. Un service adopté n'a pas de console.

## Application (code)
- `crates/hearth-agent/src/domain/catalog/loading/invariants.rs::check_console_use` (I-CON), `loading/schema/fiche.rs::RemoteConsoleUse`
- `crates/hearth-agent/src/application/ports/game_console.rs::GameConsole`, `infrastructure/console/source_rcon.rs::SourceRcon`
- `crates/hearth-agent/src/application/ports/container_engine.rs::ContainerEngine::private_address`, `infrastructure/engine/bollard.rs::private_address_of`

## Vérification
- `domain/catalog/tests_console.rs` (fiche réelle et fiches piégées)
- `tests/console_rcon.rs` (faux serveur de jeu : réponse simple, découpée, vide, mot de passe refusé, longueurs hors bornes, paquets étrangers, flux sans fin, muet, coupure)
- `infrastructure/engine/bollard.rs::private_address_tests`
- `domain/catalog/architecture_guard.rs` (`GameConsole` atteint seulement par le cas d'usage ; pas d'`exec` dans le port du moteur)

## Règles liées
- BR-JEU-016, BR-JEU-040, BR-CONSOLE-007

## Historique
- 2026-11-18 : création (HRT-267).
