---
id: BR-FICHE-044
domaine: FICHE
titre: Retour aux valeurs proposées : une remise ne touche jamais un secret ni un réglage figé, et retire la ligne quand rien n'est proposé
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-044), HRT-108
maj: 2026-10-24
---

# BR-FICHE-044 — Une remise aux valeurs proposées ne touche ni un secret ni un réglage figé

## Règle
Le geste `reset` (origine `reset` de l'acte) remet le réglage à la valeur proposée par la fiche ; quand la fiche n'en propose aucune, il le remet à « non écrit » et retire la ligne du fichier (le jeu reprend son défaut). Les remises par thème et pour tout l'onglet sont une liste de gestes `reset` composée par le client ; l'agent refuse, sans rien écrire, une remise qui contient un secret (`422 SETTING_REFUSED`, `reason = "secret"`) ou un réglage figé (`reason = "frozen"`) ou imposé (`reason = "imposed"`). Une remise ne prend effet qu'à l'enregistrement.

## Application (code)
- `crates/hearth-agent/src/domain/catalog/settings_state.rs::{apply_changes, Change}`
- `crates/hearth-agent/src/application/game_settings.rs::GameSettings::save`
- Client (HRT-109) : `apps/desktop/src/settings/edits.ts::{resettable, effectiveOf}`, `apps/desktop/src/composables/useGameSettings.ts::{resetItems, applyReset}`, `apps/desktop/src/components/organisms/SettingsResetDialog.vue`

## Vérification
- `crates/hearth-agent/src/domain/catalog/tests_state.rs::reset_puts_the_proposed_value_back_or_removes_the_line_when_nothing_is_proposed`
- `crates/hearth-agent/tests/game_settings_https.rs::a_reset_never_touches_a_secret_nor_a_frozen_setting`
- `crates/hearth-agent/tests/game_settings_https.rs::saving_touches_only_the_asked_line_and_every_unknown_line_and_comment_survives`
- `apps/desktop/src/settings/settings.test.ts::modifié, remis, remise en bloc (BR-FICHE-041, 044)`
- `apps/desktop/src/components/organisms/gameSettingsTab.test.ts::remettre aux valeurs proposées (BR-FICHE-044)`

## Cas limites
- Choisir quels réglages une remise en bloc contient (et la confirmation qui les liste) est du client (HRT-109) ; l'agent est le dernier garde-fou.

## Règles liées
- BR-FICHE-037, BR-FICHE-042, BR-FICHE-047

## Historique
- 2026-10-24 — création (HRT-108).
- 2026-10-24 — pointeurs du client (HRT-109).
