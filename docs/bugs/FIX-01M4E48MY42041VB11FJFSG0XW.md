---
id: FIX-01M4E48MY42041VB11FJFSG0XW
titre: Port faux : « L'emplacement saisi n'est pas valide » (C4)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4E48MY42041VB11FJFSG0XW : Port faux : « L'emplacement saisi n'est pas valide » (C4)

## Symptôme
Un port hors plage affichait un message qui ne parlait pas de port et ne donnait pas la plage.

## Reproduction
`useAddServer.test.ts`, `layout.spec.ts` « port faux » (rouge avant).

## Cause root
Texte fixe `connect.portInvalid` sans plage.

## Impacté
Assistant d'ajout d'un serveur, étape 1. Source : revue UX de Nora du 2026-10-08 (`contexts/hearth/art/ux-review-2026-10-08.md`).

## Workaround
Aucun.

## Correction
« Port invalide. Plage admise : 1–65535 », avec un liant de mots (U+2060) pour que la plage ne se coupe pas en fin de ligne.

## Règles
- Aucune règle métier touchée (interface).
