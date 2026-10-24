# ReauthField

Molécule · `apps/desktop/src/components/molecules/ReauthField.vue`

Le champ « Ton mot de passe » de la confirmation d'un acte (HRT-30) : `HPasswordInput` masqué, `current-password`, aide « La clé de ce poste est vérifiée en même temps. », erreur sous le champ. Aucune valeur gardée ici ; la fenêtre qui le porte (`AdminActDialog`) le vide après chaque envoi.

- Props : `modelValue`, `label?`, `help?`, `error?`
- Événements et slots : `update:modelValue` ; expose `focus()`
- Notes : Tests via `components/organisms/adminAct.test.ts`.
- `focus(options?)` : `{ preventScroll: true }` à l'ouverture d'une fenêtre (un récapitulatif reste ouvert en haut, revue UX du 2026-10-24, constat 15), sans option après un mot de passe refusé.
