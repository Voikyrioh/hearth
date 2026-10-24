---
id: BR-FICHE-037
domaine: FICHE
titre: Une fiche expose un catalogue de réglages ; un réglage a trois états : une valeur, la valeur proposée, ou non écrit
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-037), contexts/hearth/conceptions/2026-10-13-technique-fiches-de-jeu.md, HRT-113
maj: 2026-10-24
---

# BR-FICHE-037 — Une fiche expose un catalogue de réglages ; un réglage a trois états : une valeur, la valeur proposée, ou non écrit

## Règle
Chaque réglage porte un identifiant Hearth stable, le nom du jeu, un canal (`env`, `args`, `file`), un thème, un niveau (`essential`, `common`, `advanced`), un libellé et une aide bornés, un type fermé (`text`, `secret`, `int`, `bool`, `choice`, `id_list`, `opaque`), le défaut du jeu, la valeur proposée, `change` (`anytime` ou `world_creation`), une valeur imposée avec sa raison, des versions, sa source (`verified`, `partial`, `name_only`). Un réglage `name_only` a le libellé fixe « Réglage du jeu, sans explication » et le niveau `advanced`. Quand le défaut du jeu n'est pas vérifié, la fiche NE LE DEVINE PAS : elle ne propose rien et Hearth n'écrit pas la ligne (« non écrit » ≠ « valeur par défaut »). Tout réglage du jeu est classé : réglable, imposé avec sa raison, ou `[[not_exposed]]` avec sa raison (la fiche Terraria classe les 36 réglages de l'inventaire). L'effet d'un changement se DÉDUIT du canal (`file` : redémarrage ; `env` et `args` : recréation), la fiche ne le déclare pas.

## Application (code)
- `crates/hearth-agent/src/domain/catalog/loading/model.rs::{Setting, ValueSpec, validate_value}`
- `crates/hearth-agent/src/domain/catalog/loading/loader.rs::{resolve_settings, check_settings}`
- `crates/hearth-agent/src/domain/catalog/settings_state.rs::{resolve_install, initial_state, apply_changes, effect_of}`
- `crates/hearth-agent/catalog/terraria/settings.toml`

- `crates/hearth-agent/src/application/game_settings.rs::GameSettings::view` (HRT-108 : les trois états d'un réglage rendus au client, jamais la valeur d'un secret)
- Client (HRT-109) : `apps/desktop/src/settings/model.ts::{modelsOf, installModelsOf, proposedOf, defaultUnverified}`, `apps/desktop/src/components/molecules/GameSettingRow.vue`

## Vérification
- `crates/hearth-agent/src/domain/catalog/tests_state.rs::not_written_is_a_value_distinct_from_the_default_of_the_game`
- `crates/hearth-agent/src/domain/catalog/tests_state.rs::the_effect_of_a_change_is_deduced_from_the_channel_never_declared`
- `crates/hearth-agent/src/domain/catalog/tests_state.rs::reset_puts_the_proposed_value_back_or_removes_the_line_when_nothing_is_proposed`
- `crates/hearth-agent/src/domain/catalog/tests_fiches.rs::the_terraria_fiche_classes_each_of_the_36_settings_of_the_inventory`
- `crates/hearth-agent/src/domain/catalog/tests_translate.rs::terraria_with_the_proposed_values_writes_only_what_is_known_and_sends_no_order`

- `crates/hearth-agent/tests/game_settings_https.rs::reading_shows_the_installed_values_the_file_state_and_never_the_password`
- `apps/desktop/src/settings/settings.test.ts::le catalogue de Terraria rendu lisible (objectif de Voiky)`
- `apps/desktop/src/components/organisms/gameSettingsTab.test.ts::les thèmes en mots simples (BR-FICHE-041)`

## Cas limites
- Les 15 permissions du mode Voyage de Terraria sont exposées sans défaut : l'inventaire n'en a lu aucun.
- Terraria, version 2 de la fiche (HRT-108 : `priority` s'ajoute), expose 30 des 31 réglages de son fichier plus deux arguments ; `banlist` (emplacement dans le conteneur non vérifié) et les arguments `-ip`, `-steam`, `-lobby` sont écartés, avec leur raison. Libellés et aides en vouvoiement, bornes et listes de valeurs là où l'inventaire les donne, aucune version d'existence ni défaut inventé (`crates/hearth-agent/src/domain/catalog/tests_fiches.rs::the_terraria_catalog_speaks_to_the_user_in_vous_and_every_text_stays_within_its_bounds`).

## Règles liées
- BR-FICHE-038, BR-FICHE-047, BR-FICHE-049

## Historique
- 2026-10-13 — création (HRT-113, format de fiche version 2, domaine pur).
- 2026-10-24 — pointeurs du client (HRT-109).
