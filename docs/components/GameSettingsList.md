# GameSettingsList

Organisme · `apps/desktop/src/components/organisms/GameSettingsList.vue`

La liste des réglages d'un jeu (HRT-109, BR-FICHE-041, 043, 045), la même dans l'onglet « Réglages » et dans « Voir tous les réglages » de l'installation : recherche (par libellé, nom technique et phrase, tous niveaux), filtre « Modifiés », bascule « Courants / Tout afficher », une carte repliable par thème dans l'ordre de la fiche (`SettingThemeCard`), les réglages que le jeu n'a plus à part, et un slot `after` pour les sections de fin. Présentation seule : l'état est à l'appelant.

- Props : `models`, `themes`, `edits`, `mode` (`service` ou `install`), `issues`, `filter` (v-model), `disabled`
- Événements et slots : `set`, `unset`, `reset`, `resetTheme`, `blur`, `link` ; slot `after` ; expose `focusSearch()`
- Notes : Tests : `components/organisms/gameSettingsTab.test.ts`, `pages/InstallSettings.test.ts`.
