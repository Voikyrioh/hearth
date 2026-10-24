# HTextarea

Atome · `apps/desktop/src/components/atoms/HTextarea.vue`

Zone de texte, même langage que `HInput` (libellé visible, aide, erreur sous le champ avec `role="alert"` et `aria-invalid`, focus braise). Les lignes grandissent avec la saisie entre `rows` et `maxRows`, puis la zone défile. Sert à la commande de lancement d'un service (HRT-69), texte sensible : jamais affiché ailleurs que dans ce champ.

- Props : `modelValue`, `label`, `help`, `error`, `placeholder`, `disabled`, `mono`, `rows` (3), `maxRows` (8)
- Événements : `update:modelValue`, `blur`
- Notes : jetons `--bg`, `--bd`, `--ac`, `--crit`, `--focus-halo`, `--command-max-height`. Tests : `atoms/deployAtoms.test.ts`.
