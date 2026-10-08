---
id: FIX-01M4DJZB43SA08NE46DGEZK213
titre: « Plus d'infos » ne donnait aucune information sur l'alerte (C36)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4DJZB43SA08NE46DGEZK213 : « Plus d'infos » ne donnait aucune information sur l'alerte (C36)

## Symptôme
La page Sécurité ne disait rien de l'attaque : ni depuis quand, ni combien de comptes visés.

## Reproduction
`SecurityMode.test.ts` « tells the alert on the page… » ; `hrt39-security.spec.ts` « Plus d'infos mène à la carte… ».

## Cause root
Aucune carte ne portait ce que l'agent sait d'une alerte.

## Impacté
Page Sécurité du client Windows depuis HRT-26. Source : revue UX de Nora du 2026-10-08 (`contexts/hearth/art/ux-review-2026-10-08.md`).

## Workaround
Aucun.

## Correction
Carte `SecurityAlertCard` « Ce qui se passe » : ton identifiant est visé (depuis quand), combien d'AUTRES comptes (administrateur seulement, jamais leurs noms, lien vers le journal d'activité), quoi faire. L'agent ne livre ni adresse ni nombre d'essais : la carte ne les invente pas. `FIX:` dans `SecurityAlertCard.vue`.

## Règles
- BR-TRUST-010, 018, 028, 029 (mode attaque), BR-TRUST-008, 009 (alerte).

## Non-régression
- Les deux tests ci-dessus.

## Références
- Ticket : HRT-39
