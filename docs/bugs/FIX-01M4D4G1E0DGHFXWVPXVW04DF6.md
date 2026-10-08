---
id: FIX-01M4D4G1E0DGHFXWVPXVW04DF6
titre: « Mes serveurs » : l'adresse recouverte par les étiquettes, barre de défilement au milieu de l'écran (C51)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4D4G1E0DGHFXWVPXVW04DF6 : « Mes serveurs » : l'adresse recouverte par les étiquettes, barre de défilement au milieu de l'écran (C51)

## Symptôme
« 192.168.1.120 » sous « Identifiants mémorisés » ; colonne de 576 px avec sa barre de défilement en plein centre.

## Reproduction
`e2e/layout.spec.ts` « Mes serveurs à 1100×680 et 1920×1080 » : aucun recouvrement entre éléments d'une ligne, défilement au bord de la fenêtre ; rouge avant.

## Cause root
La colonne du nom (`flex: 1`, base 0) se réduisait à zéro derrière les étiquettes ; le conteneur de défilement était la colonne centrée.

## Impacté
L'interface du client (revue UX du 2026-10-08), jamais publiée.

## Workaround
Aucun.

## Correction
La colonne du nom garde sa largeur de contenu (les étiquettes passent à la ligne) ; le conteneur de défilement est pleine largeur, la colonne centrée est à l'intérieur. `// FIX:01M4D4G1E0DGHFXWVPXVW04DF6`.

## Règles
- Design : `contexts/hearth/conceptions/design-system-web.md` (fenêtre minimale 1 100 px, jetons existants).

## Non-régression
- Les tests ci-dessus.

## Références
- Ticket : HRT-36 (revue UX du 2026-10-08)
- Code : `components/organisms/ServerRow.vue`, `pages/Servers.vue`
