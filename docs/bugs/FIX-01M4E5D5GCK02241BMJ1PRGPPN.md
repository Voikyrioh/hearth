---
id: FIX-01M4E5D5GCK02241BMJ1PRGPPN
titre: Reconnexion : la page se désature et se date dès la première seconde (C48)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4E5D5GCK02241BMJ1PRGPPN : Reconnexion : la page se désature et se date dès la première seconde (C48)

## Symptôme
Cartes grisées et « Vu il y a 7 s » dès le passage en « Reconnexion… », alors que la règle dit « pastille Reconnexion…, rien d'autre » sous 30 s.

## Reproduction
`shell.test.ts`, `Dashboard.test.ts`, `e2e/hrt38-40.spec.ts` (rouges avant).

## Cause root
Le gabarit rendait la page périmée pour tout état différent de « Connecté ».

## Impacté
Gabarit du serveur (toutes les pages). Source : revue UX de Nora du 2026-10-08 (`contexts/hearth/art/ux-review-2026-10-08.md`).

## Workaround
Aucun.

## Correction
La page ne s'estompe et ne se date qu'à partir de « Hors ligne », session expirée et accès révoqué ; pendant « Reconnexion… » (3 à 30 s) seule la pastille change.

## Règles
- BR-RESIL-007 mise à jour.
