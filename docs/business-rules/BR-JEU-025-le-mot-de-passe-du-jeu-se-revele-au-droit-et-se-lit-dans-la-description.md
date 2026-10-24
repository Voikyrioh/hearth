---
id: BR-JEU-025
domaine: JEU
titre: Le mot de passe du serveur de jeu ne se révèle qu'aux comptes qui ont le droit de gérer les services, lu dans la description chiffrée
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-serveur-de-jeu-au-quotidien.md (BR-JEU-025), HRT-103
maj: 2026-10-24
---

# BR-JEU-025 — Le mot de passe du serveur de jeu ne se révèle qu'aux comptes qui ont le droit de gérer les services, lu dans la description chiffrée

## Règle
`GET /services/{key}/game-secret` (niveau `Services`) rend le mot de passe saisi du serveur de jeu, quel que soit le rôle du compte (BR-GRANT-004), sans cache. Il est descellé de la description durable du service (la valeur la plus récente : un changement de réglage prime sur la réponse de l'installation) ; ni le moteur ni le fichier du jeu ne sont lus. Jamais un secret généré par Hearth ni imposé par la fiche (BR-JEU-039, BR-JEU-040) : seul le réglage de genre `game_password` non imposé est désigné. Un service sans description (adopté, inconnu) répond comme un service inconnu (BR-JEU-001).

## Application (code)
- `crates/hearth-agent/src/application/game_settings.rs::GameSettings::reveal_game_password`
- `crates/hearth-agent/src/entrypoint/http/game_settings.rs::game_secret`

## Vérification
- `crates/hearth-agent/tests/game_stop_https.rs::the_password_is_revealed_to_the_right_only_from_the_description_and_journaled_without_it`
- `crates/hearth-agent/tests/game_settings_use_cases.rs::the_revealed_password_follows_the_saved_one_and_a_journal_that_fails_rules_the_reveal_out`

## Cas limites
- Les autres secrets saisis (mot de passe d'administration, jeton de serveur, BR-JEU-042) ne sont pas servis par cette route.

## Règles liées
- BR-JEU-026, BR-JEU-027, BR-SVC-009, BR-GRANT-004

## Historique
- 2026-10-24 — création (HRT-103, côté agent).
