# HGaugeArc

Atome · `apps/desktop/src/components/atoms/HGaugeArc.vue`

Arc de jauge de 270° en SVG pur : piste `--bd`, arc de valeur (dégradé braise à l'état normal, `--warn` en attention, `--crit` en critique) avec lueur, qui glisse vers sa nouvelle valeur (300 ms). `ratio` nul : la piste seule (mesure illisible, BR-DASH-008).

- Props : `ratio` (0 à 1 ou `null`), `level` (`normal`, `attention`, `critical`)
- Événements et slots : aucun
- Notes : Décoratif (`aria-hidden`) : le sens est porté par `Gauge`. Jetons : `--ac`, `--ac2`, `--warn`, `--crit`, `--bd`, `--glow-radius`, `--motion-base`. Tests : `components/dashboard.test.ts`.
- HRT-41 (C13) : niveau normal en braise unie `--ac` (plus de dégradé vers le rose) ; alerte seulement aux seuils. FIX:01M4E9T77SYNC7MK8Q1JP5PQ4J.
