---
id: FIX-01M4D4Y22QGBWNKKJTK8K3X12A
titre: Le nom accessible du chargement ne suivait pas le texte affiché (retour de revue HRT-34)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4D4Y22QGBWNKKJTK8K3X12A : Le nom accessible du chargement ne suivait pas le texte affiché (retour de revue HRT-34)

## Symptôme
Après 3 s le texte disait « Mesures en cours de chargement… » mais le lecteur d'écran lisait « Chargement des mesures ».

## Reproduction
`pages/Dashboard.test.ts` « after 3 seconds » : `aria-label` égal au texte ; rouge avant (échec constaté en retirant la correction).

## Cause root
L'`aria-label` était fixe.

## Impacté
L'interface du client (revue UX du 2026-10-08), jamais publiée.

## Workaround
Aucun.

## Correction
`aria-label` calculé comme le texte. `// FIX:01M4D4Y22QGBWNKKJTK8K3X12A`.

## Règles
- Design : `contexts/hearth/conceptions/design-system-web.md` (fenêtre minimale 1 100 px, jetons existants).

## Non-régression
- Les tests ci-dessus.

## Références
- Ticket : HRT-34 (revue UX du 2026-10-08)
- Code : `pages/Dashboard.vue`
