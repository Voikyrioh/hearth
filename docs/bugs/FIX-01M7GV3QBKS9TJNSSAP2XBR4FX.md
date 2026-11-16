---
id: FIX-01M7GV3QBKS9TJNSSAP2XBR4FX
titre: La page de fiche du catalogue ne remplissait pas la largeur, mêlait le technique et l'utile, et ses deux boutons étaient incompréhensibles (HRT-221)
date_découverte: 2026-11-16
date_correction: 2026-11-16
---

# FIX-01M7GV3QBKS9TJNSSAP2XBR4FX : Page de fiche du catalogue (HRT-221)

Sévérité : mineur (lisibilité).

## Symptôme
Sur un écran de 1800 px, le contenu tient dans 880 px. Image, empreinte et registre passent avant ce qui compte ; ports, dossiers et ressources sont au même niveau ; « Installer » et « Installer avec les valeurs proposées » ne disent pas ce qu'ils font.

## Reproduction
Ouvrir la fiche d'un jeu du catalogue sur un grand écran.

## Cause root
La page était bornée par `--column-max` ; `FicheSheet` empilait toutes les cartes dans l'ordre de la fiche ; les libellés des boutons reprenaient le vocabulaire du code.

## Impacté
`pages/FicheDetail.vue`, `components/organisms/FicheSheet.vue`, `i18n/fr.ts`, `styles/tokens.css`.

## Workaround
Aucun.

## Correction
Largeur pleine ; grille de cartes (`--sheet-col-min`) ; trois groupes (ce qu'il faut, réglages, « Détails techniques » replié avec Image, Dossiers, empreinte) ; boutons « Installer… » et « Installation rapide », une phrase sous chacun.

## Références
- HRT-221
