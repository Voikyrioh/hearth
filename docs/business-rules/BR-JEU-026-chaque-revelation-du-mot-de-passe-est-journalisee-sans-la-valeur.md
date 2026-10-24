---
id: BR-JEU-026
domaine: JEU
titre: Chaque révélation du mot de passe est inscrite au journal d'activité avant d'être rendue, sans la valeur ; un refus faute de droit l'est aussi
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-serveur-de-jeu-au-quotidien.md (BR-JEU-026), HRT-103
maj: 2026-10-24
---

# BR-JEU-026 — Chaque révélation du mot de passe est inscrite au journal d'activité avant d'être rendue, sans la valeur ; un refus faute de droit l'est aussi

## Règle
La réussite est écrite sous l'action « Mot de passe de serveur de jeu révélé » (`service.game.password.reveal`), cible = nom du service, AVANT de rendre la valeur, et la conditionne : si l'entrée ne s'écrit pas, rien n'est rendu (`503`-équivalent `BUSY`). Un refus faute du droit est consigné par la couche d'accès sous le même code. La valeur n'entre dans aucune entrée, trace, erreur ni `Debug`.

## Application (code)
- `crates/hearth-agent/src/application/game_settings.rs::GameSettings::reveal_game_password`
- `crates/hearth-agent/src/domain/audit/action.rs::AuditAction::ServiceGamePasswordReveal`

## Vérification
- `crates/hearth-agent/tests/game_stop_https.rs::the_password_is_revealed_to_the_right_only_from_the_description_and_journaled_without_it`
- `crates/hearth-agent/tests/game_settings_use_cases.rs::the_revealed_password_follows_the_saved_one_and_a_journal_that_fails_rules_the_reveal_out`
- `crates/hearth-proto/src/api/game_secret.rs::tests::the_password_travels_as_json_and_never_shows_in_debug`

## Cas limites
- Aucun.

## Règles liées
- BR-JEU-025, BR-SVC-009

## Historique
- 2026-10-24 — création (HRT-103, côté agent).
