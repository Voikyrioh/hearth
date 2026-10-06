# MultiSelect

Molécule · `apps/desktop/src/components/molecules/MultiSelect.vue`

Liste déroulante à choix multiple : le bouton annonce la sélection (« Tous », le libellé choisi, ou « N choisis »), la liste est un groupe de cases à cocher natives ; Échap la ferme et rend le focus au bouton, un clic ou un Tab ailleurs la ferme. Le bouton est nommé par le libellé du champ et sa valeur.

- Props : `modelValue`, `label`, `options` (`{ value, label }[]`)
- Événements et slots : `update:modelValue` (dans l'ordre des options)
- Notes : Générique (`T extends string`). Tests : `pages/audit.test.ts`.
