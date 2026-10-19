---
id: FIX-01M5A5VKBEZ7XCZ2P0CD2VXV9M
titre: L'étiquette « toi » de l'écran Comptes tutoyait l'utilisateur (HRT-65)
date_découverte: 2026-10-19
date_correction: 2026-10-19
---

# FIX-01M5A5VKBEZ7XCZ2P0CD2VXV9M : L'étiquette « toi » de l'écran Comptes tutoyait l'utilisateur (HRT-65)

## Symptôme
La ligne du compte connecté, sur l'écran Comptes, portait l'étiquette « toi ».

## Reproduction
`apps/desktop/src/components/organisms/accounts.test.ts`, `src/pages/AccountsReview.test.ts`, `e2e/accounts.spec.ts`.

## Cause root
Texte `accounts.you` écrit au tutoiement, contraire à la règle de copy de Hearth (vouvoiement).

## Impacté
Écran Comptes uniquement.

## Workaround
Aucun.

## Correction
`you: "vous"` dans `src/i18n/fr.ts` ; tests mis à jour. Les autres textes au tutoiement de ce fichier ne sont pas traités ici.

## Références
- Ticket : HRT-65, PR #108 ; revue UX Nora, « Vu en passant »
