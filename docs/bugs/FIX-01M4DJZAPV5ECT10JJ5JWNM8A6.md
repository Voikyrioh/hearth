---
id: FIX-01M4DJZAPV5ECT10JJ5JWNM8A6
titre: Les bandeaux de sécurité étaient plus larges que la page, actions à l'autre bout (C35)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4DJZAPV5ECT10JJ5JWNM8A6 : Les bandeaux de sécurité étaient plus larges que la page, actions à l'autre bout (C35)

## Symptôme
Bandeau sur 1 600 px au-dessus de cartes de 880 px ; « Activer le mode attaque » (deux fois sur la page), « Voir la page Sécurité », « Plus d'infos » en petits textes à l'extrême droite.

## Reproduction
`e2e/hrt39-security.spec.ts` (largeur du bandeau au plus 880 px, alignée sur la carte, un seul bouton d'activation) ; rouge avant.

## Cause root
`SecurityBanner` s'étirait sur la fenêtre, son corps prenait toute la place (`flex: 1`), les actions étaient des boutons « fantôme ».

## Impacté
Page Sécurité du client Windows depuis HRT-26. Source : revue UX de Nora du 2026-10-08 (`contexts/hearth/art/ux-review-2026-10-08.md`).

## Workaround
Aucun.

## Correction
`max-width: var(--column-max)`, le corps ne grandit plus (l'action suit le message), « Plus d'infos » et « Voir la page Sécurité » deviennent des boutons à contour. Sur la page Sécurité le bandeau n'a plus de bouton d'activation : le seul est celui de la carte « Mode attaque ». `FIX:` dans `SecurityBanner.vue`.

## Règles
- BR-TRUST-010, 018, 028, 029 (mode attaque), BR-TRUST-008, 009 (alerte).

## Non-régression
- `hrt39-security.spec.ts` (1100, 1366, 1920, 2560), `SecurityMode.test.ts`.

## Références
- Ticket : HRT-39
