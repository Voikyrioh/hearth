---
id: FIX-01M4D0RJ5EMX3TG1TJB0EJ5EYP
titre: Assistant d'ajout : « Suivant » grisé sans raison et curseur nulle part
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4D0RJ5EMX3TG1TJB0EJ5EYP : Assistant d'ajout : « Suivant » grisé sans raison et curseur nulle part

## Symptôme
« Suivant » restait grisé tant que le nom était vide, sans dire pourquoi ; le champ avait l'air facultatif (« Forge (ou le nom de la machine) ») et Entrée ne faisait rien (C1). Aucun champ n'était actif aux étapes 1 et 3 (C2).

## Reproduction
Test rouge avant correctif : `connect.test.ts` « puts the cursor in the first field, and says what is missing… » ; Playwright `e2e/hrt33-keyboard.spec.ts`.

## Cause root
`canNext` désactivait le bouton au lieu de laisser la validation parler ; aucune étape ne posait le focus.

## Impacté
Client Windows, écrans concernés, depuis leur livraison. Source : revue UX de Nora du 2026-10-08 (`contexts/hearth/art/ux-review-2026-10-08.md`).

## Workaround
Cliquer d'abord dans le bon champ.

## Correction
« Suivant » n'est plus grisé (sauf pendant la vérification) : au clic ou à Entrée, les champs manquants ou faux sont dits sous leur champ et le curseur va au premier. Le curseur est dans le premier champ de chaque étape qui en a un. L'exemple du nom devient « Forge ». `FIX:` dans `AddServerWizard.vue`.

## Règles
- Aucune règle métier touchée (interface seulement).

## Non-régression
- `connect.test.ts`, `e2e/connect.spec.ts`, `e2e/hrt33-keyboard.spec.ts`.

## Références
- Ticket : HRT-33 (lot 1)
