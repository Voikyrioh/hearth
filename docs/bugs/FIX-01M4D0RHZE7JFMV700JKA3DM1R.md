---
id: FIX-01M4D0RHZE7JFMV700JKA3DM1R
titre: Les fenêtres d'actes d'administration s'ouvraient avec le curseur dans la confirmation
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4D0RHZE7JFMV700JKA3DM1R : Les fenêtres d'actes d'administration s'ouvraient avec le curseur dans la confirmation

## Symptôme
« Créer un compte » et les changements de mot de passe s'ouvraient avec le curseur dans le DERNIER champ (« Ton mot de passe », « Ancien mot de passe ») : l'identifiant tapé partait masqué dans la confirmation (C17). La règle du curseur initial changeait d'une fenêtre à l'autre (C56).

## Reproduction
Test rouge avant correctif : `accounts.test.ts` « puts the cursor in the identifier field… » et « puts the cursor in the new password field… » ; Playwright `e2e/hrt33-keyboard.spec.ts`.

## Cause root
`AdminActDialog` lit l'état de la confirmation avant d'afficher ses champs ; à ce moment le focus du `<dialog>` était sur un bouton, donc la garde « si le focus n'est dans aucun champ » envoyait le curseur dans la confirmation.

## Impacté
Client Windows, écrans concernés, depuis leur livraison. Source : revue UX de Nora du 2026-10-08 (`contexts/hearth/art/ux-review-2026-10-08.md`).

## Workaround
Cliquer d'abord dans le bon champ.

## Correction
Une fois l'état lu, le curseur va dans le premier champ de l'acte (`firstActField`, hors confirmation) ; une fenêtre sans champ d'acte met le curseur dans « Ton mot de passe » ; un curseur déjà dans un champ ne bouge pas. `FIX:` dans `AdminActDialog.vue`.

## Règles
- Aucune règle métier touchée (interface seulement).

## Non-régression
- `accounts.test.ts`, `e2e/hrt33-keyboard.spec.ts` (frappe réelle au clavier, Tab).

## Références
- Ticket : HRT-33 (lot 1)
