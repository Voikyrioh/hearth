---
id: FIX-01M4D4FSDN82D1PNC7YQEERDNB
titre: Page Comptes vide : « Comptes » deux fois et « Ajouter un compte » deux fois (C25)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4D4FSDN82D1PNC7YQEERDNB : Page Comptes vide : « Comptes » deux fois et « Ajouter un compte » deux fois (C25)

## Symptôme
Titre de page « Comptes » et titre de carte « Comptes », deux boutons identiques.

## Reproduction
`e2e/layout.spec.ts` « comptes vide » ; `pages/Accounts.test.ts` (un seul bouton) ; rouge avant (2 boutons).

## Cause root
L'écran vide reprenait le titre de la page et portait son propre bouton alors que l'en-tête a déjà le sien.

## Impacté
L'interface du client (revue UX du 2026-10-08), jamais publiée.

## Workaround
Aucun.

## Correction
Titre propre (« Aucun compte à afficher »), texte qui renvoie au bouton de l'en-tête, plus de bouton dans l'écran vide. `// FIX:01M4D4FSDN82D1PNC7YQEERDNB`.

## Règles
- Design : `contexts/hearth/conceptions/design-system-web.md` (fenêtre minimale 1 100 px, jetons existants).

## Non-régression
- Les tests ci-dessus.

## Références
- Ticket : HRT-35 (revue UX du 2026-10-08)
- Code : `pages/Accounts.vue`, `i18n/fr.ts`
