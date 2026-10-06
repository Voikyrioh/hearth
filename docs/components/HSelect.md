# HSelect

Atome · `apps/desktop/src/components/atoms/HSelect.vue`

Liste déroulante native (clavier et lecteurs d'écran), libellé toujours présent, masqué à l'écran avec `hideLabel` quand le contexte le porte (cellule de tableau).

- Props : `modelValue`, `label`, `options` (`{ value, label }`), `hideLabel`, `disabled`
- Événements et slots : Événement `update:modelValue`
- Notes : Rôle d'un compte (`CreateAccountDialog`, `AccountTable`). Test : `accounts.test.ts`.
