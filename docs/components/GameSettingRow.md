# GameSettingRow

Molécule · `apps/desktop/src/components/molecules/GameSettingRow.vue`

Une ligne de réglage (HRT-109, BR-FICHE-037, 041, 042, 044, 057), le contrôle de son genre (`QuestionField`, `IdListSetting`, un champ à chasse fixe pour un texte opaque), le nom technique du jeu en petit, le défaut du jeu (« non vérifié » quand la fiche ne le dit pas), la valeur proposée quand elle diffère, le repère « Modifié », l'une des quatre mentions d'effet, « Remettre la valeur proposée ». Un réglage figé, imposé, retiré ou qui attend la recréation du service est affiché avec sa raison et son contrôle grisé ; un réglage imposé n'a pas de contrôle. Un secret n'est jamais lu : « Défini » ou non, « Retirer ». Nommée `GameSettingRow` (la conception dit `SettingRow`) pour ne pas heurter la ligne générique `SettingRow` de la page Réglages du client.

- Props : `model` (`SettingModel`), `edit`, `mode`, `issue`, `disabled`
- Événements et slots : `set`, `unset`, `reset`, `blur`, `link`
- Notes : Règles pures : `settings/edits.ts`, textes `settings/messages.ts`. Tests : `components/organisms/gameSettingsTab.test.ts`.
