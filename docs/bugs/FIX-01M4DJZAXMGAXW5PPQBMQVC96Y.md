---
id: FIX-01M4DJZAXMGAXW5PPQBMQVC96Y
titre: La marque de sécurité recouvrait la deuxième lettre des initiales du serveur (C38)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4DJZAXMGAXW5PPQBMQVC96Y : La marque de sécurité recouvrait la deuxième lettre des initiales du serveur (C38)

## Symptôme
Le petit bouclier ou triangle de 16 px recouvrait la deuxième lettre (« FO » se lisait « FC »).

## Reproduction
`e2e/hrt39-security.spec.ts` « la marque de sécurité ne recouvre pas les initiales du serveur » (rectangles mesurés : 1 px de recouvrement avec 5 px de décalage, plus avec 8 px).

## Cause root
La marque était posée à 1 px du coin de l'avatar de 38 px.

## Impacté
Page Sécurité du client Windows depuis HRT-26. Source : revue UX de Nora du 2026-10-08 (`contexts/hearth/art/ux-review-2026-10-08.md`).

## Workaround
Aucun.

## Correction
Jeton `--mark-offset: -8px` : la marque dépasse du coin. `FIX:` dans `ServerAvatar.vue`.

## Règles
- Aucune règle métier touchée (interface).

## Non-régression
- Le test Playwright ci-dessus.

## Références
- Ticket : HRT-39
