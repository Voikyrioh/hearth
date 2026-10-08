---
id: FIX-01M4CRD1VMNQP4W619XVF1AWRE
titre: Un pic rangé avec d'autres échantillons dans un même pas était tracé à la moyenne du pas (100 % tracé à 55 %)
date_découverte: 2026-10-06
date_correction: 2026-10-08
---

# FIX-01M4CRD1VMNQP4W619XVF1AWRE : Un pic rangé avec d'autres échantillons dans un même pas était tracé à la moyenne du pas (100 % tracé à 55 %)

## Symptôme
Une pointe à 100 % de processeur, rangée dans le même pas qu'un échantillon calme, se voyait à environ 55 %.

## Reproduction
`dashboard/series.test.ts` : « a 100 % peak among calm samples of the same step is drawn at 100 %, not at the mean » ; rouge avec la moyenne.

## Cause root
`resample` donnait la MOYENNE des valeurs d'un pas ; sur la fenêtre d'une heure (pas de 10 s) comme sur deux échantillons voisins d'un pas de 1 s, un pic disparaissait.

## Impacté
Le tableau de bord (HRT-11, PR #14), jamais publié.

## Workaround
Aucun.

## Correction
`resample` garde le MAXIMUM du pas. `// FIX:01M4CRD1VMNQP4W619XVF1AWRE`. Limite dite : les 55 premières minutes lues à l'ouverture sont déjà des moyennes de 10 s calculées par l'agent.

## Règles
- BR-DASH-010 (précisée).

## Non-régression
- Les tests ci-dessus.

## Références
- Ticket : HRT-18 (suites de la review HRT-11, rounds 2 et 3)
- Code : `apps/desktop/src/dashboard/series.ts`
