# HInput

Atome · `apps/desktop/src/components/atoms/HInput.vue`

Champ texte : libellé toujours visible, aide, erreur sous le champ (`role="alert"`, `aria-invalid`, `aria-describedby`), focus braise (bord `--ac` + halo). Slot `suffix` pour une action intégrée au champ. 

- Props : `modelValue`, `label`, `type` (`text`, `password`, `search`), `help`, `error`, `placeholder`, `autocomplete` (`off` par défaut), `disabled`, `mono` (chasse fixe : adresses, empreintes)
- Événements et slots : événement `update:modelValue` ; slot `suffix`
- Notes : Jetons : `--bg`, `--bd`, `--ac`, `--crit`, `--focus-halo`, `--field-pad-*`. Tests : `atoms.test.ts`.
