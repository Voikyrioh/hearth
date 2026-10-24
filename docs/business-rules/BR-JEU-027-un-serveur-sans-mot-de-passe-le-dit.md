---
id: BR-JEU-027
domaine: JEU
titre: Un serveur de jeu sans mot de passe répond sans mot de passe ; l'écran le dit
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-serveur-de-jeu-au-quotidien.md (BR-JEU-027), HRT-103
maj: 2026-10-24
---

# BR-JEU-027 — Un serveur de jeu sans mot de passe répond sans mot de passe ; l'écran le dit

## Règle
Quand aucun mot de passe n'est posé (jamais saisi, retiré, ou fiche sans réglage de mot de passe de jeu), `GET /services/{key}/game-secret` répond sans le champ `password` : le client écrit « Aucun mot de passe : toute personne qui peut joindre ce port peut rejoindre la partie. ». La révélation est tout de même journalisée (BR-JEU-026).

## Application (code)
- `crates/hearth-agent/src/application/game_settings.rs::GameSettings::reveal_game_password`
- `crates/hearth-proto/src/api/game_secret.rs::GameSecret`

## Vérification
- `crates/hearth-agent/tests/game_stop_https.rs::a_fiche_without_a_game_password_answers_200_without_one`
- `crates/hearth-agent/tests/game_settings_use_cases.rs::the_revealed_password_follows_the_saved_one_and_a_journal_that_fails_rules_the_reveal_out`

## Cas limites
- La phrase est côté client (HRT-104).

## Règles liées
- BR-JEU-025, BR-JEU-026

## Historique
- 2026-10-24 — création (HRT-103, côté agent).
