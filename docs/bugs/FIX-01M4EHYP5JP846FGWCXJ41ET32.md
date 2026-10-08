---
id: FIX-01M4EHYP5JP846FGWCXJ41ET32
titre: À 1100, les légendes des courbes passaient sur deux lignes (D4)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4EHYP5JP846FGWCXJ41ET32 : À 1100, les légendes des courbes passaient sur deux lignes (D4)

## Symptôme
« 5 dernières / minutes » et « 0 à 100 / % » sur deux lignes dans les cartes étroites.

## Reproduction
`e2e/hrtp2.spec.ts` « la durée et l'échelle d'une courbe tiennent sur une ligne » à 1100. Rouge avant : lignes de plus de 20 px.

## Cause root
Texte long dans une colonne de courbe étroite, sans repli.

## Impacté
L'interface du client (seconde passe UX du 2026-10-08), jamais publiée.

## Workaround
Aucun.

## Correction
Sans retour à la ligne ; sous 260 px de courbe (requête de conteneur) la durée s'écrit « 5 min » (le texte équivalent garde la forme complète). `// FIX:01M4EHYP5JP846FGWCXJ41ET32`.

## Règles
- Aucune règle métier modifiée.

## Non-régression
- Les tests ci-dessus.

## Références
- Ticket : HRT-41 (seconde passe UX : `contexts/hearth/art/ux-review-2026-10-08-passe2.md`)
- Code : `components/molecules/TimeSeriesChart.vue`
