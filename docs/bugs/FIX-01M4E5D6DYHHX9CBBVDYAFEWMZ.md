---
id: FIX-01M4E5D6DYHHX9CBBVDYAFEWMZ
titre: Bouton grisé : la raison seulement au survol (C42)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4E5D6DYHHX9CBBVDYAFEWMZ : Bouton grisé : la raison seulement au survol (C42)

## Symptôme
« Mettre à jour l'agent » grisé pour un compte en lecture seule, la raison n'apparaît qu'au survol.

## Reproduction
`atoms.test.ts` « la raison du blocage écrite sous le bouton ».

## Cause root
`HButton` n'offrait que l'infobulle.

## Impacté
`HButton` ; les cartes qui bloquent un bouton. Source : revue UX de Nora du 2026-10-08 (`contexts/hearth/art/ux-review-2026-10-08.md`).

## Workaround
Aucun.

## Correction
Nouvelle propriété `reason-below` : la raison est AUSSI écrite en texte permanent sous le bouton grisé. NON adoptée par la carte « État du serveur » (`AgentUpdateCard`, fichier réservé à la PR #56 en cours) : une ligne à ajouter après sa fusion.

## Règles
- Aucune règle métier touchée (interface).
