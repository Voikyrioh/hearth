# TimeSeriesChart

Molécule · `apps/desktop/src/components/molecules/TimeSeriesChart.vue`

Courbe d'une mesure sur la fenêtre choisie. Quand l'historique gardé ne couvre pas toute la fenêtre (application récemment ouverte), le dit : « Depuis 12 min ».

- Props : `series`, `max`, `label`, `window` (`1m`, `5m`, `1h`), `coveredMs`
- Événements et slots : aucun
- Notes : BR-DASH-010. Tests : `components/dashboard.test.ts`, `pages/Dashboard.test.ts`.
- HRT-41 : au-dessus de la courbe, la durée (« 5 dernières minutes ») et l'échelle (« 0 à 100 % ») ; `format` (formateur d'unités du tableau de bord) ; `legend` pour les disques et les températures ; `aria-label` « dernière valeur, minimum, maximum, sur … ». FIX:01M4E9T718D37EXTXMWA7YXWJE.
- Seconde passe (D4) : durée et échelle sans retour à la ligne ; « 5 min » sous 260 px de courbe. FIX:01M4EHYP5JP846FGWCXJ41ET32.
