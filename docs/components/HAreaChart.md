# HAreaChart

Atome · `apps/desktop/src/components/atoms/HAreaChart.vue`

Courbe pleine en SVG pur (pas d'uPlot, ADR-0015) : trait 2 px, remplissage dégradé de la couleur à 45 % vers transparent, point d'extrémité plein, pas de grille. Plusieurs séries se superposent. Un pas sans mesure (`v` nul) est un TROU : le trait s'interrompt, jamais un zéro (BR-DASH-008).

- Props : `series` (`{ points, tone: 'ac' | 'cool' }[]`), `max` (borne haute ou `null` : suit la plus grande valeur), `label`
- Événements et slots : aucun
- Notes : `role="img"` avec `label`. Jetons : `--ac`, `--cool`, `--chart-fill-opacity`, `--chart-min-height`. Tests : `components/dashboard.test.ts`.
- HRT-41 : au survol, repère vertical et info-bulle « valeur, heure » (`format`), positionnés par la variable `--hover-x`.
- HRT-41 : la courbe est un seul arrêt de tabulation (`role="slider"`) : flèches (Maj : par 10), Début, Fin déplacent le repère, `aria-valuetext` dit « valeur, heure », Échap retire le repère. `name` de série : le texte équivalent résume chaque série nommée (réseau : montant ET descendant).
- HRT-41 (revue de #58) : aucun `aria-valuetext` tant que le repère n'est pas posé (pas de récitation chaque seconde) ; le texte posé reste figé. FIX:01M4ECZJ34NZE9Z9P4YATK707H.
