# ColorSwatches

Molécule · `apps/desktop/src/components/molecules/ColorSwatches.vue`

Palette des 8 couleurs de serveur : `radiogroup` de pastilles rondes, la sélectionnée a un anneau ; chaque pastille a un nom accessible (« Couleur 3 »), le sens ne repose pas sur la couleur seule.

- Props : `modelValue` (1 à 8), `label`, `disabled`
- Événements et slots : événement `update:modelValue`
- Notes : Jetons `--server-1` à `--server-8`. Tests : `connect.test.ts`, `Servers.test.ts`.
