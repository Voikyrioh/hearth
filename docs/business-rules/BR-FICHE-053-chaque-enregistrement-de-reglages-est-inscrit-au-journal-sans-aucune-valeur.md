---
id: BR-FICHE-053
domaine: FICHE
titre: Chaque enregistrement de réglages est inscrit au journal d'activité : les noms des réglages changés qui ne sont pas des secrets, jamais une valeur
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-053), HRT-108
maj: 2026-10-24
---

# BR-FICHE-053 — Le journal nomme les réglages changés, jamais une valeur

## Règle
Un enregistrement qui change quelque chose s'inscrit sous l'action `service.game.settings` (« Réglages d'un jeu ») : le service, le NOMBRE de réglages changés, les noms techniques de ceux qui ne sont pas des secrets, et « mot de passe modifié » quand un secret a été posé. Le type de l'entrée (`Target::GameSettings`) n'a aucun champ pour une valeur. Un enregistrement qui ne change aucun octet ni aucun réglage n'écrit rien et ne s'inscrit pas. Un refus faute de droit est inscrit par la couche d'accès comme tout refus, un échec sous la même action. Le fichier brut retouché à la main (`Target::GameSettings` avec `raw_file`) est « Fichier de réglages modifié à la main » ; la route de l'onglet Fichiers qui l'écrit n'est pas livrée avec ce ticket.

## Application (code)
- `crates/hearth-agent/src/domain/audit/event.rs::Target::GameSettings`
- `crates/hearth-agent/src/application/game_settings.rs::GameSettings::record`

## Vérification
- `crates/hearth-agent/src/domain/audit/event.rs::tests::a_game_settings_entry_names_the_changed_settings_and_never_a_value`
- `crates/hearth-agent/tests/game_settings_https.rs::the_journal_names_the_changed_settings_that_are_not_secrets_and_never_a_value`
- `crates/hearth-agent/tests/game_settings_https.rs::no_secret_value_is_readable_in_the_database_the_views_or_the_error_bodies`
- `crates/hearth-agent/tests/game_settings_https.rs::a_save_that_changes_no_byte_writes_nothing_and_leaves_no_journal_entry`

## Cas limites
- Les noms des réglages INCONNUS changés (« Autres réglages ») sont comptés, pas nommés : leur nom peut être un secret.

## Règles liées
- BR-FICHE-032, BR-FICHE-047, BR-FICHE-052

## Historique
- 2026-10-24 — création (HRT-108).
