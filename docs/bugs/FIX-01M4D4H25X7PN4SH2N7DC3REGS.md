---
id: FIX-01M4D4H25X7PN4SH2N7DC3REGS
titre: Les filtres du journal prenaient le quart de l'écran à 1280 et 1366 (C29)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4D4H25X7PN4SH2N7DC3REGS : Les filtres du journal prenaient le quart de l'écran à 1280 et 1366 (C29)

## Symptôme
Carte de filtres de 190 px, le bouton seul sur sa ligne.

## Reproduction
`e2e/layout.spec.ts` « journal à 1280 et 1366 » : filtres sur une ligne, bouton compris (au plus 150 px) ; rouge avant.

## Cause root
Deux rangées empilées dont les champs passaient à la ligne avant de rétrécir.

## Impacté
L'interface du client (revue UX du 2026-10-08), jamais publiée.

## Workaround
Aucun.

## Correction
Une seule rangée qui s'étire (les champs rétrécissent avant de passer à la ligne), titre sur toute la largeur. `// FIX:01M4D4H25X7PN4SH2N7DC3REGS`.

## Règles
- Design : `contexts/hearth/conceptions/design-system-web.md` (fenêtre minimale 1 100 px, jetons existants).

## Non-régression
- Les tests ci-dessus.

## Références
- Ticket : HRT-36 (revue UX du 2026-10-08)
- Code : `components/organisms/AuditFilters.vue`
