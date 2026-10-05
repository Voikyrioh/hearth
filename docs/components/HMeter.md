# HMeter

Atome · `apps/desktop/src/components/atoms/HMeter.vue`

Jauge linéaire en SVG pur : occupation d'un disque, couleur selon le niveau (BR-DASH-003).

- Props : `ratio` (0 à 1 ou `null`), `level`, `label`
- Événements et slots : aucun
- Notes : Jetons : `--ac`, `--warn`, `--crit`, `--bd`. Tests : `components/dashboard.test.ts`.
