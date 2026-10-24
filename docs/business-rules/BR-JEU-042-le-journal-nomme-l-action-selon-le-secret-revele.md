---
id: BR-JEU-042
domaine: JEU
titre: Le journal d'activité nomme l'action selon le secret révélé, jamais la valeur
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-serveur-de-jeu-au-quotidien.md (BR-JEU-042), HRT-103
maj: 2026-10-24
---

# BR-JEU-042 — Le journal d'activité nomme l'action selon le secret révélé, jamais la valeur

## Règle
Le journal d'activité nomme l'action selon le secret : « Mot de passe de serveur de jeu révélé », « Mot de passe d'administration révélé », « Jeton de serveur révélé » (BR-JEU-026). Jamais la valeur. Cet agent n'en sert qu'un, le mot de passe de serveur de jeu (`service.game.password.reveal`) ; les deux autres libellés viendront avec leurs routes.

## Application (code)
- `crates/hearth-agent/src/domain/audit/action.rs::AuditAction::ServiceGamePasswordReveal` (libellé)
- `crates/hearth-agent/src/application/game_settings.rs::GameSettings::reveal_game_password`

## Vérification
- `crates/hearth-agent/tests/game_stop_https.rs::the_password_is_revealed_to_the_right_only_from_the_description_and_journaled_without_it`

## Cas limites
- Les libellés « Mot de passe d'administration révélé » et « Jeton de serveur révélé » n'existent pas encore côté agent.

## Règles liées
- BR-JEU-025, BR-JEU-026

## Historique
- 2026-10-24 — création (HRT-103), libellé du mot de passe de serveur de jeu.
