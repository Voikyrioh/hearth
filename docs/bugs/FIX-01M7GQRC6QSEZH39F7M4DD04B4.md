---
id: FIX-01M7GQRC6QSEZH39F7M4DD04B4
titre: Les barres de progression sont en fuseau effilé au lieu d'une barre arrondie d'épaisseur constante (HRT-220)
date_découverte: 2026-11-16
date_correction: 2026-11-16
---

# FIX-01M7GQRC6QSEZH39F7M4DD04B4 : Barres de progression pointues (HRT-220)

Sévérité : mineur (visuel, toutes les jauges).

## Symptôme
Dans le panneau Transferts, la partie remplie d'une barre se termine en pointes effilées aux deux bouts.

## Reproduction
Ouvrir le panneau Transferts avec un transfert en cours ; la barre est large (plusieurs centaines de pixels) pour 8 px de haut.

## Cause root
`HMeter` était un SVG `viewBox="0 0 100 6"` en `preserveAspectRatio="none"` avec des `rect rx="3"`. Étiré à la largeur du conteneur, le SVG multiplie le rayon horizontalement (un facteur 4 à 8) sans toucher le vertical : les coins deviennent des ellipses très aplaties, donc des pointes.

## Impacté
`apps/desktop/src/components/atoms/HMeter.vue` (disques, transferts, opérations, étapes de déploiement, sauvegardes).

## Workaround
Aucun.

## Correction
`HMeter` devient du HTML : un conteneur `div` (`border-radius: var(--radius-pill)`, `overflow: hidden`, hauteur du jeton) et un remplissage `div` en `width: {ratio}%`, même rayon. Les classes (`meter__value--{level}`, `meter__slide`) et le contrat (`role="img"`, `aria-label`) sont inchangés. Alternative écartée : garder le SVG avec `vector-effect` ; il ne corrige pas un `rx` étiré.

## Références
- HRT-220
