---
id: FIX-01M4E9T718D37EXTXMWA7YXWJE
titre: Une courbe du tableau de bord ne disait ni sa durée, ni son échelle, ni sa valeur (C10)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4E9T718D37EXTXMWA7YXWJE : Une courbe du tableau de bord ne disait ni sa durée, ni son échelle, ni sa valeur (C10)

## Symptôme
Aucune courbe n'avait d'échelle, de durée, de valeur au survol, ni de texte équivalent ; les courbes des disques et des températures n'avaient aucune légende.

## Reproduction
`e2e/hrt41.spec.ts` (aux 5 tailles) : durée et échelle écrites sur chaque courbe, info-bulle « valeur, heure » au survol, texte équivalent (dernière valeur, minimum, maximum), légendes des disques et des températures. Vitest `components/dashboard.test.ts`. Rouge avant : rien de tout cela n'existait.

## Cause root
`TimeSeriesChart` ne portait que « Depuis N min » et `HAreaChart` ne gérait pas le pointeur.

## Impacté
L'interface du client (revue UX du 2026-10-08), jamais publiée.

## Workaround
Aucun.

## Correction
Ligne « durée / échelle » au-dessus de chaque courbe (`dash.span*`, `dash.scaleTo`), légende des disques et des températures, survol (repère et info-bulle valeur + heure, mêmes formateurs d'unités que le reste du tableau de bord : BR-DASH-014), `aria-label` avec dernière valeur, minimum et maximum. `// FIX:01M4E9T718D37EXTXMWA7YXWJE`.

## Règles
- BR-DASH-010, BR-DASH-014 (formateurs d'unités uniques).

## Non-régression
- Les tests ci-dessus.

## Références
- Ticket : HRT-41 (revue UX du 2026-10-08)
- Code : `components/molecules/TimeSeriesChart.vue, components/atoms/HAreaChart.vue`
