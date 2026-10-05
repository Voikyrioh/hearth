# HPasswordInput

Atome · `apps/desktop/src/components/atoms/HPasswordInput.vue`

Champ mot de passe construit sur `HInput` : masqué par défaut, bouton « Afficher / Masquer le mot de passe » (`aria-pressed`). Le mot de passe n'est jamais affiché ailleurs (BR-ACCT-006).

- Props : `modelValue`, `label`, `help`, `error`, `placeholder`, `autocomplete`, `disabled`
- Événements et slots : événement `update:modelValue`
- Notes : Tests : `atoms.test.ts`.
