---
id: FIX-01M4D1K5GKA4XPJH8391E1HWKZ
titre: Le tableau de bord ne tenait pas à 1280 px : courbes écrasées, cartes de hauteurs inégales, trous (C6, C7)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4D1K5GKA4XPJH8391E1HWKZ : Le tableau de bord ne tenait pas à 1280 px : courbes écrasées, cartes de hauteurs inégales, trous (C6, C7)

## Symptôme
À 1280×800 les courbes faisaient 100 à 190 px, une rangée et demie seulement se voyait ; à 1920 les cartes d'une rangée avaient des hauteurs différentes (trous sous Mémoire, Processeur, bas gauche vide) et Températures était sous le pli.

## Reproduction
`e2e/dashboard.spec.ts` « tableau de bord à {1100×680, 1280×800, 1366×800, 1920×1080, 2560×1440} » : aucun chevauchement, cartes d'une rangée de même hauteur, courbes d'au moins 150 px, l'essentiel visible sans défiler à 1920.

## Cause root
La grille basculait sur 6 colonnes selon la largeur de la FENÊTRE (1200 px) alors que la barre des serveurs et la navigation retirent plus de 270 px à la page ; les cases s'alignaient au début (`align-items: start`) ; Disques et Températures empilés (637 px) dépassaient la moitié basse.

## Impacté
Le tableau de bord (HRT-11), jamais publié.

## Workaround
Aucun.

## Correction
Grille par conteneur (`container: dash`) : 12 colonnes au-dessus de 1300 px de page, 6 en dessous (Machine + Processeur, Mémoire + Réseau, Carte graphique pleine largeur, Disques pleine largeur), 1 colonne sous 700 px ; cartes étirées (`align-items: stretch`, cases en colonne dont les cartes grandissent) ; Températures empilée sous Mémoire (choix : Disques + Températures ne tenaient pas sous le pli à 1920). `// FIX:01M4D1K5GKA4XPJH8391E1HWKZ`.

## Règles
- Design : grille 12 puis 6 colonnes (`contexts/hearth/conceptions/2026-10-04-design-ecrans-socle.md`).

## Non-régression
- Les tests ci-dessus.

## Références
- Ticket : HRT-34 (revue UX du 2026-10-08)
- Code : `apps/desktop/src/pages/Dashboard.vue`
