---
id: BR-FICHE-048
domaine: FICHE
titre: Le jeu ou une main peut réécrire le fichier : Hearth compare, ne corrige pas seul, et n'écrit que dans quatre cas
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-048), contexts/hearth/conceptions/2026-10-13-technique-fiches-de-jeu.md, HRT-113
maj: 2026-10-24
---

# BR-FICHE-048 — Le jeu ou une main peut réécrire le fichier : Hearth compare, ne corrige pas seul, et n'écrit que dans quatre cas

## Règle
Quatre moments d'écriture, aucun autre : la création à l'installation (`ConfigFormat::create`, le fichier n'existe pas), un enregistrement de réglages (`apply_changes` rend les retouches), la recréation quand le fichier a disparu, le changement du réglage de port d'un jeu qui annonce son port. Le domaine fournit : l'écart entre la valeur voulue et celle du fichier (`drift`, le fichier gagne pour un réglage inconnu), les réglages imposés défaits (`imposed_broken`, aucune écriture), et l'effet déduit du canal.

## Application (code)
- `crates/hearth-agent/src/domain/catalog/settings_state.rs::{drift, imposed_broken, apply_changes}`
- `crates/hearth-agent/src/domain/catalog/formats/mod.rs::ConfigFormat::create`

- `crates/hearth-agent/src/domain/catalog/settings_io.rs::{plan_save, DriftChoice}` (HRT-108 : un choix par réglage en écart, sinon rien n'est planifié)
- `crates/hearth-agent/src/application/game_settings.rs::GameSettings::{view, save, recreate_missing_files}` (HRT-108 : empreinte du fichier lue à l'ouverture et revérifiée avant l'écriture, `game_settings.file_fingerprint`)
- Client (HRT-109) : `apps/desktop/src/components/organisms/FileChangedBanner.vue`, `apps/desktop/src/settings/edits.ts::toSaveRequest` (un choix par réglage en écart, « Garder la valeur du fichier » par défaut), `apps/desktop/src/composables/useGameSettings.ts` (relit la vue après `SETTINGS_FILE_CHANGED`)

## Vérification
- `crates/hearth-agent/src/domain/catalog/tests_state.rs::drift_compares_the_state_to_the_file_and_lists_unknown_lines_without_touching_them`
- `crates/hearth-agent/src/domain/catalog/tests_state.rs::an_imposed_setting_undone_by_hand_is_reported_never_rewritten`

- `crates/hearth-agent/tests/game_settings_https.rs::a_file_the_game_rewrote_is_shown_and_nothing_is_written_without_a_choice_per_setting`
- `crates/hearth-agent/tests/game_settings_https.rs::a_file_that_moved_since_it_was_read_is_refused_by_its_stamp`
- `crates/hearth-agent/tests/game_settings_https.rs::an_imposed_setting_undone_by_hand_is_reported_and_never_rewritten`
- `crates/hearth-agent/tests/game_settings_use_cases.rs::a_vanished_file_is_recreated_from_the_wanted_values_secret_included_and_a_present_one_is_never_touched`
- `apps/desktop/src/components/organisms/gameSettingsTab.test.ts::le fichier a changé (BR-FICHE-048)`
- `apps/desktop/src/link/simulated-game-settings.test.ts::l'écart quand le jeu a réécrit son fichier (BR-FICHE-048)`

## Cas limites
- Écart : `GET /services/{key}/settings` rend `drifts` (réglage, valeur de Hearth, valeur du fichier ; jamais un secret) ; un enregistrement exige un choix (`keep_file`, `keep_mine`) par réglage en écart, sinon `409 SETTINGS_FILE_CHANGED` (`details.ids`) et rien n'est écrit ; changer soi-même le réglage en écart vaut choix. `file_stamp` (SHA-256 du fichier tel que lu) refuse un fichier qui a bougé entre la lecture et l'enregistrement. L'empreinte du fichier tel que Hearth l'a écrit en dernier est en base (`file_changed` dans la vue).
- **Valeur du fichier invalide** : « Garder la valeur du fichier » n'adopte jamais une valeur que le réglage n'accepte pas (hors bornes, hors choix) : l'enregistrement est refusé (`SETTING_REFUSED`) et la vue signale l'écart `file_valid: false` ; « Remettre la mienne » ou un autre changement tranche. Sinon une valeur invalide serait refusée plus tard, à la recréation du fichier.
- **Course** : chaque fichier est écrit seulement s'il est ENCORE tel que le plan l'a lu, en un seul geste (lecture, comparaison, écriture atomique : `swap_file`) ; sinon `SETTINGS_FILE_CHANGED` et rien n'est écrit. Si l'enregistrement perd ensuite la course en base, `undo_writes` remet le contenu que son écriture a remplacé (celui du gagnant, ou à défaut l'ancien) mais seulement si le fichier porte encore ce que cet enregistrement y a écrit ; la réponse est `SETTINGS_FILE_CHANGED` avec `details.by` et `details.at`. Fichier et base restent d'accord.
- Les quatre moments : l'installation (HRT-101), l'enregistrement et la recréation d'un fichier disparu (`recreate_missing_files`, **fournie et testée, pas encore appelée au démarrage d'un service lancé par Hearth** : voir le suivi HRT-108) sont livrés ; le changement du port d'un jeu lié par `same_number` (opération de modification) n'a pas de route dans cette version.
- Le journal d'intention avec l'ancien contenu scellé (`write_channel_file`) attend le journal des versions précédentes de fichier (FILES-018 de la spec du hub) : un échec d'écriture remet en mémoire l'ancien contenu des fichiers déjà écrits.

## Règles liées
- BR-FICHE-046, BR-FICHE-047, BR-JEU-039

## Historique
- 2026-10-13 — création (HRT-113, format de fiche version 2, domaine pur).
- 2026-10-24 — HRT-108 : lecture, écart, empreinte et enregistrement par l'agent.
- 2026-10-24 — pointeurs du client (HRT-109).
