# StatRow

Molécule · `apps/desktop/src/components/molecules/StatRow.vue`

Une ligne « libellé : valeur » d'une carte, avec la marque d'alerte de la valeur. `stacked` : libellé au-dessus de la valeur (cartes étroites).

- Props : `label`, `value`, `level`, `muted`, `stacked`
- Événements et slots : aucun
- Notes : Une valeur absente est dite en clair par l'appelant (« Non disponible »). Tests : `pages/Dashboard.test.ts`.
- HRT-47 (S1b) : le libellé passe par `HMiddleText` ; un slot remplace la valeur texte.
