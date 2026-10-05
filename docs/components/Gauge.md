# Gauge

Molécule · `apps/desktop/src/components/molecules/Gauge.vue`

Jauge : arc (`HGaugeArc`), valeur au centre en mono, libellé dessous, marque d'alerte. Mesure illisible : « Non disponible » à la place de la valeur (BR-DASH-008).

- Props : `label`, `ratio`, `valueText`, `level`, `size` (`md`, `sm`)
- Événements et slots : aucun
- Notes : `<figure>` avec `aria-label` « libellé : valeur » et `data-level`. Tests : `components/dashboard.test.ts`.
