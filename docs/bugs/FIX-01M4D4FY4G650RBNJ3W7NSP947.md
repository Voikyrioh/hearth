---
id: FIX-01M4D4FY4G650RBNJ3W7NSP947
titre: Mode attaque + alerte : 170 px de bandeaux empilés sur toutes les pages (C37)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4D4FY4G650RBNJ3W7NSP947 : Mode attaque + alerte : 170 px de bandeaux empilés sur toutes les pages (C37)

## Symptôme
Deux bandeaux de trois lignes chacun.

## Reproduction
`e2e/layout.spec.ts` « mode attaque et alerte à 1366×800 » : hauteur cumulée au plus 80 px ; rouge avant (98 px).

## Cause root
Titre, texte et date empilés en colonne, grand remplissage.

## Impacté
L'interface du client (revue UX du 2026-10-08), jamais publiée.

## Workaround
Aucun.

## Correction
Un bandeau tient sur une ligne (titre, texte, date côte à côte, retour à la ligne seulement si la place manque), remplissage réduit. `// FIX:01M4D4FY4G650RBNJ3W7NSP947`.

## Règles
- Design : `contexts/hearth/conceptions/design-system-web.md` (fenêtre minimale 1 100 px, jetons existants).

## Non-régression
- Les tests ci-dessus.

## Références
- Ticket : HRT-36 (revue UX du 2026-10-08)
- Code : `components/molecules/SecurityBanner.vue`
