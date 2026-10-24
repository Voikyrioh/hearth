---
id: BR-JEU-027
domaine: JEU
titre: Un serveur de jeu sans mot de passe le dit : à l'installation (avertissement sans blocage) et sur sa carte
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-serveur-de-jeu-au-quotidien.md (BR-JEU-027), HRT-102, HRT-103
maj: 2026-10-24
---

# BR-JEU-027 — Un serveur de jeu sans mot de passe le dit

## Règle
Une seule règle de la spec (BR-JEU-027), deux faces. **À l'installation (client, HRT-102)** : quand une question de genre secret et de nature « mot de passe du jeu » est laissée vide, un encart prévient « Aucun mot de passe : toute personne qui peut joindre ce port peut rejoindre la partie. » ; rien n'est bloqué, y compris avec « Installer avec les valeurs proposées ». **Ensuite (agent, HRT-103)** : Quand aucun mot de passe n'est posé (jamais saisi, retiré, ou fiche sans réglage de mot de passe de jeu), `GET /services/{key}/game-secret` répond sans le champ `password` : le client écrit « Aucun mot de passe : toute personne qui peut joindre ce port peut rejoindre la partie. ». La révélation est tout de même journalisée (BR-JEU-026).

## Application (code)
- `apps/desktop/src/components/molecules/QuestionField.vue` (HRT-102)
- `crates/hearth-agent/src/application/game_settings.rs::GameSettings::reveal_game_password`
- `crates/hearth-proto/src/api/game_secret.rs::GameSecret`

## Vérification
- `apps/desktop/src/pages/InstallFromFiche.test.ts::avertit d'un mot de passe laissé vide sans bloquer` (HRT-102)
- `crates/hearth-agent/tests/game_stop_https.rs::a_fiche_without_a_game_password_answers_200_without_one`
- `crates/hearth-agent/tests/game_settings_use_cases.rs::the_revealed_password_follows_the_saved_one_and_a_journal_that_fails_rules_the_reveal_out`

## Cas limites
- La phrase de la carte est écrite par le client (HRT-104).
- Le mot de passe n'est jamais écrit dans le journal d'activité ni affiché ailleurs que dans son champ masqué.

## Règles liées
- BR-JEU-025, BR-JEU-026, BR-FICHE-034, BR-ACCT-006

## Historique
- 2026-10-24 — fiche unique : celle de HRT-102 (avertissement à l'installation) est fusionnée ici, la spec n'ayant qu'un BR-JEU-027.
- 2026-10-24 — création (HRT-103, côté agent).
