# SettingsImportDialog

Organisme · `apps/desktop/src/components/organisms/SettingsImportDialog.vue`

« Importer des réglages » (HRT-109, BR-FICHE-052) sur `AdminActDialog` (acte `game_settings_save`) : l'utilisateur colle un ancien fichier, l'agent le lit (il connaît le format du jeu) et rend, réglage par réglage, l'état (Reconnu, Inconnu : sera gardé, Figé : ignoré, Valeur que ce jeu ne connaît pas) ; une valeur refusée n'est pas appliquée, on choisit une valeur de la liste ou on garde l'ancienne ; « Appliquer ces réglages » est l'enregistrement ordinaire (origine `import`, les valeurs sont reprises du texte par l'agent, mot de passe collé compris). Le texte vit en mémoire dans la fenêtre et s'efface à sa fermeture, à l'issue ou au démontage.

- Props : `open`, `serverId`, `models`, `baseRev`, `fileStamp`, `analyze`, `save`, `labelOf`
- Événements et slots : `close`
- Notes : Règles pures : `settings/importing.ts`. Tests : `components/organisms/gameSettingsTab.test.ts`.
