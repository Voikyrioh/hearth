---
id: BR-FICHE-047
domaine: FICHE
titre: Un réglage inconnu, un commentaire et toute ligne incomprise ne sont jamais perdus ; une ligne commentée n'est jamais réactivée
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-047), contexts/hearth/conceptions/2026-10-13-technique-fiches-de-jeu.md, HRT-113
maj: 2026-10-24
---

# BR-FICHE-047 — Un réglage inconnu, un commentaire et toute ligne incomprise ne sont jamais perdus ; une ligne commentée n'est jamais réactivée

## Règle
Hearth ne retouche que la valeur du réglage demandé : tout le reste du fichier survit, à sa place, octet pour octet, à un enregistrement, une remise aux valeurs proposées, un import. Une ligne mise en commentaire reste un commentaire : régler ce réglage ajoute une vraie ligne. Les clés du fichier que la fiche ne connaît pas sont rendues par `settings_state::drift` (le fichier gagne).

## Application (code)
- `crates/hearth-agent/src/domain/catalog/formats/ (`set` et `unset` ne touchent que leur entrée)`
- `crates/hearth-agent/src/domain/catalog/settings_state.rs::drift`

- `crates/hearth-agent/src/domain/catalog/settings_state.rs::looks_secret`
- `crates/hearth-agent/src/domain/catalog/settings_io.rs::plan_save` (HRT-108 : la retouche ne touche que les valeurs demandées ; les « autres réglages » se changent ou se retirent par leur clé)
- `crates/hearth-agent/src/application/game_settings.rs::GameSettings::view` (HRT-108 : les réglages inconnus sont rendus, la valeur d'un nom qui ressemble à un secret est masquée)
- Client (HRT-109) : `apps/desktop/src/components/organisms/OtherSettingsSection.vue` (« Autres réglages », « Retirer » avec confirmation, valeur masquée sans champ)

## Vérification
- `crates/hearth-agent/src/domain/catalog/tests_state.rs::applying_edits_to_the_game_file_keeps_every_unknown_line_through_every_gesture`
- `crates/hearth-agent/src/domain/catalog/formats/kv_lines.rs::tests::a_commented_line_is_never_reactivated`
- `crates/hearth-agent/src/domain/catalog/formats/contract.rs::ten_successive_settings_keep_comments_unknown_lines_commented_lines_and_line_endings`

- `crates/hearth-agent/src/domain/catalog/tests_io.rs::an_invented_setting_and_a_comment_survive_every_gesture_of_br_fiche_047`
- `crates/hearth-agent/tests/game_settings_https.rs::saving_touches_only_the_asked_line_and_every_unknown_line_and_comment_survives`
- `crates/hearth-agent/tests/game_settings_https.rs::an_import_applies_the_chosen_lines_reads_the_secret_on_the_agent_and_keeps_the_unknown`
- `crates/hearth-agent/tests/game_settings_https.rs::the_other_settings_of_the_file_can_be_changed_or_removed_and_a_secret_looking_one_is_masked`
- `apps/desktop/src/components/organisms/gameSettingsTab.test.ts::le fichier brut, la console et les autres réglages`

## Cas limites
- Le masquage à l'affichage (nom qui contient PASS, SECRET, TOKEN ou KEY, sans tenir compte de la casse : `settings_state::looks_secret`, dans le domaine à côté de `drift`) est livré par HRT-108. **Écart avec la spécification** : la valeur d'un réglage inconnu n'est pas copiée « chiffrée » côté serveur, elle ne vit que dans le fichier du jeu (droits `0600`, propriétaire de la fiche) : une seconde copie scellée ne servirait qu'à la divergence. Les gestes d'une restauration, d'un changement de version du jeu et d'une mise à jour de la fiche (versions précédentes de fichier, HRT-110/121) rejouent le même plan et gardent donc l'inconnu.

## Règles liées
- BR-FICHE-046, BR-FICHE-048

## Historique
- 2026-10-13 — création (HRT-113, format de fiche version 2, domaine pur).
- 2026-10-24 — pointeurs du client (HRT-109).
