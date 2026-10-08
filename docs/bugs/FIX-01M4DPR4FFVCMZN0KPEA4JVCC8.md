---
id: FIX-01M4DPR4FFVCMZN0KPEA4JVCC8
titre: Nombres à l'anglaise et unités brutes (C12)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4DPR4FFVCMZN0KPEA4JVCC8 : Nombres à l'anglaise et unités brutes (C12)

## Symptôme
« 64.0 Go », « 4.5 GHz », « 4000.0 Go », « Libre : 3280.0 Go ».

## Reproduction
`dashboard/format.test.ts` (virgule, sans « ,0 », bornes 1 023 Go, 1 024 Go, 4 096 Go, 4 × 10¹² octets), `layout.spec.ts` « virgule » (rouges avant).

## Cause root
Formats en `toFixed(1)` : point décimal, « ,0 » inutile, aucune unité au-delà de 1 000 Go.

## Impacté
Tableau de bord (mémoire, disques, fréquence, débits). Source : revue UX de Nora du 2026-10-08 (`contexts/hearth/art/ux-review-2026-10-08.md`).

## Workaround
Aucun.

## Correction
`decimal()` (virgule, sans « ,0 »), un seul formateur de quantités `formatGb`, tout en binaire (1 To = 1 024 Go, bascule à 1 024 Go arrondis). Décision de Claude du 2026-10-08, à confirmer par Voiky.

## Règles
- BR-DASH-014 mise à jour (formats d'unités).
