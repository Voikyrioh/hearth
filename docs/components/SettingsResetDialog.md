# SettingsResetDialog

Organisme · `apps/desktop/src/components/organisms/SettingsResetDialog.vue`

La confirmation d'une remise en bloc (HRT-109, BR-FICHE-044) : liste les réglages concernés avec leur valeur actuelle et leur valeur proposée, dit que les réglages figés, imposés et les mots de passe ne sont pas touchés et que la remise ne prend effet qu'à l'enregistrement ; un réglage sans valeur proposée est remis en retirant sa ligne, et la fenêtre le dit.

- Props : `open`, `title`, `items`
- Événements et slots : `confirm`, `cancel`
- Notes : Tests : `components/organisms/gameSettingsTab.test.ts`.
