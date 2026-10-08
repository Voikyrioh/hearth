---
id: FIX-01M4E5D447SRCNGAQY4PP006HC
titre: Hors ligne : trois messages et trois gestes pour la même information (C30)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4E5D447SRCNGAQY4PP006HC : Hors ligne : trois messages et trois gestes pour la même information (C30)

## Symptôme
Bandeau « Serveur hors ligne… Réessayer maintenant », puis « Périmé / Données périmées, serveur injoignable / Rechargement manuel », puis « Indisponible tant que le lien avec le serveur n'est pas établi. Réessayer ».

## Reproduction
`pages/audit.test.ts` « hors ligne : le bandeau du gabarit dit tout… » et `e2e/hrt38-40.spec.ts` (rouges avant).

## Cause root
La page Journal ajoutait son propre bloc « périmé » et son propre message d'échec, en plus du bandeau et de l'estampille du gabarit.

## Impacté
Page Journal d'activité. Source : revue UX de Nora du 2026-10-08 (`contexts/hearth/art/ux-review-2026-10-08.md`).

## Workaround
Aucun.

## Correction
Le bandeau du gabarit (« Réessayer maintenant ») et l'estampille du gabarit (« Vu il y a… ») disent tout ; le bloc « périmé » de la page est retiré et le message d'échec n'est pas montré tant que le lien n'est pas « Connecté ».

## Règles
- Aucune règle métier touchée (interface).
