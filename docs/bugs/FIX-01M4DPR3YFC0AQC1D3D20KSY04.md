---
id: FIX-01M4DPR3YFC0AQC1D3D20KSY04
titre: Barres de défilement blanches, une par zone (C9)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4DPR3YFC0AQC1D3D20KSY04 : Barres de défilement blanches, une par zone (C9)

## Symptôme
Les barres de défilement système, blanches sur le fond sombre.

## Reproduction
`e2e/layout.spec.ts` « barres de défilement à … : sombres, et jamais deux à la fois » (rouge avant : `scrollbar-color` valait `auto`).

## Cause root
Aucun style de barre ni `color-scheme` : la WebView affichait la barre claire du système.

## Impacté
Toutes les zones qui défilent du client (tableau de bord, Comptes, Réglages, Mes serveurs, assistant d'ajout, listes de la barre). Source : revue UX de Nora du 2026-10-08 (`contexts/hearth/art/ux-review-2026-10-08.md`).

## Workaround
Aucun.

## Correction
`color-scheme: dark` ; poignée `--scroll-thumb` (#7d6c68) contre sa surface : 3,60 pour 1 sur le fond de page, 3,42 sur la barre latérale, 3,17 sur une carte (WCAG 1.4.11 : 3), au survol `--scroll-thumb-hover` (#b3a19c) : 7,26 / 6,89 / 6,38. Chromium (WebView2) prend les pseudo-éléments `::-webkit-scrollbar*` (seuls à permettre l'éclaircissement au survol, les propriétés normalisées les écraseraient), les autres moteurs `scrollbar-width` et `scrollbar-color` (`@supports`). Test des ratios : `styles/scrollbar.test.ts`. Rendu WebView2 réel non vu, à regarder au smoke. NON fait : les deux barres imbriquées du Journal d'activité (page et tableau, `AuditTable`, autre développeur) ; la vérification en vrai WebView2 reste à faire.

## Règles
- Aucune règle métier touchée (interface), sauf mention.
