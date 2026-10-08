---
id: FIX-01M4CRD0HH1YX2RQHBK72GM0VQ
titre: Un échantillon réellement perdu était comblé par interpolation sur la courbe, contre la règle « un vrai trou reste un trou »
date_découverte: 2026-10-06
date_correction: 2026-10-08
---

# FIX-01M4CRD0HH1YX2RQHBK72GM0VQ : Un échantillon réellement perdu était comblé par interpolation sur la courbe, contre la règle « un vrai trou reste un trou »

## Symptôme
Un échantillon à 1 Hz perdu (voisins à environ 2 s l'un de l'autre) était tracé par la moyenne de ses voisins, sans trou visible.

## Reproduction
`dashboard/series.test.ts` : « a really lost sample (neighbours 2 s apart) stays a hole, it is not interpolated » ; rouge avec `BRIDGE_MS = 2500`, vert à 1500.

## Cause root
`BRIDGE_MS` valait 2500 ms : la dérive d'horloge (voisins à ~1 s) ET la perte d'un échantillon (voisins à ~2 s) passaient sous le seuil. La règle écrite (BR-DASH-010) dit qu'un vrai trou reste un trou : la règle prime, le code était trop large.

## Impacté
Le tableau de bord (HRT-11, PR #14), jamais publié.

## Workaround
Aucun.

## Correction
`BRIDGE_MS` passe à 1500 ms : seul le pas vide laissé par la dérive est comblé ; un échantillon perdu reste un trou. `// FIX:01M4CRD0HH1YX2RQHBK72GM0VQ`.

## Règles
- BR-DASH-010 (précisée).

## Non-régression
- Les tests ci-dessus.

## Références
- Ticket : HRT-18 (suites de la review HRT-11, rounds 2 et 3)
- Code : `apps/desktop/src/dashboard/series.ts`
