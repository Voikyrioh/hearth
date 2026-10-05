# SettingRow

Molécule · `apps/desktop/src/components/molecules/SettingRow.vue`

Ligne de réglage : libellé, aide optionnelle et contrôle à droite. Fournit l'identifiant du libellé au contrôle pour qu'il se nomme par `aria-labelledby`.

- Props : `label`, `help` (optionnelle)
- Événements et slots : slot par défaut avec la portée `{ labelId }`
- Notes : l'identifiant vient de `useId()`. Utilisée par `Settings`. Tests : `SettingRow.test.ts`.
