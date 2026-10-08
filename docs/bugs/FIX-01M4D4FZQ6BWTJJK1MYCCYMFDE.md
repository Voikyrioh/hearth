---
id: FIX-01M4D4FZQ6BWTJJK1MYCCYMFDE
titre: Beaucoup de serveurs : le bas de la barre sortait de l'écran (C50)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4D4FZQ6BWTJJK1MYCCYMFDE : Beaucoup de serveurs : le bas de la barre sortait de l'écran (C50)

## Symptôme
Avec 14 serveurs à 1100×680, « + », « Mes serveurs » et « Réglages » inatteignables, barre sans défilement.

## Reproduction
`e2e/layout.spec.ts` « 14 serveurs à 1100×680 et 1280×800 » : les trois icônes dans la fenêtre, liste qui défile ; rouge avant.

## Cause root
Tous les liens de la barre étaient dans une colonne sans défilement.

## Impacté
L'interface du client (revue UX du 2026-10-08), jamais publiée.

## Workaround
Aucun.

## Correction
La liste des serveurs est un conteneur qui défile ; « + », « Mes serveurs » et « Réglages » restent fixes en bas. `// FIX:01M4D4FZQ6BWTJJK1MYCCYMFDE`.

## Règles
- Design : `contexts/hearth/conceptions/design-system-web.md` (fenêtre minimale 1 100 px, jetons existants).

## Non-régression
- Les tests ci-dessus.

## Références
- Ticket : HRT-36 (revue UX du 2026-10-08)
- Code : `components/organisms/ServerRail.vue`
