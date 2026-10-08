---
id: FIX-01M4ECZJVHHJA1MNQ2E4FSFJ0K
titre: « Un poste à moitié reconnu » : jargon sur la page Sécurité (C34, reste)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4ECZJVHHJA1MNQ2E4FSFJ0K : « Un poste à moitié reconnu » : jargon sur la page Sécurité (C34, reste)

## Symptôme
Deux phrases de la carte du mode attaque parlaient d'« un poste à moitié reconnu » qui « a droit à un seul essai ».

## Reproduction
Vitest `i18n/plainLanguage.test.ts`. Rouge avant : la tournure était dans les textes.

## Cause root
Reprise littérale du vocabulaire de la règle (un poste connu par un seul critère).

## Impacté
L'interface du client (revue UX du 2026-10-08), jamais publiée.

## Workaround
Aucun.

## Correction
« Un poste que le serveur ne connaît que par son adresse n'aurait (aura) droit qu'à un seul essai » ; le sens de BR-TRUST-012 ne change pas. `// FIX:01M4ECZJVHHJA1MNQ2E4FSFJ0K`.

## Règles
- Aucune règle métier modifiée.

## Non-régression
- Les tests ci-dessus.

## Références
- Tickets : HRT-39, HRT-40, HRT-41, HRT-43 (revue UX du 2026-10-08)
- Code : `i18n/fr.ts`
