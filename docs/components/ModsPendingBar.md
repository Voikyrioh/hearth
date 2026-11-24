# ModsPendingBar

Molécule · `apps/desktop/src/components/molecules/ModsPendingBar.vue`

La barre « N changements prêts » de l'onglet Mods (HRT-262, maquette Main) : « Tout annuler » et « Vérifier et appliquer » ; rien n'est fait tant que le compte n'a pas appliqué.

- Props : `count`, `sentence` (le changement nommé quand il est seul), `busy`, `blockedReason`
- Colle au bas de la zone qui défile (`position: sticky`).
- Événements : `cancelAll`, `review`

Collante en haut (sous l'en-tête collant, `--mods-pinned-h`) et en bas de la zone qui défile : elle reste à portée où que l'on soit dans la liste.
