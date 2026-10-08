---
id: FIX-01M4D4Y0D7YFWCV6R1GEHXZGPB
titre: Tableau de bord : jauges perdues dans un grand vide à 1920 et 2560 (retour de revue HRT-34)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4D4Y0D7YFWCV6R1GEHXZGPB : Tableau de bord : jauges perdues dans un grand vide à 1920 et 2560 (retour de revue HRT-34)

## Symptôme
Mémoire : jauge en haut à gauche, 144 px de vide dessous ; Carte graphique : tiers bas vide.

## Reproduction
`e2e/dashboard.spec.ts` « tableau de bord à … » : vide au-dessus/au-dessous d'une jauge au plus 80 px (mesuré sur le cadran et la légende, pas sur la boîte étirée) ; rouge avant (Mémoire 144 px) ; le point de montage a la taille du nom du disque (rouge avant : 12 px).

## Cause root
Les jauges restaient en haut de leur rangée étirée ; Disques + Températures empilés fixaient la hauteur de la rangée.

## Impacté
L'interface du client (revue UX du 2026-10-08), jamais publiée.

## Workaround
Aucun.

## Correction
Jauges centrées dans leur rangée, courbe qui prend la hauteur ; Carte graphique + Températures empilées (4 colonnes) à côté de Réseau (4) et Disques (4) ; point de montage à la taille du texte. `// FIX:01M4D4Y0D7YFWCV6R1GEHXZGPB`.

## Règles
- Design : `contexts/hearth/conceptions/design-system-web.md` (fenêtre minimale 1 100 px, jetons existants).

## Non-régression
- Les tests ci-dessus.

## Références
- Ticket : HRT-34 (revue UX du 2026-10-08)
- Code : `pages/Dashboard.vue`, `organisms/MemoryCard.vue`, `GpuPanel.vue`, `DisksCard.vue`
