# HSegmented

Atome · `apps/desktop/src/components/atoms/HSegmented.vue`

Choix exclusif entre quelques options : `radiogroup`, flèches pour passer d'une option à l'autre (avec bouclage), un seul arrêt de tabulation.

- Props : `modelValue`, `options` (`{ value, label }[]`), `label` (nom du groupe)
- Événements et slots : événement `update:modelValue`
- Notes : Générique (`T extends string`). Tests : `atoms.test.ts`.
