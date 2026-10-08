---
id: FIX-01M4D0RJB17YE0F26QFXGJ2WEV
titre: Un mot de passe de confirmation faux effaçait la saisie du nouveau compte
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4D0RJB17YE0F26QFXGJ2WEV : Un mot de passe de confirmation faux effaçait la saisie du nouveau compte

## Symptôme
Après « Mot de passe incorrect. » à la création d'un compte ou au changement de mot de passe, les champs du NOUVEAU mot de passe étaient vides (C18) ; la spec dit « sans perdre la saisie de l'acte ».

## Reproduction
Tests rouges avant correctif : `accounts.test.ts` « keeps the whole entry of the new account when the confirmation password is wrong », « asks for the old password then the new one… » ; `e2e/hrt33-keyboard.spec.ts`.

## Cause root
`CreateAccountDialog` et `PasswordDialog` ne gardaient la saisie que pour `password_required` ; tout autre refus la vidait.

## Impacté
Client Windows, écrans concernés, depuis leur livraison. Source : revue UX de Nora du 2026-10-08 (`contexts/hearth/art/ux-review-2026-10-08.md`).

## Workaround
Cliquer d'abord dans le bon champ.

## Correction
Un refus garde toute la saisie ; les mots de passe du nouveau compte ne sont vidés qu'après un envoi non refusé. La fenêtre vide, elle, le mot de passe de confirmation après chaque envoi. `FIX:` dans les deux fenêtres.

## Règles
- Aucune règle métier touchée (interface seulement).

## Non-régression
- `accounts.test.ts`, `e2e/accounts.spec.ts` (création refusée : formulaire conservé en entier), `e2e/hrt33-keyboard.spec.ts`.

## Références
- Ticket : HRT-33 (lot 1)
