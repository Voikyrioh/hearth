---
id: FIX-01M4D1K8675QMBAFTM76XF4XJC
titre: La carte Carte graphique absente restait sans explication (C15)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4D1K8675QMBAFTM76XF4XJC : La carte Carte graphique absente restait sans explication (C15)

## Symptôme
« Non disponible sur cette machine » seul, alors que la carte Températures explique son absence.

## Reproduction
`pages/Dashboard.test.ts` « a machine without GPU and probes keeps both sections with their explanation » ; Playwright « une machine sans carte graphique ni sonde ».

## Cause root
Le texte réutilisait la mention générique `dash.notOnMachine`.

## Impacté
Le tableau de bord (HRT-11), jamais publié.

## Workaround
Aucun.

## Correction
Une phrase d'explication (`dash.noGpu`). `// FIX:01M4D1K8675QMBAFTM76XF4XJC`.

## Règles
- Design : grille 12 puis 6 colonnes (`contexts/hearth/conceptions/2026-10-04-design-ecrans-socle.md`).

## Non-régression
- Les tests ci-dessus.

## Références
- Ticket : HRT-34 (revue UX du 2026-10-08)
- Code : `apps/desktop/src/components/organisms/GpuCard.vue`, `i18n/fr.ts`
