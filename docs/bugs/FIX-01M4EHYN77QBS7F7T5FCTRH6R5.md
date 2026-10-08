---
id: FIX-01M4EHYN77QBS7F7T5FCTRH6R5
titre: Journal à 1280 : le tableau dépassait sa carte, deux barres horizontales empilées, « Raison » coupée (D1)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4EHYN77QBS7F7T5FCTRH6R5 : Journal à 1280 : le tableau dépassait sa carte, deux barres horizontales empilées, « Raison » coupée (D1)

## Symptôme
Le tableau de 1 020 px dans une carte de 960 px : deux barres de défilement horizontales l'une sur l'autre, la colonne « Raison » coupée.

## Reproduction
`e2e/hrtp2.spec.ts` « le tableau tient dans sa carte, une seule barre horizontale au plus » aux 5 tailles. Rouge avant : tableau de 1 020 px dans 960 px à 1280, deux zones qui défilent à 1100.

## Cause root
Largeur minimale des colonnes (1 026 px avec écarts et marges) supérieure à la page d'une fenêtre de 1 280 px ; la zone de lignes, qui défile en hauteur, défilait aussi en largeur (`overflow-x` devenu `auto`).

## Impacté
L'interface du client (seconde passe UX du 2026-10-08), jamais publiée.

## Workaround
Aucun.

## Correction
Minimums de colonnes réduits (842 px, tableau de 950 px) ; zone de lignes en `overflow-x: hidden` : une seule barre horizontale, celle de la carte, sous 1 280 px. `// FIX:01M4EHYN77QBS7F7T5FCTRH6R5`.

## Règles
- Aucune règle métier modifiée.

## Non-régression
- Les tests ci-dessus.

## Références
- Ticket : HRT-43 (seconde passe UX : `contexts/hearth/art/ux-review-2026-10-08-passe2.md`)
- Code : `components/organisms/AuditTable.vue, styles/tokens.css`
